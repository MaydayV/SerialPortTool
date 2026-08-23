// SerialPortTool - 串口助手 Tauri 后端入口
pub mod conn;

use conn::{ConnConfig, ConnManager};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::fs::OpenOptions;
use std::io::{BufWriter, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

const LOG_BUFFER_CAPACITY: usize = 64 * 1024;
const LOG_FLUSH_INTERVAL: Duration = Duration::from_millis(500);
const MAX_SEND_BYTES: usize = 4 * 1024 * 1024;
const MAX_LOG_BATCH_BYTES: usize = 8 * 1024 * 1024;
const MAX_EXPORT_CHUNK_BYTES: usize = 8 * 1024 * 1024;
const MAX_LOG_PATH_BYTES: usize = 4096;
const MAX_AUTHORIZED_OUTPUT_PATHS: usize = 64;

struct LogFile {
    writer: BufWriter<std::fs::File>,
    buffered_bytes: usize,
    last_flush: Instant,
}

#[derive(Default)]
struct LogManager {
    files: Mutex<HashMap<String, LogFile>>,
    authorized_paths: Mutex<HashSet<String>>,
}

impl LogManager {
    fn authorize(&self, path: &str) -> Result<(), String> {
        validate_output_path(path)?;
        let mut paths = self
            .authorized_paths
            .lock()
            .map_err(|_| "文件授权锁异常".to_string())?;
        if !paths.contains(path) && paths.len() >= MAX_AUTHORIZED_OUTPUT_PATHS {
            return Err("本次会话选择的输出文件过多，请重新启动应用后再试".into());
        }
        paths.insert(path.to_string());
        Ok(())
    }

    fn ensure_authorized(&self, path: &str) -> Result<(), String> {
        validate_output_path(path)?;
        let paths = self
            .authorized_paths
            .lock()
            .map_err(|_| "文件授权锁异常".to_string())?;
        if paths.contains(path) {
            Ok(())
        } else {
            Err("文件未经本次会话的系统保存对话框授权".into())
        }
    }

    fn append(&self, path: &str, text: &str) -> Result<(), String> {
        self.ensure_authorized(path)?;
        let mut files = self
            .files
            .lock()
            .map_err(|_| "日志写入锁异常".to_string())?;
        if !files.contains_key(path) {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map_err(|e| format!("打开日志文件失败: {}", e))?;
            files.insert(
                path.to_string(),
                LogFile {
                    writer: BufWriter::with_capacity(LOG_BUFFER_CAPACITY, file),
                    buffered_bytes: 0,
                    last_flush: Instant::now(),
                },
            );
        }

        let result = (|| {
            let state = files.get_mut(path).ok_or("日志写入器不存在")?;
            state
                .writer
                .write_all(text.as_bytes())
                .map_err(|e| format!("写入日志文件失败: {}", e))?;
            state.buffered_bytes += text.len();
            if state.buffered_bytes >= LOG_BUFFER_CAPACITY
                || state.last_flush.elapsed() >= LOG_FLUSH_INTERVAL
            {
                state
                    .writer
                    .flush()
                    .map_err(|e| format!("刷新日志文件失败: {}", e))?;
                state.buffered_bytes = 0;
                state.last_flush = Instant::now();
            }
            Ok(())
        })();
        if result.is_err() {
            files.remove(path);
        }
        result
    }

    fn write_export_chunk(&self, path: &str, text: &str, truncate: bool) -> Result<(), String> {
        self.ensure_authorized(path)?;
        if text.len() > MAX_EXPORT_CHUNK_BYTES {
            return Err("单批导出内容不能超过 8 MiB".into());
        }

        // 导出与持续日志可能选择到同一文件。先关闭缓存写入器，避免覆盖后旧缓冲再次写回。
        if let Some(mut state) = self
            .files
            .lock()
            .map_err(|_| "日志写入锁异常".to_string())?
            .remove(path)
        {
            state
                .writer
                .flush()
                .map_err(|e| format!("刷新日志文件失败: {}", e))?;
        }

        let mut options = OpenOptions::new();
        options.create(true).write(true);
        if truncate {
            options.truncate(true);
        } else {
            options.append(true);
        }
        let mut file = options
            .open(path)
            .map_err(|e| format!("打开导出文件失败: {}", e))?;
        file.write_all(text.as_bytes())
            .map_err(|e| format!("写入导出文件失败: {}", e))?;
        file.flush().map_err(|e| format!("刷新导出文件失败: {}", e))
    }

    fn flush_all(&self) -> Result<(), String> {
        let mut files = self
            .files
            .lock()
            .map_err(|_| "日志写入锁异常".to_string())?;
        for state in files.values_mut() {
            state
                .writer
                .flush()
                .map_err(|e| format!("刷新日志文件失败: {}", e))?;
            state.buffered_bytes = 0;
            state.last_flush = Instant::now();
        }
        Ok(())
    }

    fn flush_due(&self) -> Result<(), String> {
        let mut files = self
            .files
            .lock()
            .map_err(|_| "日志写入锁异常".to_string())?;
        for state in files.values_mut() {
            if state.buffered_bytes > 0 && state.last_flush.elapsed() >= LOG_FLUSH_INTERVAL {
                state
                    .writer
                    .flush()
                    .map_err(|e| format!("刷新日志文件失败: {}", e))?;
                state.buffered_bytes = 0;
                state.last_flush = Instant::now();
            }
        }
        Ok(())
    }

    fn flush_and_close_all(&self) -> Result<(), String> {
        self.flush_all()?;
        self.files
            .lock()
            .map_err(|_| "日志写入锁异常".to_string())?
            .clear();
        Ok(())
    }
}

fn validate_output_path(path: &str) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("文件路径为空".into());
    }
    if path.len() > MAX_LOG_PATH_BYTES {
        return Err("文件路径过长".into());
    }
    if !std::path::Path::new(path).is_absolute() {
        return Err("文件路径必须是绝对路径".into());
    }
    Ok(())
}

#[derive(Serialize)]
struct PortInfo {
    name: String,
    desc: String,
    port_type: String,
    vid: Option<u16>,
    pid: Option<u16>,
    serial: Option<String>,
}

/// 枚举系统串口列表
#[tauri::command]
fn list_ports() -> Result<Vec<PortInfo>, String> {
    let ports = serialport::available_ports().map_err(|e| e.to_string())?;
    Ok(ports
        .into_iter()
        .map(|p| {
            let (port_type, vid, pid, serial, desc) = match &p.port_type {
                serialport::SerialPortType::UsbPort(info) => (
                    "usb".to_string(),
                    Some(info.vid),
                    Some(info.pid),
                    info.serial_number.clone(),
                    format!(
                        "{} {}",
                        info.manufacturer.clone().unwrap_or_default(),
                        info.product.clone().unwrap_or_default()
                    )
                    .trim()
                    .to_string(),
                ),
                serialport::SerialPortType::BluetoothPort => {
                    ("bluetooth".to_string(), None, None, None, String::new())
                }
                serialport::SerialPortType::PciPort => {
                    ("pci".to_string(), None, None, None, String::new())
                }
                _ => ("unknown".to_string(), None, None, None, String::new()),
            };
            PortInfo {
                name: p.port_name,
                desc,
                port_type,
                vid,
                pid,
                serial,
            }
        })
        .collect())
}

/// 打开连接（串口 / TCP / UDP）
#[tauri::command]
fn conn_open(
    manager: tauri::State<'_, ConnManager>,
    app: tauri::AppHandle,
    cfg: ConnConfig,
) -> Result<(), String> {
    manager.open(cfg, app)
}

/// 关闭连接
#[tauri::command]
fn conn_close(manager: tauri::State<'_, ConnManager>) -> Result<(), String> {
    manager.close();
    Ok(())
}

/// 发送数据（接收 bytes 数组）
#[tauri::command]
fn conn_send(manager: tauri::State<'_, ConnManager>, data: Vec<u8>) -> Result<usize, String> {
    if data.len() > MAX_SEND_BYTES {
        return Err("单次发送不能超过 4 MiB".into());
    }
    manager.send(&data)
}

#[tauri::command]
fn append_log_file(
    manager: tauri::State<'_, Arc<LogManager>>,
    path: String,
    line: String,
) -> Result<(), String> {
    let path = path.trim();
    validate_output_path(path)?;
    if line.len() > MAX_LOG_BATCH_BYTES {
        return Err("单批日志不能超过 8 MiB".into());
    }
    manager.append(path, &line)
}

/// 使用系统 NSSavePanel/原生保存对话框选择输出文件，并记录本次会话授权。
#[tauri::command]
async fn select_output_file(
    app: tauri::AppHandle,
    manager: tauri::State<'_, Arc<LogManager>>,
    kind: String,
) -> Result<Option<String>, String> {
    let unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (title, file_name, filter_name, extensions): (&str, String, &str, &[&str]) =
        match kind.as_str() {
            "log" => (
                "选择日志保存位置",
                format!("serialaid-log-{}.log", unix_seconds),
                "日志文件",
                &["log", "txt"],
            ),
            "templates" => (
                "导出协议模板",
                "serialaid-templates.json".to_string(),
                "JSON 文件",
                &["json"],
            ),
            "curve" => (
                "导出波形数据",
                format!("curve-{}.csv", unix_seconds),
                "CSV 文件",
                &["csv"],
            ),
            _ => return Err("不支持的导出文件类型".into()),
        };

    let selected = app
        .dialog()
        .file()
        .set_title(title)
        .set_file_name(file_name)
        .add_filter(filter_name, extensions)
        .blocking_save_file();
    let Some(selected) = selected else {
        return Ok(None);
    };
    let path = selected
        .into_path()
        .map_err(|_| "所选文件不是本机文件路径".to_string())?;
    let path = path.to_string_lossy().to_string();
    manager.authorize(&path)?;
    Ok(Some(path))
}

#[tauri::command]
fn write_user_file(
    manager: tauri::State<'_, Arc<LogManager>>,
    path: String,
    text: String,
    truncate: bool,
) -> Result<(), String> {
    manager.write_export_chunk(path.trim(), &text, truncate)
}

#[tauri::command]
fn flush_log_files(manager: tauri::State<'_, Arc<LogManager>>) -> Result<(), String> {
    manager.flush_and_close_all()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let log_manager = Arc::new(LogManager::default());
    let log_flusher = log_manager.clone();
    std::thread::spawn(move || loop {
        std::thread::sleep(LOG_FLUSH_INTERVAL);
        let _ = log_flusher.flush_due();
    });
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(ConnManager::new())
        .manage(log_manager)
        .invoke_handler(tauri::generate_handler![
            list_ports,
            conn_open,
            conn_close,
            conn_send,
            select_output_file,
            write_user_file,
            append_log_file,
            flush_log_files
        ])
        .setup(|app| {
            // 端口热插拔监听
            conn::spawn_port_watcher(app.handle().clone());
            Ok(())
        })
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                let manager = window.app_handle().state::<Arc<LogManager>>();
                let _ = manager.flush_and_close_all();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buffered_log_writer_flushes_complete_content() {
        let path = std::env::temp_dir().join(format!(
            "serial-aid-log-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let manager = LogManager::default();
        manager.authorize(path.to_str().unwrap()).unwrap();
        manager.append(path.to_str().unwrap(), "first\n").unwrap();
        manager.append(path.to_str().unwrap(), "second\n").unwrap();
        manager.flush_all().unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert_eq!(content, "first\nsecond\n");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn log_writer_rejects_paths_not_selected_in_this_session() {
        let path = std::env::temp_dir().join("serial-aid-unauthorized.log");
        let manager = LogManager::default();
        let error = manager
            .append(path.to_str().unwrap(), "blocked\n")
            .unwrap_err();
        assert!(error.contains("未经本次会话"));
    }

    #[test]
    fn authorized_export_truncates_then_appends_chunks() {
        let path = std::env::temp_dir().join(format!(
            "serial-aid-export-{}-{}.txt",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        ));
        let manager = LogManager::default();
        manager.authorize(path.to_str().unwrap()).unwrap();
        manager
            .write_export_chunk(path.to_str().unwrap(), "first", true)
            .unwrap();
        manager
            .write_export_chunk(path.to_str().unwrap(), "-second", false)
            .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "first-second");
        let _ = std::fs::remove_file(path);
    }
}
