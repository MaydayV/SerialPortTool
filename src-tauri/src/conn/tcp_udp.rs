// TCP/UDP 连接实现（M1）
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;
use tauri::{AppHandle, Runtime};

use super::serial::{emit_rx, emit_status};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct TcpUdpConfig {
    pub protocol: String, // tcp / udp
    pub mode: String,     // client / server
    pub target: String,   // client: host:port
    pub port: u16,        // server: 本地监听端口
    pub auto_reconnect: bool,
    pub reconnect_interval: f64, // 秒
}

impl Default for TcpUdpConfig {
    fn default() -> Self {
        Self {
            protocol: "tcp".into(),
            mode: "client".into(),
            target: "127.0.0.1:2345".into(),
            port: 2345,
            auto_reconnect: false,
            reconnect_interval: 1.0,
        }
    }
}

pub enum ConnSock {
    Tcp(TcpStream),
    Udp(UdpSocket),
}

pub struct TcpUdpConn {
    pub protocol: String,
    pub mode: String,
    pub target: String,
    pub port: u16,
    pub auto_reconnect: bool,
    pub reconnect_interval: f64,
    pub stop: Arc<AtomicBool>,
    pub connected: Arc<AtomicBool>,
    // TCP client / UDP: 主连接
    pub sock: Arc<Mutex<Option<ConnSock>>>,
    // UDP 目标地址
    pub target_addr: Option<SocketAddr>,
    // TCP server: 监听器 + 客户端列表
    pub listener: Arc<Mutex<Option<TcpListener>>>,
    pub clients: Arc<Mutex<HashMap<SocketAddr, TcpStream>>>,
    // 线程
    pub threads: Vec<JoinHandle<()>>,
}

impl TcpUdpConn {
    pub fn new(cfg: TcpUdpConfig) -> Self {
        Self {
            protocol: cfg.protocol,
            mode: cfg.mode,
            target: cfg.target,
            port: cfg.port,
            auto_reconnect: cfg.auto_reconnect,
            reconnect_interval: cfg.reconnect_interval,
            stop: Arc::new(AtomicBool::new(false)),
            connected: Arc::new(AtomicBool::new(false)),
            sock: Arc::new(Mutex::new(None)),
            target_addr: None,
            listener: Arc::new(Mutex::new(None)),
            clients: Arc::new(Mutex::new(HashMap::new())),
            threads: Vec::new(),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// 解析 host:port（兼容 "host:port"、http:// 前缀、尾斜杠）
    fn parse_target(target: &str, default_port: u16) -> Result<(String, u16), String> {
        let t = target.trim();
        let t = t.strip_prefix("http://").unwrap_or(t);
        let t = t.strip_prefix("tcp://").unwrap_or(t);
        let (host, port) = match t.rsplit_once(':') {
            Some((h, p)) => {
                let p = p.trim().trim_end_matches('/');
                let p: u16 = p.parse().map_err(|_| format!("端口无效: {}", p))?;
                (h, p)
            }
            None => (t, default_port),
        };
        let host = host.trim_end_matches('/');
        if host.is_empty() {
            Err("目标地址为空".into())
        } else {
            Ok((host.to_string(), port))
        }
    }

    pub fn open<R: Runtime>(&mut self, app: AppHandle<R>) -> Result<(), String> {
        self.stop.store(false, Ordering::SeqCst);
        match (self.protocol.as_str(), self.mode.as_str()) {
            ("tcp", "client") => self.open_tcp_client(app)?,
            ("tcp", "server") => self.open_tcp_server(app)?,
            ("udp", _) => self.open_udp(app)?,
            _ => return Err("不支持的连接类型".into()),
        }
        Ok(())
    }

    fn open_tcp_client<R: Runtime>(&mut self, app: AppHandle<R>) -> Result<(), String> {
        let (host, port) = Self::parse_target(&self.target, 80)?;
        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr).map_err(|e| format!("连接 {} 失败: {}", addr, e))?;
        stream.set_nodelay(true).map_err(|e| e.to_string())?;
        let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
        let local = stream.local_addr().unwrap_or_else(|_| SocketAddr::from(([0; 4], 0)));
        self.sock.lock().unwrap().replace(ConnSock::Tcp(stream));
        self.connected.store(true, Ordering::SeqCst);
        emit_status(&app, "connected", format!("已连接 {} (本地 {})", addr, local));
        self.spawn_tcp_client_rx(app.clone());
        Ok(())
    }

    fn open_tcp_server<R: Runtime>(&mut self, app: AppHandle<R>) -> Result<(), String> {
        let bind_addr = format!("0.0.0.0:{}", self.port);
        let listener = TcpListener::bind(&bind_addr)
            .map_err(|e| format!("监听 {} 失败: {}", bind_addr, e))?;
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        self.listener.lock().unwrap().replace(listener);
        self.connected.store(true, Ordering::SeqCst);
        emit_status(&app, "connected", format!("TCP Server 监听 {}", bind_addr));
        // accept 循环线程
        let listener = self.listener.clone();
        let clients = self.clients.clone();
        let stop = self.stop.clone();
        let app2 = app.clone();
        self.threads.push(std::thread::spawn(move || {
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let (stream, addr) = {
                    let l = listener.lock().unwrap();
                    match l.as_ref() {
                        Some(l) => match l.accept() {
                            Ok((s, a)) => (s, a),
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(50));
                                continue;
                            }
                            Err(_) => break,
                        },
                        None => break,
                    }
                };
                let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                let _ = stream.set_write_timeout(Some(Duration::from_millis(100)));
                let client_count = {
                    let mut c = clients.lock().unwrap();
                    c.insert(addr, stream);
                    c.len()
                };
                emit_status(&app2, "connected", format!("客户端 {} 接入 (共 {})", addr, client_count));
                // 每个客户端一个接收线程
                let clients = clients.clone();
                let stop = stop.clone();
                let app3 = app2.clone();
                std::thread::spawn(move || {
                    let mut buf = vec![0u8; 4096];
                    loop {
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        let (n, closed) = {
                            let mut guard = clients.lock().unwrap();
                            match guard.get_mut(&addr) {
                                Some(s) => match s.read(&mut buf) {
                                    Ok(0) => (0, true),
                                    Ok(n) => (n, false),
                                    Err(ref e)
                                        if e.kind() == std::io::ErrorKind::WouldBlock
                                            || e.kind() == std::io::ErrorKind::TimedOut =>
                                    {
                                        (0, false)
                                    }
                                    Err(_) => (0, true),
                                },
                                None => (0, true),
                            }
                        };
                        if closed {
                            let remain = {
                                let mut c = clients.lock().unwrap();
                                c.remove(&addr);
                                c.len()
                            };
                            emit_status(&app3, "connected", format!("客户端 {} 断开 (剩 {})", addr, remain));
                            break;
                        }
                        if n > 0 {
                            emit_rx(&app3, &buf[..n]);
                        }
                    }
                });
            }
        }));
        Ok(())
    }

    fn open_udp<R: Runtime>(&mut self, app: AppHandle<R>) -> Result<(), String> {
        // client/server 都 bind 本地端口
        let bind_addr = format!("0.0.0.0:{}", self.port);
        let sock = UdpSocket::bind(&bind_addr).map_err(|e| format!("绑定 {} 失败: {}", bind_addr, e))?;
        let _ = sock.set_read_timeout(Some(Duration::from_millis(100)));
        let target_addr = if self.mode == "client" && !self.target.trim().is_empty() {
            let (host, port) = Self::parse_target(&self.target, 0)?;
            Some(format!("{}:{}", host, port).parse::<SocketAddr>().map_err(|e| format!("目标地址无效: {}", e))?)
        } else {
            None
        };
        self.sock.lock().unwrap().replace(ConnSock::Udp(sock));
        self.target_addr = target_addr;
        self.connected.store(true, Ordering::SeqCst);
        emit_status(
            &app,
            "connected",
            format!(
                "UDP 监听 {} ({})",
                bind_addr,
                if let Some(t) = self.target_addr {
                    format!("目标 {}", t)
                } else {
                    "仅监听".into()
                }
            ),
        );
        self.spawn_udp_rx(app.clone());
        Ok(())
    }

    fn spawn_tcp_client_rx<R: Runtime>(&mut self, app: AppHandle<R>) {
        let sock = self.sock.clone();
        let stop = self.stop.clone();
        let connected = self.connected.clone();
        let auto_reconnect = self.auto_reconnect;
        let interval = self.reconnect_interval;
        let target = match Self::parse_target(&self.target, 80) {
            Ok((h, p)) => format!("{}:{}", h, p),
            Err(_) => self.target.clone(),
        };
        self.threads.push(std::thread::spawn(move || {
            let mut buf = vec![0u8; 4096];
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let (n, closed) = {
                    let mut guard = sock.lock().unwrap();
                    match guard.as_mut() {
                        Some(ConnSock::Tcp(s)) => match s.read(&mut buf) {
                            Ok(0) => (0, true),
                            Ok(n) => (n, false),
                            Err(ref e)
                                if e.kind() == std::io::ErrorKind::WouldBlock
                                    || e.kind() == std::io::ErrorKind::TimedOut =>
                            {
                                (0, false)
                            }
                            Err(_) => (0, true),
                        },
                        _ => (0, true),
                    }
                };
                if closed {
                    connected.store(false, Ordering::SeqCst);
                    sock.lock().unwrap().take();
                    if auto_reconnect {
                        emit_status(
                            &app,
                            "connecting",
                            format!("{} 断开，{}s 后重连...", target, interval),
                        );
                        let mut tried = false;
                        while !stop.load(Ordering::SeqCst) {
                            std::thread::sleep(Duration::from_secs_f64(interval));
                            match TcpStream::connect(&target) {
                                Ok(stream) => {
                                    let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                                    sock.lock().unwrap().replace(ConnSock::Tcp(stream));
                                    connected.store(true, Ordering::SeqCst);
                                    emit_status(&app, "connected", format!("重连成功 {}", target));
                                    break;
                                }
                                Err(e) => {
                                    if !tried {
                                        emit_status(&app, "connecting", format!("重连失败: {}", e));
                                        tried = true;
                                    }
                                }
                            }
                        }
                    } else {
                        emit_status(&app, "closed", "连接已关闭".to_string());
                        break;
                    }
                } else if n > 0 {
                    emit_rx(&app, &buf[..n]);
                }
            }
        }));
    }

    fn spawn_udp_rx<R: Runtime>(&mut self, app: AppHandle<R>) {
        let sock = self.sock.clone();
        let stop = self.stop.clone();
        self.threads.push(std::thread::spawn(move || {
            let mut buf = vec![0u8; 4096];
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let n = {
                    let guard = sock.lock().unwrap();
                    match guard.as_ref() {
                        Some(ConnSock::Udp(s)) => match s.recv(&mut buf) {
                            Ok(n) => n,
                            Err(ref e)
                                if e.kind() == std::io::ErrorKind::WouldBlock
                                    || e.kind() == std::io::ErrorKind::TimedOut =>
                            {
                                0
                            }
                            Err(_) => 0,
                        },
                        _ => 0,
                    }
                };
                if n > 0 {
                    emit_rx(&app, &buf[..n]);
                }
            }
        }));
    }

    pub fn send(&self, data: &[u8]) -> Result<usize, String> {
        let mut guard = self.sock.lock().unwrap();
        match guard.as_mut() {
            Some(ConnSock::Tcp(s)) => s.write(data).map_err(|e| e.to_string()),
            Some(ConnSock::Udp(s)) => match self.target_addr {
                Some(t) => s.send_to(data, t).map_err(|e| e.to_string()),
                None => Err("UDP 目标未设置".into()),
            },
            _ => Err("连接未建立".into()),
        }
    }

    /// TCP server 广播到所有客户端
    pub fn broadcast(&self, data: &[u8]) -> Result<usize, String> {
        let mut clients = self.clients.lock().unwrap();
        let mut sent = 0;
        let mut dead = Vec::new();
        for (addr, s) in clients.iter_mut() {
            match s.write(data) {
                Ok(n) => sent += n,
                Err(_) => dead.push(*addr),
            }
        }
        for addr in dead {
            clients.remove(&addr);
        }
        Ok(sent)
    }

    pub fn close(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.sock.lock().unwrap().take();
        self.listener.lock().unwrap().take();
        self.clients.lock().unwrap().clear();
        self.connected.store(false, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_basic() {
        let (h, p) = TcpUdpConn::parse_target("192.168.1.10:8080", 80).unwrap();
        assert_eq!(h, "192.168.1.10");
        assert_eq!(p, 8080);
    }

    #[test]
    fn parse_target_default_port() {
        let (h, p) = TcpUdpConn::parse_target("example.com", 80).unwrap();
        assert_eq!(h, "example.com");
        assert_eq!(p, 80);
    }

    #[test]
    fn parse_target_http_prefix_and_slash() {
        let (h, p) = TcpUdpConn::parse_target("http://host:1234/", 80).unwrap();
        assert_eq!(h, "host");
        assert_eq!(p, 1234);
    }

    #[test]
    fn parse_target_empty_fails() {
        assert!(TcpUdpConn::parse_target("", 80).is_err());
        assert!(TcpUdpConn::parse_target("  ", 80).is_err());
    }

    #[test]
    fn parse_target_invalid_port_fails() {
        assert!(TcpUdpConn::parse_target("host:abc", 80).is_err());
    }
}
