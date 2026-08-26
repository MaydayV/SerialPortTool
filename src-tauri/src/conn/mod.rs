// 连接层：统一管理串口 / TCP / UDP
pub mod serial;
pub mod tcp_udp;

use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Runtime};

pub use serial::{
    emit_rx_observed, RxObserver, RxPayload, SerialConfig, SerialConn, StatusObserver,
};
pub use tcp_udp::{TcpUdpConfig, TcpUdpConn};

/// 当前活动的连接（单连接模型）
pub enum ActiveConn {
    Serial(SerialConn),
    TcpUdp(TcpUdpConn),
}

/// 连接配置（前端传入，serde tag 区分类型）
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(tag = "type", content = "config")]
pub enum ConnConfig {
    Serial(SerialConfig),
    TcpUdp(TcpUdpConfig),
}

pub struct ConnManager {
    pub conn: Mutex<Option<ActiveConn>>,
    lifecycle: Mutex<()>,
    send_queue: Arc<Mutex<()>>,
    rx_observer: Mutex<Option<RxObserver>>,
    status_observer: Mutex<Option<StatusObserver>>,
}

impl Default for ConnManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ConnManager {
    pub fn new() -> Self {
        Self {
            conn: Mutex::new(None),
            lifecycle: Mutex::new(()),
            send_queue: Arc::new(Mutex::new(())),
            rx_observer: Mutex::new(None),
            status_observer: Mutex::new(None),
        }
    }

    pub fn is_connected(&self) -> bool {
        let guard = self.conn.lock().unwrap();
        match guard.as_ref() {
            Some(ActiveConn::Serial(c)) => c.is_connected(),
            Some(ActiveConn::TcpUdp(c)) => c.is_connected(),
            None => false,
        }
    }

    /// 打开连接（替换旧连接）
    pub fn open<R: Runtime>(&self, cfg: ConnConfig, app: AppHandle<R>) -> Result<(), String> {
        let rx_observer = self.rx_observer.lock().unwrap().clone();
        let status_observer = self.status_observer.lock().unwrap().clone();
        self.open_with_observers(cfg, app, rx_observer, status_observer)
    }

    pub fn set_observers(
        &self,
        rx_observer: Option<RxObserver>,
        status_observer: Option<StatusObserver>,
    ) {
        *self.rx_observer.lock().unwrap() = rx_observer;
        *self.status_observer.lock().unwrap() = status_observer;
    }

    pub fn send_queue_lock(&self) -> Arc<Mutex<()>> {
        self.send_queue.clone()
    }

    pub fn open_with_observers<R: Runtime>(
        &self,
        cfg: ConnConfig,
        app: AppHandle<R>,
        rx_observer: Option<RxObserver>,
        status_observer: Option<StatusObserver>,
    ) -> Result<(), String> {
        let _operation = self.lifecycle.lock().unwrap();
        // 先关闭旧连接
        self.close_inner();
        let new_conn = match cfg {
            ConnConfig::Serial(cfg) => {
                let mut c = SerialConn::new(cfg);
                c.open_with_observers(app, rx_observer.clone(), status_observer.clone())?;
                ActiveConn::Serial(c)
            }
            ConnConfig::TcpUdp(cfg) => {
                let mut c = TcpUdpConn::new(cfg);
                c.open_with_observer(app, rx_observer, status_observer)?;
                ActiveConn::TcpUdp(c)
            }
        };
        *self.conn.lock().unwrap() = Some(new_conn);
        Ok(())
    }

    /// 发送数据
    pub fn send(&self, data: &[u8]) -> Result<usize, String> {
        let guard = self.conn.lock().unwrap();
        match guard.as_ref() {
            Some(ActiveConn::Serial(c)) => c.send(data),
            Some(ActiveConn::TcpUdp(c)) => {
                if c.mode == "server" && c.protocol == "tcp" {
                    c.broadcast(data)
                } else {
                    c.send(data)
                }
            }
            None => Err("未连接".into()),
        }
    }

    /// 关闭当前连接
    pub fn close(&self) {
        let _operation = self.lifecycle.lock().unwrap();
        self.close_inner();
    }

    fn close_inner(&self) {
        let mut guard = self.conn.lock().unwrap();
        if let Some(conn) = guard.take() {
            match conn {
                ActiveConn::Serial(mut c) => c.close(),
                ActiveConn::TcpUdp(mut c) => c.close(),
            }
        }
    }
}

impl Drop for ConnManager {
    fn drop(&mut self) {
        self.close();
    }
}

/// 端口热插拔监听：轮询端口列表，变化时推送前端
pub fn spawn_port_watcher(app: AppHandle) {
    std::thread::spawn(move || {
        let mut last: Vec<String> = Vec::new();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(1500));
            let mut ports = serialport::available_ports()
                .map(|ps| ps.iter().map(|p| p.port_name.clone()).collect::<Vec<_>>())
                .unwrap_or_default();
            ports.sort();
            if ports != last {
                let added = ports
                    .iter()
                    .filter(|p| !last.contains(p))
                    .cloned()
                    .collect::<Vec<_>>();
                let removed = last
                    .iter()
                    .filter(|p| !ports.contains(p))
                    .cloned()
                    .collect::<Vec<_>>();
                let _ = app.emit(
                    "ports-changed",
                    serde_json::json!({ "ports": ports, "added": added, "removed": removed }),
                );
                last = ports;
            }
        }
    });
}
