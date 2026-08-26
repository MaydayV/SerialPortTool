//! Typed application control façade shared by Tauri commands and future MCP tools.

pub mod events;
pub mod state;

use crate::conn::{ConnConfig, ConnManager, RxObserver, StatusObserver};
use crate::mcp::PermissionMode;
use crate::mcp::{ActionResult, MAX_SEND_BYTES};
use events::{
    timestamp_ms, ActionEvent, ActionEventLog, ActionOrigin, ActionStage, ApprovalRequiredEvent,
    PendingApprovalInfo,
};
use serde::{Deserialize, Serialize};
use state::{
    ConnectionConfigSummary, ConnectionSnapshot, ConnectionStatus, ControlState, RxReadError,
    RxReadResult, RxRingBuffer, RxWaitError, DEFAULT_RX_MAX_BYTES, DEFAULT_RX_MAX_RECORDS,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
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
const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ApprovalDecision {
    Allow,
    Deny,
    Cancel,
}

struct PendingApproval {
    info: PendingApprovalInfo,
    decision: Mutex<Option<ApprovalDecision>>,
    changed: Condvar,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceState {
    pub connection: ConnectionSnapshot,
    pub rx_buffer_records: usize,
    pub rx_buffer_bytes: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClearReceivedSummary {
    pub records: usize,
    pub bytes: usize,
}

#[derive(Clone)]
pub struct AppControlService {
    manager: Arc<ConnManager>,
    state: Arc<ControlState>,
    rx_buffer: Arc<RxRingBuffer>,
    action_events: Arc<ActionEventLog>,
    action_counter: Arc<AtomicU64>,
    pending_config: Arc<Mutex<Option<ConnConfig>>>,
    permission_mode: Arc<Mutex<PermissionMode>>,
    pending_approvals: Arc<Mutex<HashMap<String, Arc<PendingApproval>>>>,
    approval_timeout: Duration,
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
            action_counter: Arc::new(AtomicU64::new(ACTION_ID_COUNTER_START)),
            pending_config: Arc::new(Mutex::new(None)),
            permission_mode: Arc::new(Mutex::new(PermissionMode::Ask)),
            pending_approvals: Arc::new(Mutex::new(HashMap::new())),
            approval_timeout: DEFAULT_APPROVAL_TIMEOUT,
        }
    }

    #[cfg(test)]
    pub fn with_approval_timeout(mut self, timeout: Duration) -> Self {
        self.approval_timeout = timeout;
        self
    }

    pub fn permission_mode(&self) -> PermissionMode {
        self.permission_mode
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn set_permission_mode(&self, mode: PermissionMode) {
        *self
            .permission_mode
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = mode;
    }

    pub fn pending_approvals(&self) -> Vec<PendingApprovalInfo> {
        self.pending_approvals
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .map(|approval| approval.info.clone())
            .collect()
    }

    pub fn approve_action(&self, action_id: &str) -> Result<(), String> {
        self.resolve_approval(action_id, ApprovalDecision::Allow)
    }

    pub fn deny_action(&self, action_id: &str) -> Result<(), String> {
        self.resolve_approval(action_id, ApprovalDecision::Deny)
    }

    pub fn cancel_pending_approvals(&self) {
        let approvals: Vec<_> = self
            .pending_approvals
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .cloned()
            .collect();
        for approval in approvals {
            let mut decision = approval
                .decision
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            if decision.is_none() {
                *decision = Some(ApprovalDecision::Cancel);
                approval.changed.notify_all();
            }
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

    pub fn latest_rx_cursor(&self) -> u64 {
        self.rx_buffer.latest_cursor()
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
        let operation = if origin == ActionOrigin::Mcp {
            "connect"
        } else {
            "open"
        };
        let action_id =
            self.authorize_mcp_action(&app, &origin, operation, "打开连接", "使用当前连接配置")?;
        let manager = self.manager.clone();
        let pending_config = self.pending_config.clone();
        let state = self.state.clone();
        let open_app = app.clone();
        let result = self.run_action_with_id(
            &app,
            origin,
            operation,
            "打开连接",
            action_id,
            move || {
                *pending_config
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(cfg.clone());
                state.set_config(Some(summary));
                state.set_status(ConnectionStatus::Connecting);
                manager.open(cfg, open_app)
            },
        );
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

    pub fn configure<R: Runtime>(
        &self,
        cfg: ConnConfig,
        app: AppHandle<R>,
        origin: ActionOrigin,
    ) -> Result<ActionResult<ConnectionConfigSummary>, String> {
        let summary = ConnectionConfigSummary::from(&cfg);
        let action_id = self.authorize_mcp_action(
            &app,
            &origin,
            "configure_connection",
            "配置连接",
            &format!("kind={}, endpoint={:?}", summary.kind, summary.endpoint),
        )?;
        let pending_config = self.pending_config.clone();
        let state = self.state.clone();
        self.run_action_with_id(
            &app,
            origin,
            "configure_connection",
            "配置连接",
            action_id,
            move || {
                *pending_config
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(cfg);
                state.set_config(Some(summary.clone()));
                Ok(summary)
            },
        )
    }

    pub fn connect<R: Runtime>(
        &self,
        app: AppHandle<R>,
        origin: ActionOrigin,
    ) -> Result<ActionResult<()>, String> {
        let cfg = self
            .pending_config
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
            .ok_or_else(|| "未配置连接".to_string())?;
        self.open(cfg, app, origin)
    }

    pub fn close<R: Runtime>(
        &self,
        app: AppHandle<R>,
        origin: ActionOrigin,
    ) -> Result<ActionResult<()>, String> {
        let operation = if origin == ActionOrigin::Mcp {
            "disconnect"
        } else {
            "close"
        };
        let action_id =
            self.authorize_mcp_action(&app, &origin, operation, "关闭连接", "关闭当前活动连接")?;
        let manager = self.manager.clone();
        let result = self.run_action_with_id(
            &app,
            origin,
            operation,
            "关闭连接",
            action_id,
            move || {
                manager.close();
                Ok(())
            },
        );
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
        let operation = if origin == ActionOrigin::Mcp {
            "send_data"
        } else {
            "send"
        };
        let action_id = self.authorize_mcp_action(
            &app,
            &origin,
            operation,
            "发送数据",
            &format!("binary payload, {} bytes", data.len()),
        )?;
        let manager = self.manager.clone();
        let tx_data = data.clone();
        let result = self.run_action_with_id(
            &app,
            origin,
            operation,
            "发送数据",
            action_id,
            move || {
                let queue = manager.send_queue_lock();
                let _queue_guard = queue.lock().map_err(|_| "发送队列锁异常".to_string())?;
                manager.send(&data)
            },
        );
        if let Ok(action) = &result {
            self.state.record_tx(action.result);
            let payload = crate::conn::RxPayload {
                data: tx_data,
                ts: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
                    * 1000.0,
                peer: None,
            };
            let _ = app.emit("tx-data", payload);
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
    ) -> Result<ActionResult<ClearReceivedSummary>, String> {
        let action_id = self.authorize_mcp_action(
            &app,
            &origin,
            "clear_received",
            "清空接收缓冲",
            "清除当前接收记录和统计",
        )?;
        let rx_buffer = self.rx_buffer.clone();
        let state = self.state.clone();
        let result = self.run_action_with_id(
            &app,
            origin,
            "clear_received",
            "清空接收缓冲",
            action_id,
            move || {
                let summary = ClearReceivedSummary {
                    records: rx_buffer.len(),
                    bytes: rx_buffer.stored_bytes(),
                };
                rx_buffer.clear();
                state.reset_stats();
                Ok(summary)
            },
        );
        if let Ok(action) = &result {
            let _ = app.emit("rx-cleared", action.action_id.clone());
        }
        result
    }

    fn next_action_id(&self) -> String {
        let sequence = self.action_counter.fetch_add(1, Ordering::Relaxed);
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("action-{}-{}", millis, sequence)
    }

    fn resolve_approval(&self, action_id: &str, decision: ApprovalDecision) -> Result<(), String> {
        let approval = self
            .pending_approvals
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(action_id)
            .cloned()
            .ok_or_else(|| "审批不存在、已结束或 action_id 无效".to_string())?;
        let mut current = approval
            .decision
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if current.is_some() {
            return Err("审批不存在、已结束或 action_id 无效".to_string());
        }
        *current = Some(decision);
        approval.changed.notify_all();
        Ok(())
    }

    fn authorize_mcp_action<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        origin: &ActionOrigin,
        operation: &str,
        label: &str,
        parameter_summary: &str,
    ) -> Result<String, String> {
        let action_id = self.next_action_id();
        if *origin != ActionOrigin::Mcp {
            return Ok(action_id);
        }
        match self.permission_mode() {
            PermissionMode::Full => Ok(action_id),
            PermissionMode::Observe => {
                let message = format!("权限模式 observe 禁止 MCP 写操作: {operation}");
                self.publish(
                    app,
                    ActionEvent {
                        action_id: action_id.clone(),
                        origin: ActionOrigin::Mcp,
                        operation: operation.into(),
                        stage: ActionStage::Failed,
                        summary: message.clone(),
                        timestamp_ms: timestamp_ms(),
                    },
                );
                Err(format!("[{action_id}] {message}"))
            }
            PermissionMode::Ask => {
                let expires_at_ms = timestamp_ms()
                    .saturating_add(self.approval_timeout.as_millis().min(u64::MAX as u128) as u64);
                let info = PendingApprovalInfo {
                    action_id: action_id.clone(),
                    operation: operation.into(),
                    summary: label.into(),
                    parameter_summary: parameter_summary.into(),
                    source: "mcp".into(),
                    expires_at_ms,
                };
                let approval = Arc::new(PendingApproval {
                    info: info.clone(),
                    decision: Mutex::new(None),
                    changed: Condvar::new(),
                });
                self.pending_approvals
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .insert(action_id.clone(), approval.clone());
                self.publish(
                    app,
                    ActionEvent {
                        action_id: action_id.clone(),
                        origin: ActionOrigin::Mcp,
                        operation: operation.into(),
                        stage: ActionStage::ApprovalRequired,
                        summary: format!("等待确认: {label}"),
                        timestamp_ms: timestamp_ms(),
                    },
                );
                let _ = app.emit(
                    "approval-required",
                    ApprovalRequiredEvent {
                        action_id: action_id.clone(),
                        operation: info.operation.clone(),
                        summary: info.summary.clone(),
                        parameter_summary: info.parameter_summary.clone(),
                        source: info.source.clone(),
                        expires_at_ms,
                    },
                );
                let mut decision = approval
                    .decision
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                let deadline = std::time::Instant::now() + self.approval_timeout;
                while decision.is_none() {
                    let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                    if remaining.is_zero() {
                        break;
                    }
                    let (next, result) = approval
                        .changed
                        .wait_timeout(decision, remaining)
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    decision = next;
                    if result.timed_out() {
                        break;
                    }
                }
                let decision = decision.take().unwrap_or(ApprovalDecision::Cancel);
                self.pending_approvals
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .remove(&action_id);
                match decision {
                    ApprovalDecision::Allow => {
                        self.publish(
                            app,
                            ActionEvent {
                                action_id: action_id.clone(),
                                origin: ActionOrigin::Mcp,
                                operation: operation.into(),
                                stage: ActionStage::Approved,
                                summary: "用户已允许操作".into(),
                                timestamp_ms: timestamp_ms(),
                            },
                        );
                        Ok(action_id)
                    }
                    ApprovalDecision::Deny => {
                        let message = format!("用户拒绝 MCP 操作: {operation}");
                        self.publish(
                            app,
                            ActionEvent {
                                action_id: action_id.clone(),
                                origin: ActionOrigin::Mcp,
                                operation: operation.into(),
                                stage: ActionStage::Denied,
                                summary: message.clone(),
                                timestamp_ms: timestamp_ms(),
                            },
                        );
                        Err(format!("[{action_id}] {message}"))
                    }
                    ApprovalDecision::Cancel => {
                        let message = format!("MCP 操作审批超时或应用正在关闭: {operation}");
                        self.publish(
                            app,
                            ActionEvent {
                                action_id: action_id.clone(),
                                origin: ActionOrigin::Mcp,
                                operation: operation.into(),
                                stage: ActionStage::TimedOut,
                                summary: message.clone(),
                                timestamp_ms: timestamp_ms(),
                            },
                        );
                        Err(format!("[{action_id}] {message}"))
                    }
                }
            }
        }
    }

    fn publish<R: Runtime>(&self, app: &AppHandle<R>, event: ActionEvent) {
        self.action_events.push(event.clone());
        let _ = app.emit("control-action", event);
    }

    fn run_action_with_id<R, T, F>(
        &self,
        app: &AppHandle<R>,
        origin: ActionOrigin,
        operation: &str,
        label: &str,
        action_id: String,
        operation_fn: F,
    ) -> Result<ActionResult<T>, String>
    where
        R: Runtime,
        T: Serialize,
        F: FnOnce() -> Result<T, String>,
    {
        self.publish(
            app,
            ActionEvent {
                action_id: action_id.clone(),
                origin: origin.clone(),
                operation: operation.into(),
                stage: ActionStage::Started,
                summary: format!("{}开始", label),
                timestamp_ms: timestamp_ms(),
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
                        timestamp_ms: timestamp_ms(),
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
                        timestamp_ms: timestamp_ms(),
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
        service.set_permission_mode(PermissionMode::Full);
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

    fn test_config() -> ConnConfig {
        ConnConfig::Serial(SerialConfig {
            port: "/dev/does-not-open-in-this-test".into(),
            baudrate: 115200,
            ..SerialConfig::default()
        })
    }

    fn wait_for_pending(service: &AppControlService) -> String {
        for _ in 0..100 {
            if let Some(approval) = service.pending_approvals().first() {
                return approval.action_id.clone();
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("approval was not published");
    }

    #[test]
    fn permission_defaults_to_ask() {
        assert_eq!(
            AppControlService::new().permission_mode(),
            PermissionMode::Ask
        );
    }

    #[test]
    fn observe_rejects_mcp_writes_without_executing_them() {
        let service = AppControlService::new();
        service.set_permission_mode(PermissionMode::Observe);
        let result = service.configure(test_config(), app(), ActionOrigin::Mcp);
        assert!(result.unwrap_err().contains("observe"));
        assert!(service.pending_approvals().is_empty());
        assert!(service.snapshot().config.is_none());
    }

    #[test]
    fn full_allows_mcp_write_without_waiting_for_approval() {
        let service = AppControlService::new();
        service.set_permission_mode(PermissionMode::Full);
        let result = service
            .configure(test_config(), app(), ActionOrigin::Mcp)
            .unwrap();
        assert!(result.action_id.starts_with("action-"));
        assert_eq!(service.snapshot().config.unwrap().kind, "serial");
    }

    #[test]
    fn approval_allow_executes_and_records_action_id() {
        let service =
            Arc::new(AppControlService::new().with_approval_timeout(Duration::from_secs(1)));
        let worker = service.clone();
        let handle =
            std::thread::spawn(move || worker.configure(test_config(), app(), ActionOrigin::Mcp));
        let action_id = wait_for_pending(&service);
        service.approve_action(&action_id).unwrap();
        let result = handle.join().unwrap().unwrap();
        assert_eq!(result.action_id, action_id);
        assert!(service
            .action_events()
            .iter()
            .any(|event| event.action_id == action_id && event.stage == ActionStage::Approved));
    }

    #[test]
    fn approval_deny_does_not_execute_and_rejects_replayed_decision() {
        let service =
            Arc::new(AppControlService::new().with_approval_timeout(Duration::from_secs(1)));
        let worker = service.clone();
        let handle =
            std::thread::spawn(move || worker.configure(test_config(), app(), ActionOrigin::Mcp));
        let action_id = wait_for_pending(&service);
        service.deny_action(&action_id).unwrap();
        assert!(service.deny_action(&action_id).is_err());
        let error = handle.join().unwrap().unwrap_err();
        assert!(error.contains(&action_id) && error.contains("拒绝"));
        assert!(service.snapshot().config.is_none());
    }

    #[test]
    fn approval_timeout_returns_error_with_action_id() {
        let service =
            Arc::new(AppControlService::new().with_approval_timeout(Duration::from_millis(10)));
        let worker = service.clone();
        let handle =
            std::thread::spawn(move || worker.configure(test_config(), app(), ActionOrigin::Mcp));
        let action_id = wait_for_pending(&service);
        let error = handle.join().unwrap().unwrap_err();
        assert!(error.contains(&action_id) && error.contains("超时"));
        assert!(service.pending_approvals().is_empty());
    }

    #[test]
    fn invalid_action_id_cannot_be_approved() {
        let service = AppControlService::new();
        assert!(service.approve_action("action-does-not-exist").is_err());
    }

    #[test]
    fn shutdown_cancels_pending_approval_without_deadlock() {
        let service =
            Arc::new(AppControlService::new().with_approval_timeout(Duration::from_secs(60)));
        let worker = service.clone();
        let handle =
            std::thread::spawn(move || worker.configure(test_config(), app(), ActionOrigin::Mcp));
        let _ = wait_for_pending(&service);
        service.cancel_pending_approvals();
        assert!(handle.join().unwrap().unwrap_err().contains("关闭"));
    }
}
