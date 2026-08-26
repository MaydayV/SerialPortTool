// 串口连接实现（M1）
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Runtime};

pub type RxObserver = Arc<dyn Fn(RxPayload) + Send + Sync + 'static>;
pub type StatusObserver = Arc<dyn Fn(&str, &str) + Send + Sync + 'static>;

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SerialConfig {
    pub port: String,
    pub baudrate: u32,
    pub data_bits: u8,        // 5-8
    pub parity: String,       // none/odd/even
    pub stop_bits: f32,       // 1 / 2
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
    pub cfg: SerialConfig,
    connected: Arc<AtomicBool>,
}

impl SerialConn {
    pub fn new(cfg: SerialConfig) -> Self {
        Self {
            port: Arc::new(Mutex::new(None)),
            stop: Arc::new(AtomicBool::new(false)),
            rx_thread: None,
            connected: Arc::new(AtomicBool::new(false)),
            cfg,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// 打开串口并启动接收线程（调用方保证 UI 线程）
    pub fn open<R: Runtime>(&mut self, app: AppHandle<R>) -> Result<(), String> {
        self.open_with_observers(app, None, None)
    }

    pub fn open_with_observers<R: Runtime>(
        &mut self,
        app: AppHandle<R>,
        rx_observer: Option<RxObserver>,
        status_observer: Option<StatusObserver>,
    ) -> Result<(), String> {
        self.stop.store(false, Ordering::SeqCst);
        let port = build_port(&self.cfg)?;
        self.port.lock().unwrap().replace(port);
        self.connected.store(true, Ordering::SeqCst);
        emit_status_observed(
            &app,
            "connected",
            format!("已连接 {}", self.cfg.port),
            status_observer.as_ref(),
        );
        self.spawn_rx_thread(app, rx_observer, status_observer);
        Ok(())
    }

    /// 接收线程同时负责断线检测和串口重连，避免多个线程竞争端口锁。
    fn spawn_rx_thread<R: Runtime>(
        &mut self,
        app: AppHandle<R>,
        rx_observer: Option<RxObserver>,
        status_observer: Option<StatusObserver>,
    ) {
        let port = self.port.clone();
        let stop = self.stop.clone();
        let connected = self.connected.clone();
        let cfg = self.cfg.clone();
        let auto_reconnect = cfg.auto_reconnect;
        self.rx_thread = Some(std::thread::spawn(move || {
            let mut buf = vec![0u8; 4096];
            let mut frame: Vec<u8> = Vec::new();

            while !stop.load(Ordering::SeqCst) {
                let read_result = {
                    let mut guard = port.lock().unwrap();
                    match guard.as_mut() {
                        Some(p) => p.read(&mut buf),
                        None => Err(std::io::Error::new(
                            std::io::ErrorKind::NotConnected,
                            "串口未打开",
                        )),
                    }
                };

                match read_result {
                    Ok(n) if n > 0 => {
                        frame.extend_from_slice(&buf[..n]);
                        if frame.len() >= 64 * 1024 {
                            emit_rx_observed(&app, &frame, None, rx_observer.as_ref());
                            frame.clear();
                        }
                    }
                    Ok(_) => {
                        if !frame.is_empty() {
                            emit_rx_observed(&app, &frame, None, rx_observer.as_ref());
                            frame.clear();
                        }
                    }
                    Err(e) if is_timeout(&e) => {
                        if !frame.is_empty() {
                            emit_rx_observed(&app, &frame, None, rx_observer.as_ref());
                            frame.clear();
                        }
                    }
                    Err(e) => {
                        let message = e.to_string();
                        port.lock().unwrap().take();
                        connected.store(false, Ordering::SeqCst);
                        frame.clear();

                        if stop.load(Ordering::SeqCst) {
                            break;
                        }

                        emit_status_observed(
                            &app,
                            "lose",
                            format!("连接断开: {}", message),
                            status_observer.as_ref(),
                        );
                        if !auto_reconnect {
                            emit_status_observed(
                                &app,
                                "closed",
                                "连接已关闭".to_string(),
                                status_observer.as_ref(),
                            );
                            break;
                        }

                        emit_status_observed(
                            &app,
                            "connecting",
                            "掉线，尝试重连...".to_string(),
                            status_observer.as_ref(),
                        );
                        while !stop.load(Ordering::SeqCst) {
                            if sleep_until(&stop, Duration::from_millis(1000)) {
                                break;
                            }
                            match build_port(&cfg) {
                                Ok(new_port) => {
                                    if stop.load(Ordering::SeqCst) {
                                        break;
                                    }
                                    port.lock().unwrap().replace(new_port);
                                    connected.store(true, Ordering::SeqCst);
                                    emit_status_observed(
                                        &app,
                                        "connected",
                                        format!("重连成功 {}", cfg.port),
                                        status_observer.as_ref(),
                                    );
                                    break;
                                }
                                Err(reconnect_error) => {
                                    emit_status_observed(
                                        &app,
                                        "connecting",
                                        format!("重连失败: {}", reconnect_error),
                                        status_observer.as_ref(),
                                    );
                                }
                            }
                        }
                    }
                }
            }

            connected.store(false, Ordering::SeqCst);
            if stop.load(Ordering::SeqCst) {
                emit_status_observed(
                    &app,
                    "closed",
                    "连接已关闭".to_string(),
                    status_observer.as_ref(),
                );
            }
        }));
    }

    /// 发送，保证整个缓冲区写入后才返回。
    pub fn send(&self, data: &[u8]) -> Result<usize, String> {
        let mut guard = self.port.lock().unwrap();
        let p = guard.as_mut().ok_or("串口未打开")?;
        p.write_all(data).map_err(|e| e.to_string())?;
        Ok(data.len())
    }

    /// 关闭：先发停止信号，再释放端口，最后等待所有线程退出。
    pub fn close(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.port.lock().unwrap().take();
        self.connected.store(false, Ordering::SeqCst);
        if let Some(t) = self.rx_thread.take() {
            let _ = t.join();
        }
    }
}

fn build_port(cfg: &SerialConfig) -> Result<Box<dyn serialport::SerialPort>, String> {
    if cfg.port.trim().is_empty() {
        return Err("串口名称为空".into());
    }
    if cfg.baudrate == 0 {
        return Err("波特率必须大于 0".into());
    }
    let mut builder = serialport::new(&cfg.port, cfg.baudrate).timeout(Duration::from_millis(50));
    builder = match cfg.data_bits {
        5 => builder.data_bits(serialport::DataBits::Five),
        6 => builder.data_bits(serialport::DataBits::Six),
        7 => builder.data_bits(serialport::DataBits::Seven),
        8 => builder.data_bits(serialport::DataBits::Eight),
        _ => return Err(format!("不支持的数据位: {}", cfg.data_bits)),
    };
    builder = match cfg.parity.as_str() {
        "odd" => builder.parity(serialport::Parity::Odd),
        "even" => builder.parity(serialport::Parity::Even),
        "none" => builder.parity(serialport::Parity::None),
        _ => return Err(format!("不支持的校验方式: {}", cfg.parity)),
    };
    builder = match cfg.stop_bits {
        value if (value - 1.0).abs() < f32::EPSILON => builder.stop_bits(serialport::StopBits::One),
        value if (value - 2.0).abs() < f32::EPSILON => builder.stop_bits(serialport::StopBits::Two),
        _ => return Err(format!("不支持的停止位: {}", cfg.stop_bits)),
    };
    builder = match cfg.flow_control.as_str() {
        "software" => builder.flow_control(serialport::FlowControl::Software),
        "hardware" => builder.flow_control(serialport::FlowControl::Hardware),
        "none" => builder.flow_control(serialport::FlowControl::None),
        _ => return Err(format!("不支持的流控方式: {}", cfg.flow_control)),
    };
    let mut port = builder.open().map_err(|e| e.to_string())?;
    port.write_request_to_send(cfg.rts)
        .map_err(|e| format!("设置 RTS 失败: {}", e))?;
    port.write_data_terminal_ready(cfg.dtr)
        .map_err(|e| format!("设置 DTR 失败: {}", e))?;
    Ok(port)
}

fn is_timeout(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::TimedOut
        || error.to_string().to_ascii_lowercase().contains("timeout")
        || error.to_string().contains("Operation timed out")
}

fn sleep_until(stop: &AtomicBool, duration: Duration) -> bool {
    let mut remaining = duration;
    while remaining > Duration::ZERO {
        if stop.load(Ordering::SeqCst) {
            return true;
        }
        let step = remaining.min(Duration::from_millis(100));
        std::thread::sleep(step);
        remaining = remaining.saturating_sub(step);
    }
    stop.load(Ordering::SeqCst)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct RxPayload {
    pub data: Vec<u8>,
    pub ts: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub peer: Option<String>,
}

pub fn emit_rx<R: Runtime>(app: &AppHandle<R>, data: &[u8]) {
    emit_rx_from(app, data, None);
}

pub fn emit_rx_from<R: Runtime>(app: &AppHandle<R>, data: &[u8], peer: Option<String>) {
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
            peer,
        },
    );
}

pub fn emit_rx_observed<R: Runtime>(
    app: &AppHandle<R>,
    data: &[u8],
    peer: Option<String>,
    observer: Option<&RxObserver>,
) {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        * 1000.0;
    let payload = RxPayload {
        data: data.to_vec(),
        ts,
        peer,
    };
    if let Some(observer) = observer {
        observer(payload.clone());
    }
    let _ = app.emit("rx-data", payload);
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

pub fn emit_status_observed<R: Runtime>(
    app: &AppHandle<R>,
    status: &str,
    msg: String,
    observer: Option<&StatusObserver>,
) {
    if let Some(observer) = observer {
        observer(status, &msg);
    }
    emit_status(app, status, msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named_config() -> SerialConfig {
        SerialConfig {
            port: "test-port-that-must-not-be-opened".into(),
            ..SerialConfig::default()
        }
    }

    #[test]
    fn rejects_empty_port_before_opening() {
        assert!(build_port(&SerialConfig::default())
            .err()
            .unwrap()
            .contains("串口名称为空"));
    }

    #[test]
    fn rejects_zero_baudrate_before_opening() {
        let mut config = named_config();
        config.baudrate = 0;
        assert!(build_port(&config).err().unwrap().contains("波特率"));
    }

    #[test]
    fn rejects_invalid_serial_parameters_before_opening() {
        let mut config = named_config();
        config.data_bits = 9;
        assert!(build_port(&config).err().unwrap().contains("数据位"));

        let mut config = named_config();
        config.parity = "mark".into();
        assert!(build_port(&config).err().unwrap().contains("校验方式"));

        let mut config = named_config();
        config.stop_bits = 1.5;
        assert!(build_port(&config).err().unwrap().contains("停止位"));

        let mut config = named_config();
        config.flow_control = "invalid".into();
        assert!(build_port(&config).err().unwrap().contains("流控"));
    }
}
