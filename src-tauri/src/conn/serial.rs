// 串口连接实现（M1）
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SerialConfig {
    pub port: String,
    pub baudrate: u32,
    pub data_bits: u8,      // 5-8
    pub parity: String,     // none/odd/even
    pub stop_bits: f32,     // 1 / 2
    pub flow_control: String, // none/software/hardware
    pub rts: bool,
    pub dtr: bool,
    pub auto_reconnect: bool,
}

impl Default for SerialConfig {
    fn default() -> Self {
        Self {
            port: String::new(),
            baudrate: 115200,
            data_bits: 8,
            parity: "none".into(),
            stop_bits: 1.0,
            flow_control: "none".into(),
            rts: false,
            dtr: false,
            auto_reconnect: false,
        }
    }
}

pub struct SerialConn {
    pub port: Arc<Mutex<Option<Box<dyn serialport::SerialPort>>>>,
    pub stop: Arc<AtomicBool>,
    pub rx_thread: Option<JoinHandle<()>>,
    pub reconnect_thread: Option<JoinHandle<()>>,
    pub cfg: SerialConfig,
    connected: Arc<AtomicBool>,
}

impl SerialConn {
    pub fn new(cfg: SerialConfig) -> Self {
        Self {
            port: Arc::new(Mutex::new(None)),
            stop: Arc::new(AtomicBool::new(false)),
            rx_thread: None,
            reconnect_thread: None,
            connected: Arc::new(AtomicBool::new(false)),
            cfg,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// 打开串口并启动接收线程（调用方保证 UI 线程）
    pub fn open<R: Runtime>(&mut self, app: AppHandle<R>) -> Result<(), String> {
        self.stop.store(false, Ordering::SeqCst);
        self.open_port(&app)?;
        Ok(())
    }

    fn open_port<R: Runtime>(&mut self, app: &AppHandle<R>) -> Result<(), String> {
        if self.port.lock().unwrap().is_some() {
            return Ok(()); // 已打开
        }
        let cfg = &self.cfg;
        let mut builder = serialport::new(&cfg.port, cfg.baudrate)
            .timeout(Duration::from_millis(50));
        builder = match cfg.data_bits {
            5 => builder.data_bits(serialport::DataBits::Five),
            6 => builder.data_bits(serialport::DataBits::Six),
            7 => builder.data_bits(serialport::DataBits::Seven),
            _ => builder.data_bits(serialport::DataBits::Eight),
        };
        builder = match cfg.parity.as_str() {
            "odd" => builder.parity(serialport::Parity::Odd),
            "even" => builder.parity(serialport::Parity::Even),
            _ => builder.parity(serialport::Parity::None),
        };
        builder = if cfg.stop_bits >= 2.0 {
            builder.stop_bits(serialport::StopBits::Two)
        } else {
            builder.stop_bits(serialport::StopBits::One)
        };
        builder = match cfg.flow_control.as_str() {
            "software" => builder.flow_control(serialport::FlowControl::Software),
            "hardware" => builder.flow_control(serialport::FlowControl::Hardware),
            _ => builder.flow_control(serialport::FlowControl::None),
        };
        let mut port = builder.open().map_err(|e| e.to_string())?;
        let _ = port.write_request_to_send(cfg.rts);
        let _ = port.write_data_terminal_ready(cfg.dtr);
        self.port.lock().unwrap().replace(port);
        self.connected.store(true, Ordering::SeqCst);
        emit_status(app, "connected", format!("已连接 {}", cfg.port));
        self.spawn_rx_thread(app.clone());
        Ok(())
    }

    /// 接收线程：read 循环，空闲判定组帧，emit 到前端
    fn spawn_rx_thread<R: Runtime>(&mut self, app: AppHandle<R>) {
        let port = self.port.clone();
        let stop = self.stop.clone();
        let connected = self.connected.clone();
        let auto_reconnect = self.cfg.auto_reconnect;
        self.rx_thread = Some(std::thread::spawn(move || {
            let mut buf = vec![0u8; 4096];
            let mut frame: Vec<u8> = Vec::new();
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let n = {
                    let mut guard = port.lock().unwrap();
                    match guard.as_mut() {
                        Some(p) => match p.read(&mut buf) {
                            Ok(n) => n,
                            Err(e) => {
                                let is_timeout = e
                                    .to_string()
                                    .contains("timeout")
                                    || e.kind() == std::io::ErrorKind::TimedOut
                                    || e.to_string().contains("Operation timed out");
                                if is_timeout {
                                    0
                                } else {
                                    // 设备移除等错误
                                    guard.take();
                                    connected.store(false, Ordering::SeqCst);
                                    emit_status(&app, "lose", format!("连接断开: {}", e));
                                    break;
                                }
                            }
                        },
                        None => {
                            // 串口被外部关闭
                            connected.store(false, Ordering::SeqCst);
                            break;
                        }
                    }
                };
                if n > 0 {
                    frame.extend_from_slice(&buf[..n]);
                    // 尽量一次多攒：若数据量大，继续读直到空闲
                    if frame.len() >= 64 * 1024 {
                        emit_rx(&app, &frame);
                        frame.clear();
                    }
                } else if !frame.is_empty() {
                    // 空闲判定：一帧完成
                    emit_rx(&app, &frame);
                    frame.clear();
                }
            }
            // 线程退出
            connected.store(false, Ordering::SeqCst);
            if auto_reconnect {
                emit_status(&app, "connecting", "掉线，尝试重连...".to_string());
            } else {
                emit_status(&app, "closed", "连接已关闭".to_string());
            }
        }));
    }

    /// 发送
    pub fn send(&self, data: &[u8]) -> Result<usize, String> {
        let mut guard = self.port.lock().unwrap();
        let p = guard.as_mut().ok_or("串口未打开")?;
        p.write(data).map_err(|e| e.to_string())
    }

    /// 关闭：停线程 + 关串口
    pub fn close(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.port.lock().unwrap().take();
        self.connected.store(false, Ordering::SeqCst);
        if let Some(t) = self.rx_thread.take() {
            let _ = t.join();
        }
        if let Some(t) = self.reconnect_thread.take() {
            let _ = t.join();
        }
    }
}

#[derive(Serialize, Clone)]
pub struct RxPayload {
    pub data: Vec<u8>,
    pub ts: f64,
}

pub fn emit_rx<R: Runtime>(app: &AppHandle<R>, data: &[u8]) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0;
    let _ = app.emit(
        "rx-data",
        RxPayload {
            data: data.to_vec(),
            ts,
        },
    );
}

#[derive(Serialize, Clone)]
pub struct StatusPayload {
    pub status: String, // connected/closed/lose/connecting
    pub msg: String,
}

pub fn emit_status<R: Runtime>(app: &AppHandle<R>, status: &str, msg: String) {
    let _ = app.emit(
        "conn-status",
        StatusPayload {
            status: status.into(),
            msg,
        },
    );
}
