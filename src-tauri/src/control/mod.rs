//! Typed application control façade shared by Tauri commands and future MCP tools.

pub mod events;
pub mod state;

use crate::conn::{ConnConfig, ConnManager, RxObserver, StatusObserver};
use crate::mcp::{ActionResult, MAX_SEND_BYTES};
use events::{ActionEvent, ActionEventLog, ActionOrigin, ActionStage};
use serde::{Deserialize, Serialize};
use state::{
    ConnectionConfigSummary, ConnectionSnapshot, ConnectionStatus, ControlState, RxReadError,
    RxReadResult, RxRingBuffer, RxWaitError, DEFAULT_RX_MAX_BYTES, DEFAULT_RX_MAX_RECORDS,
};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};

pub use events::{ActionOrigin as ControlActionOrigin, ActionStage as ControlActionStage};
pub use state::{
    ConnectionConfigSummary as ControlConnectionConfigSummary,
    ConnectionSnapshot as ControlConnectionSnapshot, ConnectionStatus as ControlConnectionStatus,
    RxReadError as ControlRxReadError, RxReadResult as ControlRxReadResult,
    RxRecord as ControlRxRecord, RxRingBuffer as ControlRxRingBuffer,
    RxWaitError as ControlRxWaitError, TrafficStats as ControlTrafficStats,
};

const ACTION_ID_COUNTER_START: u64 = 1;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceState {
    pub connection: ConnectionSnapshot,
    pub rx_buffer_records: usize,
    pub rx_buffer_bytes: usize,
}

pub struct AppControlService {
    manager: Arc<ConnManager>,
    state: Arc<ControlState>,
    rx_buffer: Arc<RxRingBuffer>,
    action_events: Arc<ActionEventLog>,
    action_counter: AtomicU64,
}

impl Default for AppControlService {
    fn default() -> Self {
        Self::new()
    }
}

impl AppControlService {
    pub fn new() -> Self {
        Self::with_manager(Arc::new(ConnManager::new()))
    }

    pub fn with_manager(manager: Arc<ConnManager>) -> Self {
        let state = Arc::new(ControlState::default());
        let rx_buffer = Arc::new(RxRingBuffer::new(
            DEFAULT_RX_MAX_RECORDS,
            DEFAULT_RX_MAX_BYTES,
        ));
        let action_events = Arc::new(ActionEventLog::default());

        let rx_state = state.clone();
        let rx_ring = rx_buffer.clone();
        let rx_observer: RxObserver = Arc::new(move |payload| {
            rx_state.record_rx(payload.data.len());
            rx_ring.push_payload(payload);
        });
        let status_state = state.clone();
        let status_observer: StatusObserver = Arc::new(move |status, _message| {
            let status = match status {
                "connected" => ConnectionStatus::Connected,
                "connecting" => ConnectionStatus::Connecting,
                "lose" => ConnectionStatus::Lose,
                _ => ConnectionStatus::Closed,
            };
            status_state.set_status(status);
        });
        manager.set_observers(Some(rx_observer), Some(status_observer));

        Self {
            manager,
            state,
            rx_buffer,
            action_events,
            action_counter: AtomicU64::new(ACTION_ID_COUNTER_START),
        }
    }

    pub fn manager(&self) -> &Arc<ConnManager> {
        &self.manager
    }

    pub fn rx_buffer(&self) -> &Arc<RxRingBuffer> {
        &self.rx_buffer
    }

    pub fn snapshot(&self) -> ConnectionSnapshot {
        self.state.snapshot()
    }

    pub fn state(&self) -> ServiceState {
        ServiceState {
            connection: self.snapshot(),
            rx_buffer_records: self.rx_buffer.len(),
            rx_buffer_bytes: self.rx_buffer.stored_bytes(),
        }
    }

    pub fn action_events(&self) -> Vec<ActionEvent> {
        self.action_events.snapshot()
    }

    pub fn list_state(&self) -> ConnectionSnapshot {
        self.snapshot()
    }

    pub fn open<R: Runtime>(
        &self,
        cfg: ConnConfig,
        app: AppHandle<R>,
        origin: ActionOrigin,
    ) -> Result<ActionResult<()>, String> {
        let summary = ConnectionConfigSummary::from(&cfg);
        self.state.set_config(Some(summary));
        self.state.set_status(ConnectionStatus::Connecting);
        let manager = self.manager.clone();
        let open_app = app.clone();
        let result = self.run_action(&app, origin, "open", "打开连接", move || {
            manager.open(cfg, open_app)
        });
        match result {
            Ok(result) => {
                self.state.set_status(ConnectionStatus::Connected);
                Ok(result)
            }
            Err(error) => {
                self.state.set_status(ConnectionStatus::Closed);
                Err(error)
            }
        }
    }

    pub fn close<R: Runtime>(
        &self,
        app: AppHandle<R>,
        origin: ActionOrigin,
    ) -> Result<ActionResult<()>, String> {
        let manager = self.manager.clone();
        let result = self.run_action(&app, origin, "close", "关闭连接", move || {
            manager.close();
            Ok(())
        });
        self.state.set_status(ConnectionStatus::Closed);
        result
    }

    pub fn send<R: Runtime>(
        &self,
        data: Vec<u8>,
        app: AppHandle<R>,
        origin: ActionOrigin,
    ) -> Result<ActionResult<usize>, String> {
        if data.len() > MAX_SEND_BYTES {
            return Err(format!(
                "单次发送不能超过 {} MiB",
                MAX_SEND_BYTES / (1024 * 1024)
            ));
        }
        let manager = self.manager.clone();
        let result = self.run_action(&app, origin, "send", "发送数据", move || {
            let queue = manager.send_queue_lock();
            let _queue_guard = queue.lock().map_err(|_| "发送队列锁异常".to_string())?;
            manager.send(&data)
        });
        if let Ok(action) = &result {
            self.state.record_tx(action.result);
        }
        result
    }

    pub fn read_received(
        &self,
        cursor: u64,
        limit: usize,
        max_bytes: usize,
    ) -> Result<RxReadResult, String> {
        self.rx_buffer
            .read(cursor, limit, max_bytes)
            .map_err(format_read_error)
    }

    /// Waits for data after the current tail. Use `wait_for_data_from` when a
    /// caller has its own cursor and must avoid missing data between calls.
    pub fn wait_for_data(
        &self,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<RxReadResult, String> {
        let cursor = self.rx_buffer.latest_cursor();
        self.wait_for_data_from(cursor, timeout, max_bytes)
    }

    pub fn wait_for_data_from(
        &self,
        cursor: u64,
        timeout: Duration,
        max_bytes: usize,
    ) -> Result<RxReadResult, String> {
        self.rx_buffer
            .wait_for_data(cursor, max_bytes, timeout)
            .map_err(format_wait_error)
    }

    pub fn clear_received<R: Runtime>(
        &self,
        app: AppHandle<R>,
        origin: ActionOrigin,
    ) -> Result<ActionResult<()>, String> {
        let rx_buffer = self.rx_buffer.clone();
        let state = self.state.clone();
        self.run_action(
            &app,
            origin,
            "clear_received",
            "清空接收缓冲",
            move || {
                rx_buffer.clear();
                state.reset_stats();
                Ok(())
            },
        )
    }

    fn next_action_id(&self) -> String {
        let sequence = self.action_counter.fetch_add(1, Ordering::Relaxed);
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("action-{}-{}", millis, sequence)
    }

    fn publish<R: Runtime>(&self, app: &AppHandle<R>, event: ActionEvent) {
        self.action_events.push(event.clone());
        let _ = app.emit("control-action", event);
    }

    fn run_action<R, T, F>(
        &self,
        app: &AppHandle<R>,
        origin: ActionOrigin,
        operation: &str,
        label: &str,
        operation_fn: F,
    ) -> Result<ActionResult<T>, String>
    where
        R: Runtime,
        T: Serialize,
        F: FnOnce() -> Result<T, String>,
    {
        let action_id = self.next_action_id();
        self.publish(
            app,
            ActionEvent {
                action_id: action_id.clone(),
                origin: origin.clone(),
                operation: operation.into(),
                stage: ActionStage::Started,
                summary: format!("{}开始", label),
            },
        );
        match operation_fn() {
            Ok(result) => {
                self.publish(
                    app,
                    ActionEvent {
                        action_id: action_id.clone(),
                        origin,
                        operation: operation.into(),
                        stage: ActionStage::Finished,
                        summary: format!("{}完成", label),
                    },
                );
                Ok(ActionResult {
                    action_id,
                    summary: format!("{}完成", label),
                    result,
                })
            }
            Err(error) => {
                self.publish(
                    app,
                    ActionEvent {
                        action_id: action_id.clone(),
                        origin,
                        operation: operation.into(),
                        stage: ActionStage::Failed,
                        summary: format!("{}失败: {}", label, summarize_error(&error)),
                    },
                );
                Err(format!("{} [{}]: {}", label, action_id, error))
            }
        }
    }
}

fn summarize_error(error: &str) -> String {
    const MAX_SUMMARY_CHARS: usize = 160;
    error.chars().take(MAX_SUMMARY_CHARS).collect()
}

fn format_read_error(error: RxReadError) -> String {
    match error {
        RxReadError::CursorExpired { requested, oldest } => {
            format!(
                "接收 cursor 已过期: requested={}, oldest={}",
                requested, oldest
            )
        }
        RxReadError::InvalidLimit => "读取限制必须大于 0".into(),
    }
}

fn format_wait_error(error: RxWaitError) -> String {
    match error {
        RxWaitError::Timeout => "等待接收数据超时".into(),
        RxWaitError::CursorExpired { requested, oldest } => {
            format!(
                "接收 cursor 已过期: requested={}, oldest={}",
                requested, oldest
            )
        }
        RxWaitError::InvalidMaxBytes => "max_bytes 必须大于 0".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conn::SerialConfig;
    use crate::control::state::TrafficStats;

    fn app() -> tauri::AppHandle<tauri::test::MockRuntime> {
        tauri::test::mock_app().handle().clone()
    }

    #[test]
    fn write_actions_publish_unique_started_and_finished_events() {
        let service = AppControlService::new();
        let first = service.clear_received(app(), ActionOrigin::Ui).unwrap();
        let second = service.clear_received(app(), ActionOrigin::Mcp).unwrap();
        assert_ne!(first.action_id, second.action_id);
        let events = service.action_events();
        assert_eq!(events.len(), 4);
        assert_eq!(events[0].stage, ActionStage::Started);
        assert_eq!(events[1].stage, ActionStage::Finished);
        assert_eq!(events[2].origin, ActionOrigin::Mcp);
    }

    #[test]
    fn clear_received_resets_buffer_and_traffic_stats() {
        let service = AppControlService::new();
        service.rx_buffer.push(b"rx", 1.0, None);
        service.state.record_rx(2);
        service.state.record_tx(3);
        service
            .clear_received(app(), ActionOrigin::Ui)
            .expect("clear failed");

        assert_eq!(service.rx_buffer.len(), 0);
        assert_eq!(service.snapshot().stats, TrafficStats::default());
    }

    #[test]
    fn open_failure_publishes_failed_action_and_resets_state() {
        let service = AppControlService::new();
        let result = service.open(
            ConnConfig::Serial(SerialConfig::default()),
            app(),
            ActionOrigin::Ui,
        );
        assert!(result.is_err());
        assert_eq!(service.snapshot().status, ConnectionStatus::Closed);
        assert_eq!(
            service.action_events().last().unwrap().stage,
            ActionStage::Failed
        );
    }

    #[test]
    fn send_queue_serializes_operations() {
        let service = Arc::new(AppControlService::new());
        let queue = service.manager().send_queue_lock();
        let first = queue.lock().unwrap();
        let service2 = service.clone();
        let handle = std::thread::spawn(move || {
            service2.send(vec![1], app(), ActionOrigin::Ui).unwrap_err()
        });
        std::thread::sleep(Duration::from_millis(10));
        assert!(!handle.is_finished());
        drop(first);
        assert!(handle.join().unwrap().contains("发送数据"));
    }
}
