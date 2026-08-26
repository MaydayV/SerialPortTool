//! Shared control state and the bounded receive buffer.

use crate::conn::{ConnConfig, RxPayload};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

pub const DEFAULT_RX_MAX_RECORDS: usize = 4096;
pub const DEFAULT_RX_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionStatus {
    Connected,
    Closed,
    Connecting,
    Lose,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ConnectionConfigSummary {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baudrate: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_port: Option<u16>,
}

impl From<&ConnConfig> for ConnectionConfigSummary {
    fn from(config: &ConnConfig) -> Self {
        match config {
            ConnConfig::Serial(config) => Self {
                kind: "serial".into(),
                endpoint: Some(config.port.clone()),
                baudrate: Some(config.baudrate),
                ..Self::default()
            },
            ConnConfig::TcpUdp(config) => Self {
                kind: config.protocol.clone(),
                endpoint: Some(config.target.clone()),
                mode: Some(config.mode.clone()),
                port: Some(config.port.to_string()),
                local_port: Some(config.local_port),
                ..Self::default()
            },
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TrafficStats {
    pub rx_records: u64,
    pub rx_bytes: u64,
    pub tx_records: u64,
    pub tx_bytes: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionSnapshot {
    pub status: ConnectionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ConnectionConfigSummary>,
    pub stats: TrafficStats,
}

impl Default for ConnectionSnapshot {
    fn default() -> Self {
        Self {
            status: ConnectionStatus::Closed,
            config: None,
            stats: TrafficStats::default(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RxRecord {
    /// Monotonically increasing cursor. A read cursor returns records strictly after it.
    pub cursor: u64,
    pub ts: f64,
    pub data: Vec<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer: Option<String>,
}

// f64 cannot derive Eq, but an RX record is still exactly comparable in tests and API code.
impl PartialEq for RxRecord {
    fn eq(&self, other: &Self) -> bool {
        self.cursor == other.cursor
            && self.ts.to_bits() == other.ts.to_bits()
            && self.data == other.data
            && self.peer == other.peer
    }
}
impl Eq for RxRecord {}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RxReadResult {
    pub records: Vec<RxRecord>,
    pub next_cursor: u64,
    pub oldest_cursor: u64,
    pub has_more: bool,
    pub bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RxReadError {
    CursorExpired { requested: u64, oldest: u64 },
    InvalidLimit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RxWaitError {
    Timeout,
    CursorExpired { requested: u64, oldest: u64 },
    InvalidMaxBytes,
}

struct RxBufferInner {
    records: VecDeque<RxRecord>,
    bytes: usize,
    next_cursor: u64,
}

/// A bounded, process-local receive log. The condition variable lets waiters sleep
/// without polling, while the record and byte caps prevent an unattended MCP client
/// from growing memory indefinitely.
pub struct RxRingBuffer {
    inner: Mutex<RxBufferInner>,
    changed: Condvar,
    max_records: usize,
    max_bytes: usize,
}

impl RxRingBuffer {
    pub fn new(max_records: usize, max_bytes: usize) -> Self {
        assert!(max_records > 0, "RX ring buffer needs at least one record");
        assert!(max_bytes > 0, "RX ring buffer needs at least one byte");
        Self {
            inner: Mutex::new(RxBufferInner {
                records: VecDeque::new(),
                bytes: 0,
                next_cursor: 1,
            }),
            changed: Condvar::new(),
            max_records,
            max_bytes,
        }
    }

    pub fn push(&self, data: &[u8], ts: f64, peer: Option<String>) -> u64 {
        let mut stored = data.to_vec();
        if stored.len() > self.max_bytes {
            stored.truncate(self.max_bytes);
        }
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        let cursor = inner.next_cursor;
        inner.next_cursor = inner.next_cursor.saturating_add(1);
        inner.bytes += stored.len();
        inner.records.push_back(RxRecord {
            cursor,
            ts,
            data: stored,
            peer,
        });
        while inner.records.len() > self.max_records || inner.bytes > self.max_bytes {
            if let Some(record) = inner.records.pop_front() {
                inner.bytes = inner.bytes.saturating_sub(record.data.len());
            }
        }
        self.changed.notify_all();
        cursor
    }

    pub fn push_payload(&self, payload: RxPayload) -> u64 {
        self.push(&payload.data, payload.ts, payload.peer)
    }

    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        inner.records.clear();
        inner.bytes = 0;
        self.changed.notify_all();
    }

    pub fn len(&self) -> usize {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .records
            .len()
    }

    pub fn stored_bytes(&self) -> usize {
        self.inner.lock().unwrap_or_else(|p| p.into_inner()).bytes
    }

    pub fn latest_cursor(&self) -> u64 {
        self.inner
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .next_cursor
            .saturating_sub(1)
    }

    pub fn read(
        &self,
        cursor: u64,
        limit: usize,
        max_bytes: usize,
    ) -> Result<RxReadResult, RxReadError> {
        if limit == 0 || max_bytes == 0 {
            return Err(RxReadError::InvalidLimit);
        }
        let inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        Self::read_locked(&inner, cursor, limit, max_bytes)
    }

    fn read_locked(
        inner: &RxBufferInner,
        cursor: u64,
        limit: usize,
        max_bytes: usize,
    ) -> Result<RxReadResult, RxReadError> {
        let oldest_cursor = inner
            .records
            .front()
            .map(|r| r.cursor)
            .unwrap_or_else(|| inner.next_cursor);
        if !inner.records.is_empty() && cursor.saturating_add(1) < oldest_cursor {
            return Err(RxReadError::CursorExpired {
                requested: cursor,
                oldest: oldest_cursor,
            });
        }

        let mut records = Vec::new();
        let mut bytes = 0;
        for record in inner.records.iter().filter(|record| record.cursor > cursor) {
            if records.len() >= limit {
                break;
            }
            if !records.is_empty() && bytes + record.data.len() > max_bytes {
                break;
            }
            let mut record = record.clone();
            if records.is_empty() && record.data.len() > max_bytes {
                record.data.truncate(max_bytes);
            }
            bytes += record.data.len();
            records.push(record);
        }
        let next_cursor = records.last().map(|r| r.cursor).unwrap_or(cursor);
        let has_more = inner.records.iter().any(|r| r.cursor > next_cursor);
        Ok(RxReadResult {
            records,
            next_cursor,
            oldest_cursor,
            has_more,
            bytes,
        })
    }

    pub fn wait_for_data(
        &self,
        cursor: u64,
        max_bytes: usize,
        timeout: Duration,
    ) -> Result<RxReadResult, RxWaitError> {
        if max_bytes == 0 {
            return Err(RxWaitError::InvalidMaxBytes);
        }
        let deadline = Instant::now() + timeout;
        let mut inner = self.inner.lock().unwrap_or_else(|p| p.into_inner());
        loop {
            match Self::read_locked(&inner, cursor, usize::MAX, max_bytes) {
                Ok(result) if !result.records.is_empty() => return Ok(result),
                Err(RxReadError::CursorExpired { requested, oldest }) => {
                    return Err(RxWaitError::CursorExpired { requested, oldest })
                }
                Ok(_) => {}
                Err(RxReadError::InvalidLimit) => unreachable!(),
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(RxWaitError::Timeout);
            }
            let (guard, wait_result) = self
                .changed
                .wait_timeout(inner, remaining)
                .unwrap_or_else(|p| p.into_inner());
            inner = guard;
            if wait_result.timed_out() {
                return Err(RxWaitError::Timeout);
            }
        }
    }
}

#[derive(Default)]
pub struct ControlState {
    snapshot: Mutex<ConnectionSnapshot>,
}

impl ControlState {
    pub fn snapshot(&self) -> ConnectionSnapshot {
        self.snapshot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .clone()
    }

    pub fn set_config(&self, config: Option<ConnectionConfigSummary>) {
        self.snapshot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .config = config;
    }

    pub fn set_status(&self, status: ConnectionStatus) {
        self.snapshot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .status = status;
    }

    pub fn record_rx(&self, bytes: usize) {
        let mut snapshot = self.snapshot.lock().unwrap_or_else(|p| p.into_inner());
        snapshot.stats.rx_records = snapshot.stats.rx_records.saturating_add(1);
        snapshot.stats.rx_bytes = snapshot.stats.rx_bytes.saturating_add(bytes as u64);
    }

    pub fn record_tx(&self, bytes: usize) {
        let mut snapshot = self.snapshot.lock().unwrap_or_else(|p| p.into_inner());
        snapshot.stats.tx_records = snapshot.stats.tx_records.saturating_add(1);
        snapshot.stats.tx_bytes = snapshot.stats.tx_bytes.saturating_add(bytes as u64);
    }

    pub fn reset_stats(&self) {
        self.snapshot
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .stats = TrafficStats::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(data: &[u8], cursor: u64) -> RxRecord {
        RxRecord {
            cursor,
            ts: cursor as f64,
            data: data.to_vec(),
            peer: None,
        }
    }

    #[test]
    fn ring_is_bounded_and_reports_expired_cursor() {
        let ring = RxRingBuffer::new(2, 5);
        ring.push(b"ab", 1.0, None);
        ring.push(b"cd", 2.0, None);
        ring.push(b"ef", 3.0, None);
        assert_eq!(ring.len(), 2);
        assert_eq!(ring.stored_bytes(), 4);
        assert_eq!(
            ring.read(0, 2, 5),
            Err(RxReadError::CursorExpired {
                requested: 0,
                oldest: 2
            })
        );
        assert_eq!(
            ring.read(1, 2, 5).unwrap().records,
            vec![record(b"cd", 2), record(b"ef", 3)]
        );
    }

    #[test]
    fn read_obeys_record_and_byte_limits() {
        let ring = RxRingBuffer::new(10, 100);
        ring.push(b"abc", 1.0, None);
        ring.push(b"de", 2.0, None);
        let result = ring.read(0, 1, 10).unwrap();
        assert_eq!(result.records.len(), 1);
        assert!(result.has_more);
        let result = ring.read(0, 10, 3).unwrap();
        assert_eq!(result.records[0].data, b"abc");
        assert!(result.has_more);
    }

    #[test]
    fn wait_succeeds_and_times_out() {
        let ring = std::sync::Arc::new(RxRingBuffer::new(10, 100));
        let producer = ring.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(10));
            producer.push(b"hello", 1.0, Some("peer".into()));
        });
        let result = ring.wait_for_data(0, 3, Duration::from_secs(1)).unwrap();
        assert_eq!(result.bytes, 3);
        assert_eq!(result.records[0].data, b"hel");
        assert_eq!(
            ring.wait_for_data(1, 10, Duration::from_millis(10)),
            Err(RxWaitError::Timeout)
        );
    }
}
