//! Runtime lifecycle for the optional embedded MCP transports.
//!
//! The GUI owns this manager. MCP is opt-in: when disabled, neither the
//! loopback HTTP listener nor the local Unix socket is running.

use super::tools::{AppToolControlContext, ToolControlContext};
use super::{McpServer, McpServerHandle};
use crate::control::{local_ipc::LocalIpcServerHandle, AppControlService};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Runtime};

struct RuntimeState {
    http: Option<McpServerHandle>,
    ipc: Option<LocalIpcServerHandle>,
}

/// Owns the optional MCP transports and their shared application control context.
pub struct McpRuntime {
    state: Mutex<RuntimeState>,
    control: Arc<AppControlService>,
}

impl McpRuntime {
    pub fn new(control: Arc<AppControlService>) -> Self {
        Self {
            state: Mutex::new(RuntimeState {
                http: None,
                ipc: None,
            }),
            control,
        }
    }

    pub fn enabled(&self) -> bool {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.http.is_some() && state.ipc.is_some()
    }

    pub fn endpoint(&self) -> String {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state
            .http
            .as_ref()
            .map(McpServerHandle::endpoint)
            .unwrap_or_default()
    }

    pub fn token(&self) -> Result<String, String> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state
            .http
            .as_ref()
            .map(McpServerHandle::local_pairing_token)
            .ok_or_else(|| "MCP 当前未启用，请先在设置中启用 MCP".to_string())
    }

    pub fn reset_token(&self) -> Result<(), String> {
        let state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let http = state
            .http
            .as_ref()
            .ok_or_else(|| "MCP 当前未启用，无法重置 Token".to_string())?;
        http.reset_local_pairing_token()
            .map(|_| ())
            .map_err(|error| format!("重置 MCP Token 失败: {error}"))
    }

    pub fn set_enabled<R: Runtime>(&self, enabled: bool, app: AppHandle<R>) -> Result<(), String> {
        if !enabled {
            self.stop();
            return Ok(());
        }

        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.http.is_some() && state.ipc.is_some() {
            return Ok(());
        }
        if state.http.is_some() || state.ipc.is_some() {
            let http = state.http.take();
            let ipc = state.ipc.take();
            drop(state);
            if let Some(handle) = http {
                handle.shutdown();
            }
            if let Some(handle) = ipc {
                handle.shutdown();
            }
            state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }

        let http = McpServer::new()
            .map_err(|error| format!("启动 MCP HTTP 服务失败: {error}"))?
            .with_control(self.control.clone(), app.clone())
            .start()
            .map_err(|error| format!("启动 MCP HTTP 服务失败: {error}"))?;
        let local_control: Arc<dyn ToolControlContext> =
            Arc::new(AppToolControlContext::new(self.control.clone(), app));
        let ipc = match crate::control::local_ipc::LocalIpcServer::start(
            crate::control::local_ipc::default_endpoint(),
            local_control,
        ) {
            Ok(ipc) => ipc,
            Err(error) => {
                http.shutdown();
                return Err(format!("启动 MCP 本地 IPC 失败: {error}"));
            }
        };
        state.http = Some(http);
        state.ipc = Some(ipc);
        Ok(())
    }

    pub fn stop(&self) {
        let (http, ipc) = {
            let mut state = self
                .state
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            (state.http.take(), state.ipc.take())
        };
        if let Some(handle) = http {
            handle.shutdown();
        }
        if let Some(handle) = ipc {
            handle.shutdown();
        }
    }
}

impl Drop for McpRuntime {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_is_disabled_by_default_and_hides_credentials() {
        let runtime = McpRuntime::new(Arc::new(AppControlService::new()));
        assert!(!runtime.enabled());
        assert_eq!(runtime.endpoint(), "");
        assert!(runtime.token().unwrap_err().contains("未启用"));
        assert!(runtime.reset_token().unwrap_err().contains("未启用"));
    }
}
