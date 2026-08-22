// TCP/UDP 连接实现（M1）
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::str::FromStr;
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
    pub target: String,   // client: host:port; UDP server 可选回复目标
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
    pub last_sender: Arc<Mutex<Option<SocketAddr>>>,
    // TCP server: 监听器 + 客户端列表（列表只保存写端）
    pub listener: Arc<Mutex<Option<TcpListener>>>,
    pub clients: Arc<Mutex<HashMap<SocketAddr, TcpStream>>>,
    // accept 线程和客户端接收线程
    pub threads: Vec<JoinHandle<()>>,
    pub worker_threads: Arc<Mutex<Vec<JoinHandle<()>>>>,
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
            last_sender: Arc::new(Mutex::new(None)),
            listener: Arc::new(Mutex::new(None)),
            clients: Arc::new(Mutex::new(HashMap::new())),
            threads: Vec::new(),
            worker_threads: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::SeqCst)
    }

    /// 解析 host:port，规范支持 [::1]:port，也兼容 host:port、协议前缀和尾斜杠。
    fn parse_target(target: &str, default_port: u16) -> Result<(String, u16), String> {
        let mut value = target.trim();
        value = value
            .strip_prefix("http://")
            .or_else(|| value.strip_prefix("tcp://"))
            .or_else(|| value.strip_prefix("udp://"))
            .unwrap_or(value)
            .trim_end_matches('/');
        if value.is_empty() {
            return Err("目标地址为空".into());
        }

        if let Some(rest) = value.strip_prefix('[') {
            let end = rest.find(']').ok_or("IPv6 地址缺少 ]")?;
            let host = &rest[..end];
            if host.is_empty() {
                return Err("目标地址为空".into());
            }
            let suffix = &rest[end + 1..];
            let port = if suffix.is_empty() {
                default_port
            } else if let Some(port) = suffix.strip_prefix(':') {
                port.parse::<u16>()
                    .map_err(|_| format!("端口无效: {}", port))?
            } else {
                return Err(format!("IPv6 地址格式无效: {}", value));
            };
            return Ok((host.to_string(), port));
        }

        if let Ok(addr) = SocketAddr::from_str(value) {
            return Ok((addr.ip().to_string(), addr.port()));
        }

        if value.matches(':').count() > 1 {
            return Err("IPv6 地址必须使用 [host]:port 格式".into());
        }

        match value.rsplit_once(':') {
            Some((host, port)) => {
                let port = port
                    .parse::<u16>()
                    .map_err(|_| format!("端口无效: {}", port))?;
                if host.trim().is_empty() {
                    Err("目标地址为空".into())
                } else {
                    Ok((host.trim().to_string(), port))
                }
            }
            None => Ok((value.to_string(), default_port)),
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
        let addr = socket_addr_string(&host, port);
        let stream = connect_tcp(&addr).map_err(|e| format!("连接 {} 失败: {}", addr, e))?;
        configure_tcp(&stream)?;
        let local = stream
            .local_addr()
            .map(|a| a.to_string())
            .unwrap_or_default();
        self.sock.lock().unwrap().replace(ConnSock::Tcp(stream));
        self.connected.store(true, Ordering::SeqCst);
        emit_status(
            &app,
            "connected",
            format!("已连接 {} (本地 {})", addr, local),
        );
        self.spawn_tcp_client_rx(app.clone());
        Ok(())
    }

    fn open_tcp_server<R: Runtime>(&mut self, app: AppHandle<R>) -> Result<(), String> {
        let ipv6_addr = format!("[::]:{}", self.port);
        let (listener, bind_addr) = match TcpListener::bind(&ipv6_addr) {
            Ok(listener) => (listener, ipv6_addr),
            Err(_) => {
                let ipv4_addr = format!("0.0.0.0:{}", self.port);
                let listener = TcpListener::bind(&ipv4_addr)
                    .map_err(|e| format!("监听 {} 失败: {}", ipv4_addr, e))?;
                (listener, ipv4_addr)
            }
        };
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        self.listener.lock().unwrap().replace(listener);
        self.connected.store(true, Ordering::SeqCst);
        emit_status(&app, "connected", format!("TCP Server 监听 {}", bind_addr));

        let listener = self.listener.clone();
        let clients = self.clients.clone();
        let stop = self.stop.clone();
        let connected = self.connected.clone();
        let workers = self.worker_threads.clone();
        let app2 = app.clone();
        self.threads.push(std::thread::spawn(move || {
            loop {
                if stop.load(Ordering::SeqCst) {
                    break;
                }
                let accepted = {
                    let guard = listener.lock().unwrap();
                    match guard.as_ref() {
                        Some(listener) => match listener.accept() {
                            Ok(pair) => Some(pair),
                            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => None,
                            Err(error) => {
                                emit_status(
                                    &app2,
                                    "lose",
                                    format!("TCP Server 监听失败: {}", error),
                                );
                                break;
                            }
                        },
                        None => break,
                    }
                };
                let (stream, addr) = match accepted {
                    Some(pair) => pair,
                    None => {
                        std::thread::sleep(Duration::from_millis(50));
                        continue;
                    }
                };

                if let Err(error) = configure_tcp(&stream) {
                    emit_status(
                        &app2,
                        "lose",
                        format!("客户端 {} 设置失败: {}", addr, error),
                    );
                    continue;
                }
                let read_stream = match stream.try_clone() {
                    Ok(read_stream) => read_stream,
                    Err(error) => {
                        emit_status(
                            &app2,
                            "lose",
                            format!("客户端 {} 初始化失败: {}", addr, error),
                        );
                        continue;
                    }
                };
                let client_count = {
                    let mut guard = clients.lock().unwrap();
                    guard.insert(addr, stream);
                    guard.len()
                };
                emit_status(
                    &app2,
                    "connected",
                    format!("客户端 {} 接入 (共 {})", addr, client_count),
                );

                let clients2 = clients.clone();
                let stop2 = stop.clone();
                let app3 = app2.clone();
                workers.lock().unwrap().push(std::thread::spawn(move || {
                    let mut stream = read_stream;
                    let mut buf = vec![0u8; 4096];
                    let mut closed = false;
                    while !stop2.load(Ordering::SeqCst) {
                        match stream.read(&mut buf) {
                            Ok(0) => {
                                closed = true;
                                break;
                            }
                            Ok(n) => emit_rx(&app3, &buf[..n]),
                            Err(ref error)
                                if error.kind() == std::io::ErrorKind::WouldBlock
                                    || error.kind() == std::io::ErrorKind::TimedOut => {}
                            Err(_) => {
                                closed = true;
                                break;
                            }
                        }
                    }
                    let remain = {
                        let mut guard = clients2.lock().unwrap();
                        guard.remove(&addr);
                        guard.len()
                    };
                    if closed && !stop2.load(Ordering::SeqCst) {
                        emit_status(
                            &app3,
                            "connected",
                            format!("客户端 {} 断开 (剩 {})", addr, remain),
                        );
                    }
                }));
            }
            if !stop.load(Ordering::SeqCst) {
                connected.store(false, Ordering::SeqCst);
                emit_status(&app2, "closed", "TCP Server 已停止监听".to_string());
            }
        }));
        Ok(())
    }

    fn open_udp<R: Runtime>(&mut self, app: AppHandle<R>) -> Result<(), String> {
        let target_addr = if self.mode == "client" && !self.target.trim().is_empty() {
            let (host, port) = Self::parse_target(&self.target, 0)?;
            Some(
                socket_addr_string(&host, port)
                    .parse::<SocketAddr>()
                    .map_err(|e| format!("目标地址无效: {}", e))?,
            )
        } else {
            None
        };
        let (sock, bind_addr) = match target_addr {
            Some(target) if target.is_ipv4() => {
                let bind_addr = format!("0.0.0.0:{}", self.port);
                let sock = UdpSocket::bind(&bind_addr)
                    .map_err(|e| format!("绑定 {} 失败: {}", bind_addr, e))?;
                (sock, bind_addr)
            }
            Some(_) => {
                let bind_addr = format!("[::]:{}", self.port);
                let sock = UdpSocket::bind(&bind_addr)
                    .map_err(|e| format!("绑定 {} 失败: {}", bind_addr, e))?;
                (sock, bind_addr)
            }
            None => {
                let ipv6_bind = format!("[::]:{}", self.port);
                let ipv4_bind = format!("0.0.0.0:{}", self.port);
                match UdpSocket::bind(&ipv6_bind) {
                    Ok(sock) => (sock, ipv6_bind),
                    Err(_) => {
                        let sock = UdpSocket::bind(&ipv4_bind)
                            .map_err(|e| format!("绑定 {} 失败: {}", ipv4_bind, e))?;
                        (sock, ipv4_bind)
                    }
                }
            }
        };
        sock.set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|e| e.to_string())?;
        self.sock.lock().unwrap().replace(ConnSock::Udp(sock));
        self.target_addr = target_addr;
        self.connected.store(true, Ordering::SeqCst);
        emit_status(
            &app,
            "connected",
            format!(
                "UDP 监听 {} ({})",
                bind_addr,
                if let Some(target) = self.target_addr {
                    format!("目标 {}", target)
                } else {
                    "等待发送方".into()
                }
            ),
        );
        self.spawn_udp_rx(app);
        Ok(())
    }

    fn spawn_tcp_client_rx<R: Runtime>(&mut self, app: AppHandle<R>) {
        let sock = self.sock.clone();
        let stop = self.stop.clone();
        let connected = self.connected.clone();
        let auto_reconnect = self.auto_reconnect;
        let interval = reconnect_duration(self.reconnect_interval);
        let target = match Self::parse_target(&self.target, 80) {
            Ok((host, port)) => socket_addr_string(&host, port),
            Err(_) => self.target.clone(),
        };
        self.threads.push(std::thread::spawn(move || {
            let mut buf = vec![0u8; 4096];
            while !stop.load(Ordering::SeqCst) {
                let read_result = {
                    let mut guard = sock.lock().unwrap();
                    match guard.as_mut() {
                        Some(ConnSock::Tcp(stream)) => stream.read(&mut buf),
                        _ => Err(std::io::Error::new(
                            std::io::ErrorKind::NotConnected,
                            "TCP 未连接",
                        )),
                    }
                };
                match read_result {
                    Ok(0) => {
                        connected.store(false, Ordering::SeqCst);
                        sock.lock().unwrap().take();
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        emit_status(&app, "lose", format!("{} 连接断开", target));
                        if !auto_reconnect {
                            emit_status(&app, "closed", "连接已关闭".to_string());
                            break;
                        }
                        reconnect_tcp(&sock, &stop, &connected, &app, &target, interval);
                    }
                    Ok(n) => emit_rx(&app, &buf[..n]),
                    Err(ref error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            || error.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(error) => {
                        connected.store(false, Ordering::SeqCst);
                        sock.lock().unwrap().take();
                        if stop.load(Ordering::SeqCst) {
                            break;
                        }
                        emit_status(&app, "lose", format!("{} 连接错误: {}", target, error));
                        if !auto_reconnect {
                            emit_status(&app, "closed", "连接已关闭".to_string());
                            break;
                        }
                        reconnect_tcp(&sock, &stop, &connected, &app, &target, interval);
                    }
                }
            }
            connected.store(false, Ordering::SeqCst);
            if stop.load(Ordering::SeqCst) {
                emit_status(&app, "closed", "连接已关闭".to_string());
            }
        }));
    }

    fn spawn_udp_rx<R: Runtime>(&mut self, app: AppHandle<R>) {
        let sock = self.sock.clone();
        let stop = self.stop.clone();
        let last_sender = self.last_sender.clone();
        self.threads.push(std::thread::spawn(move || {
            let mut buf = vec![0u8; 4096];
            while !stop.load(Ordering::SeqCst) {
                let received = {
                    let guard = sock.lock().unwrap();
                    match guard.as_ref() {
                        Some(ConnSock::Udp(socket)) => match socket.recv_from(&mut buf) {
                            Ok((n, sender)) => Some((n, sender)),
                            Err(ref error)
                                if error.kind() == std::io::ErrorKind::WouldBlock
                                    || error.kind() == std::io::ErrorKind::TimedOut =>
                            {
                                None
                            }
                            Err(_) => None,
                        },
                        _ => None,
                    }
                };
                if let Some((n, sender)) = received {
                    *last_sender.lock().unwrap() = Some(sender);
                    emit_rx(&app, &buf[..n]);
                }
            }
        }));
    }

    /// TCP 写入完整缓冲区；UDP client 发给配置目标，UDP server 默认回复最近发送方。
    pub fn send(&self, data: &[u8]) -> Result<usize, String> {
        let mut guard = self.sock.lock().unwrap();
        match guard.as_mut() {
            Some(ConnSock::Tcp(stream)) => {
                stream.write_all(data).map_err(|e| e.to_string())?;
                Ok(data.len())
            }
            Some(ConnSock::Udp(socket)) => {
                let target = self
                    .target_addr
                    .or_else(|| *self.last_sender.lock().unwrap())
                    .ok_or("UDP 目标未设置，尚未收到发送方")?;
                let written = socket.send_to(data, target).map_err(|e| e.to_string())?;
                if written == data.len() {
                    Ok(written)
                } else {
                    Err(format!("UDP 只发送了 {}/{} 字节", written, data.len()))
                }
            }
            _ => Err("连接未建立".into()),
        }
    }

    /// TCP server 向所有客户端完整写入；UDP server 向广播地址完整发送。
    pub fn broadcast(&self, data: &[u8]) -> Result<usize, String> {
        if self.protocol == "udp" {
            let mut guard = self.sock.lock().unwrap();
            let socket = match guard.as_mut() {
                Some(ConnSock::Udp(socket)) => socket,
                _ => return Err("UDP 未建立".into()),
            };
            let target = self
                .target_addr
                .unwrap_or_else(|| SocketAddr::new(IpAddr::V4(Ipv4Addr::BROADCAST), self.port));
            if target.ip().is_ipv4() {
                socket.set_broadcast(true).map_err(|e| e.to_string())?;
            }
            let written = socket.send_to(data, target).map_err(|e| e.to_string())?;
            return if written == data.len() {
                Ok(written)
            } else {
                Err(format!("UDP 只发送了 {}/{} 字节", written, data.len()))
            };
        }

        let mut clients = self.clients.lock().unwrap();
        let mut sent = 0usize;
        let mut dead = Vec::new();
        for (addr, stream) in clients.iter_mut() {
            if stream.write_all(data).is_ok() {
                sent += data.len();
            } else {
                dead.push(*addr);
            }
        }
        for addr in dead {
            clients.remove(&addr);
        }
        if sent == 0 && !data.is_empty() {
            Err("没有可用的 TCP 客户端".into())
        } else {
            Ok(sent)
        }
    }

    pub fn close(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.sock.lock().unwrap().take();
        self.listener.lock().unwrap().take();
        self.clients.lock().unwrap().clear();
        self.connected.store(false, Ordering::SeqCst);

        // 先等待 accept/client 主循环，再回收 accept 期间登记的 worker。
        for thread in self.threads.drain(..) {
            let _ = thread.join();
        }
        let workers = std::mem::take(&mut *self.worker_threads.lock().unwrap());
        for thread in workers {
            let _ = thread.join();
        }
    }
}

fn socket_addr_string(host: &str, port: u16) -> String {
    if host.contains(':') {
        format!("[{}]:{}", host.trim_matches(['[', ']']), port)
    } else {
        format!("{}:{}", host, port)
    }
}

fn connect_tcp(target: &str) -> std::io::Result<TcpStream> {
    let mut last_error = None;
    for address in target.to_socket_addrs()? {
        match TcpStream::connect_timeout(&address, Duration::from_secs(1)) {
            Ok(stream) => return Ok(stream),
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::AddrNotAvailable, "目标地址无效")
    }))
}

fn configure_tcp(stream: &TcpStream) -> Result<(), String> {
    stream.set_nodelay(true).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(Duration::from_millis(100)))
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn reconnect_duration(seconds: f64) -> Duration {
    if seconds.is_finite() && seconds > 0.0 {
        Duration::from_secs_f64(seconds.min(3600.0))
    } else {
        Duration::from_millis(100)
    }
}

fn reconnect_tcp<R: Runtime>(
    sock: &Arc<Mutex<Option<ConnSock>>>,
    stop: &Arc<AtomicBool>,
    connected: &Arc<AtomicBool>,
    app: &AppHandle<R>,
    target: &str,
    interval: Duration,
) {
    emit_status(
        app,
        "connecting",
        format!("{} 断开，{}ms 后重连...", target, interval.as_millis()),
    );
    while !stop.load(Ordering::SeqCst) {
        if sleep_until(stop, interval) {
            return;
        }
        match connect_tcp(target) {
            Ok(stream) => match configure_tcp(&stream) {
                Ok(()) => {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    sock.lock().unwrap().replace(ConnSock::Tcp(stream));
                    connected.store(true, Ordering::SeqCst);
                    emit_status(app, "connected", format!("重连成功 {}", target));
                    return;
                }
                Err(error) => emit_status(app, "connecting", format!("重连设置失败: {}", error)),
            },
            Err(error) => emit_status(app, "connecting", format!("重连失败: {}", error)),
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_basic() {
        let (host, port) = TcpUdpConn::parse_target("192.168.1.10:8080", 80).unwrap();
        assert_eq!(host, "192.168.1.10");
        assert_eq!(port, 8080);
    }

    #[test]
    fn parse_target_default_port() {
        let (host, port) = TcpUdpConn::parse_target("example.com", 80).unwrap();
        assert_eq!(host, "example.com");
        assert_eq!(port, 80);
    }

    #[test]
    fn parse_target_http_prefix_and_slash() {
        let (host, port) = TcpUdpConn::parse_target("http://host:1234/", 80).unwrap();
        assert_eq!(host, "host");
        assert_eq!(port, 1234);
    }

    #[test]
    fn parse_target_ipv6() {
        let (host, port) = TcpUdpConn::parse_target("[::1]:8080", 80).unwrap();
        assert_eq!(host, "::1");
        assert_eq!(port, 8080);
        assert_eq!(socket_addr_string(&host, port), "[::1]:8080");
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
