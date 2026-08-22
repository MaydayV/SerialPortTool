// SerialPortTool - 串口助手 Tauri 后端入口
pub mod conn;

use conn::{ConnConfig, ConnManager};
use serde::Serialize;
use std::fs::OpenOptions;
use std::io::Write;

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
    manager.send(&data)
}

/// 连接状态
#[tauri::command]
fn conn_is_connected(manager: tauri::State<'_, ConnManager>) -> Result<bool, String> {
    Ok(manager.is_connected())
}

#[tauri::command]
fn append_log_file(path: String, line: String) -> Result<(), String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("日志路径为空".into());
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("打开日志文件失败: {}", e))?;
    file.write_all(line.as_bytes())
        .map_err(|e| format!("写入日志文件失败: {}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(ConnManager::new())
        .invoke_handler(tauri::generate_handler![
            list_ports,
            conn_open,
            conn_close,
            conn_send,
            conn_is_connected,
            append_log_file
        ])
        .setup(|app| {
            // 端口热插拔监听
            conn::spawn_port_watcher(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
