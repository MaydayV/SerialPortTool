//! Events emitted by the application control service.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::Mutex;

const EVENT_HISTORY_CAPACITY: usize = 1024;

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
    Finished,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActionEvent {
    pub action_id: String,
    pub origin: ActionOrigin,
    pub operation: String,
    pub stage: ActionStage,
    pub summary: String,
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
