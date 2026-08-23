// 高频压测：验证接收线程 + 事件推送在高吞吐下不崩溃、不丢帧
use serial_aid_lib::conn::{ConnConfig, ConnManager, TcpUdpConfig};
use std::io::Write;
use std::net::TcpListener;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tauri::Listener;

#[test]
fn high_frequency_rx_stress() {
    // 高频 TCP 数据源（固定 16 MiB，必须完整到达）
    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let sent_bytes = Arc::new(AtomicUsize::new(0));

    let sent2 = sent_bytes.clone();
    let producer = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let chunk = vec![0xAAu8; 4096];
        for _ in 0..4096 {
            stream.write_all(&chunk).unwrap();
            sent2.fetch_add(chunk.len(), Ordering::SeqCst);
        }
        let _ = stream.shutdown(std::net::Shutdown::Both);
    });

    let manager = ConnManager::new();
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();

    let rx_count = Arc::new(AtomicUsize::new(0));
    let rx_bytes = Arc::new(AtomicUsize::new(0));
    let rx_count2 = rx_count.clone();
    let rx_bytes2 = rx_bytes.clone();
    let _ = app.listen("rx-data", move |ev| {
        if let Ok(payload) = serde_json::from_str::<serde_json::Value>(ev.payload()) {
            if let Some(data) = payload["data"].as_array() {
                rx_count2.fetch_add(1, Ordering::SeqCst);
                rx_bytes2.fetch_add(data.len(), Ordering::SeqCst);
            }
        }
    });

    let cfg = ConnConfig::TcpUdp(TcpUdpConfig {
        protocol: "tcp".into(),
        mode: "client".into(),
        target: format!("127.0.0.1:{}", port),
        port,
        local_port: 0,
        auto_reconnect: false,
        reconnect_interval: 1.0,
    });
    manager.open(cfg, handle).expect("open failed");

    producer.join().unwrap();
    for _ in 0..500 {
        if rx_bytes.load(Ordering::SeqCst) >= sent_bytes.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    let sent = sent_bytes.load(Ordering::SeqCst);
    let received = rx_bytes.load(Ordering::SeqCst);
    let frames = rx_count.load(Ordering::SeqCst);

    println!(
        "stress: sent={} bytes, received={} bytes ({} frames), throughput={:.1} MB/s",
        sent,
        received,
        frames,
        received as f64 / 1024.0 / 1024.0
    );

    assert_eq!(received, sent, "TCP 压测数据未完整送达");
    assert!(frames > 0, "无帧到达");

    manager.close();
}
