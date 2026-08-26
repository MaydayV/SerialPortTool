//! Events emitted by the application control service.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const EVENT_HISTORY_CAPACITY: usize = 1024;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ActionOrigin {
    #[default]
    Ui,
    Mcp,
    System,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActionStage {
    Started,
    ApprovalRequired,
    Approved,
    Finished,
    Failed,
    Denied,
    TimedOut,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionEvent {
    pub action_id: String,
    pub origin: ActionOrigin,
    pub operation: String,
    pub stage: ActionStage,
    pub summary: String,
    pub timestamp_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalRequiredEvent {
    pub action_id: String,
    pub operation: String,
    pub summary: String,
    pub parameter_summary: String,
    pub source: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingApprovalInfo {
    pub action_id: String,
    pub operation: String,
    pub summary: String,
    pub parameter_summary: String,
    pub source: String,
    pub expires_at_ms: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum McpActivityStage {
    Connected,
    Started,
    Finished,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct McpActivityEvent {
    pub stage: McpActivityStage,
    pub operation: String,
    pub summary: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action_id: Option<String>,
    pub timestamp_ms: u64,
}

#[derive(Default)]
pub(crate) struct ActionEventLog {
    events: Mutex<VecDeque<ActionEvent>>,
}

impl ActionEventLog {
    pub(crate) fn push(&self, event: ActionEvent) {
        let mut events = self
            .events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if events.len() >= EVENT_HISTORY_CAPACITY {
            events.pop_front();
        }
        events.push_back(event);
    }

    pub(crate) fn snapshot(&self) -> Vec<ActionEvent> {
        self.events
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .iter()
            .cloned()
            .collect()
    }
}

pub(crate) fn timestamp_ms() -> u64 {
    now_ms()
}
