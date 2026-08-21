// 端到端集成测试：ConnManager → TCP client/server/UDP 收发环回
use serial_aid_lib::conn::{ConnConfig, ConnManager, TcpUdpConfig};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::Listener;

/// 起一个 TCP echo server（发 hello，回显收到的数据；收到 ping-123 后退出）
fn start_echo_server(port: u16) -> Arc<Mutex<Vec<u8>>> {
    let received = Arc::new(Mutex::new(Vec::new()));
    let received2 = received.clone();
    thread::spawn(move || {
        let listener = TcpListener::bind(("127.0.0.1", port)).unwrap();
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        stream.write_all(b"hello from echo").unwrap();
        let mut buf = [0u8; 256];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    received2.lock().unwrap().extend_from_slice(&buf[..n]);
                    let _ = stream.write_all(&buf[..n]);
                    if received2.lock().unwrap().windows(8).any(|w| w == b"ping-123") {
                        break;
                    }
                }
                Err(ref e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    // 等待客户端发送
                    continue;
                }
                Err(_) => break,
            }
        }
    });
    received
}

fn free_port() -> u16 {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    l.local_addr().unwrap().port()
}

#[test]
fn tcp_client_roundtrip() {
    let port = free_port();
    let echo_received = start_echo_server(port);

    let manager = ConnManager::new();
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();

    // 监听 rx-data 事件
    let rx_frames = Arc::new(Mutex::new(Vec::new()));
    let rx_frames2 = rx_frames.clone();
    let _ = app.listen("rx-data", move |ev| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&ev.payload()) {
            if let Some(data) = payload["data"].as_array() {
                let bytes: Vec<u8> = data
                    .iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect();
                rx_frames2.lock().unwrap().push(bytes);
            }
        }
    });

    let cfg = ConnConfig::TcpUdp(TcpUdpConfig {
        protocol: "tcp".into(),
        mode: "client".into(),
        target: format!("127.0.0.1:{}", port),
        port,
        auto_reconnect: false,
        reconnect_interval: 1.0,
    });
    manager.open(cfg, handle).expect("open failed");

    thread::sleep(Duration::from_millis(300));
    assert!(!rx_frames.lock().unwrap().is_empty(), "未收到服务端数据");
    let first = rx_frames.lock().unwrap()[0].clone();
    assert_eq!(first, b"hello from echo", "欢迎数据不匹配");

    manager.send(b"ping-123").expect("send failed");

    thread::sleep(Duration::from_millis(300));
    let frames = rx_frames.lock().unwrap();
    assert!(frames.iter().any(|f| f == b"ping-123"), "未收到回显: {:?}", frames);

    let recv = echo_received.lock().unwrap();
    assert_eq!(&recv[..], b"ping-123", "服务端收到数据不匹配");

    manager.close();
}

#[test]
fn tcp_server_broadcast() {
    let port = free_port();
    let manager = ConnManager::new();
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();

    let cfg = ConnConfig::TcpUdp(TcpUdpConfig {
        protocol: "tcp".into(),
        mode: "server".into(),
        target: String::new(),
        port,
        auto_reconnect: false,
        reconnect_interval: 1.0,
    });
    manager.open(cfg, handle).expect("open server failed");

    let client = thread::spawn(move || {
        let mut stream = std::net::TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(b"client-data").unwrap();
        let mut buf = [0u8; 64];
        let n = stream.read(&mut buf).unwrap();
        buf[..n].to_vec()
    });

    thread::sleep(Duration::from_millis(300));
    manager.send(b"server-bcast").expect("broadcast failed");

    let got = client.join().unwrap();
    assert_eq!(got, b"server-bcast", "客户端未收到广播");

    manager.close();
}

#[test]
fn udp_client_roundtrip() {
    use std::net::UdpSocket;
    let sock = UdpSocket::bind(("127.0.0.1", 0)).unwrap();
    let port = sock.local_addr().unwrap().port();
    let server = thread::spawn(move || {
        let mut buf = [0u8; 256];
        let (n, src) = sock.recv_from(&mut buf).unwrap();
        let data = buf[..n].to_vec();
        sock.send_to(&data, src).unwrap();
        data
    });

    let manager = ConnManager::new();
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();

    let rx_frames = Arc::new(Mutex::new(Vec::new()));
    let rx_frames2 = rx_frames.clone();
    let _ = app.listen("rx-data", move |ev| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(&ev.payload()) {
            if let Some(data) = payload["data"].as_array() {
                let bytes: Vec<u8> = data
                    .iter()
                    .filter_map(|v| v.as_u64().map(|n| n as u8))
                    .collect();
                rx_frames2.lock().unwrap().push(bytes);
            }
        }
    });

    let cfg = ConnConfig::TcpUdp(TcpUdpConfig {
        protocol: "udp".into(),
        mode: "client".into(),
        target: format!("127.0.0.1:{}", port),
        port: 0,
        auto_reconnect: false,
        reconnect_interval: 1.0,
    });
    manager.open(cfg, handle).expect("open udp failed");

    thread::sleep(Duration::from_millis(200));
    manager.send(b"udp-ping").expect("udp send failed");

    let echo = server.join().unwrap();
    assert_eq!(echo, b"udp-ping", "UDP 服务端收到数据不匹配");

    thread::sleep(Duration::from_millis(300));
    let frames = rx_frames.lock().unwrap();
    assert!(frames.iter().any(|f| f == b"udp-ping"), "UDP 未收到回显: {:?}", frames);

    manager.close();
}
