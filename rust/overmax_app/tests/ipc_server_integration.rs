//! IPC 서버 통합 테스트: 실제 소켓을 통해 SSE 스트림과 JSON-RPC를 검증한다.
//!
//! 검증 항목 (docs/plans/2026-08-26-ipc-service-architecture.md):
//! - 대역 바인딩 및 endpoint 파일 기록 (§5.1)
//! - Host 헤더 검증 (§5.4 보호 가드)
//! - SSE named-event 엔벨로프 + 접속 시 state_snapshot 선송신 (§5.3)
//! - JSON-RPC 2.0 get_current_context (§5.4)

use overmax_app::system::ipc_server::{
    spawn_ipc_manager, update_latest_state, BoundPortSlot, IpcEvent,
};
use overmax_core::{GameSessionState, PlayContext, SceneType};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const TEST_PORT: u16 = 30197;

fn test_settings(port: u16, enabled: bool) -> Arc<Mutex<Value>> {
    Arc::new(Mutex::new(json!({
        "ipc": { "enabled": enabled, "port": port }
    })))
}

fn wait_bound_port(slot: &BoundPortSlot, timeout: Duration) -> Option<u16> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(guard) = slot.lock() {
            if let Some(p) = *guard {
                return Some(p);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    None
}

fn http_get_events(port: u16, host: &str) -> std::io::Result<TcpStream> {
    let mut s = TcpStream::connect(("127.0.0.1", port))?;
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    write!(
        s,
        "GET /events HTTP/1.1\r\nHost: {host}\r\nAccept: text/event-stream\r\n\r\n"
    )?;
    Ok(s)
}

fn read_until_data(reader: &mut BufReader<TcpStream>) -> Option<Value> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if Instant::now() > deadline {
            return None;
        }
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => return None,
            Ok(_) => {}
            Err(_) => return None,
        }
        if let Some(data) = line.strip_prefix("data: ") {
            let value: Value = serde_json::from_str(data.trim()).ok()?;
            // 엔벨로프 필수 필드 검증 (§5.3)
            assert_eq!(
                value.get("protocol").and_then(Value::as_str),
                Some("overmax-ipc/1")
            );
            assert!(value.get("seq").and_then(Value::as_u64).is_some());
            assert!(value.get("ts_ms").is_some());
            return Some(value);
        }
    }
}

fn stable_state(song_id: i32) -> GameSessionState {
    GameSessionState {
        scene: SceneType::Freestyle,
        context: Some(PlayContext {
            song_id,
            mode: overmax_core::Mode::B5,
            diff: overmax_core::Difficulty::SC,
            rate: 99.23,
            is_max_combo: false,
        }),
        is_stable: true,
        is_fullscreen: false,
    }
}

#[test]
fn ipc_sse_stream_and_rpc_end_to_end() {
    let root = tempfile::tempdir().expect("tempdir");
    let settings = test_settings(TEST_PORT, true);

    let (publisher, handle, slot) = spawn_ipc_manager(root.path().to_path_buf(), settings, "test");

    // ── 바인딩 대기 + endpoint 파일 검증 ──
    let port =
        wait_bound_port(&slot, Duration::from_secs(5)).expect("server did not bind within timeout");
    assert!(
        (30100..=30199).contains(&port),
        "bound port {port} outside recommended band"
    );

    let endpoint_path = root.path().join("cache").join("ipc_endpoint.json");
    let mut waited = Instant::now();
    let endpoint: Value = loop {
        if let Ok(body) = std::fs::read_to_string(&endpoint_path) {
            break serde_json::from_str(&body).expect("endpoint json parse");
        }
        assert!(
            waited.elapsed() < Duration::from_secs(5),
            "endpoint file missing"
        );
        std::thread::sleep(Duration::from_millis(50));
        waited = Instant::now();
    };
    assert_eq!(
        endpoint.get("port").and_then(Value::as_u64),
        Some(port as u64)
    );

    // ── 안정화 상태 등록 → 이후 접속의 state_snapshot 원천이 됨 ──
    update_latest_state(stable_state(1234));

    // ── SSE 접속: 정상 Host ──
    let stream = http_get_events(port, "127.0.0.1").expect("connect /events");
    let mut reader = BufReader::new(stream.try_clone().unwrap());

    let mut status_line = String::new();
    reader.read_line(&mut status_line).expect("status line");
    assert!(
        status_line.contains("200"),
        "unexpected status: {status_line}"
    );
    let mut saw_event_stream = false;
    loop {
        let mut header = String::new();
        reader.read_line(&mut header).expect("header");
        if header.trim().is_empty() {
            break;
        }
        if header.to_ascii_lowercase().contains("text/event-stream") {
            saw_event_stream = true;
        }
    }
    assert!(saw_event_stream, "missing Content-Type: text/event-stream");

    // 접속 직후 state_snapshot 선송신 검증
    let first = read_until_data(&mut reader).expect("no first frame");
    assert_eq!(
        first.get("type").and_then(Value::as_str),
        Some("state_snapshot"),
        "first event must be state_snapshot"
    );
    let ctx_payload = &first["payload"]["context"];
    assert_eq!(
        ctx_payload.get("song_id").and_then(Value::as_i64),
        Some(1234)
    );

    // 라이브 이벤트 푸시 검증 (scene_detected)
    publisher.publish(IpcEvent::SceneDetected {
        scene: "Freestyle".into(),
    });
    let pushed = read_until_data(&mut reader).expect("no pushed frame");
    assert_eq!(
        pushed.get("type").and_then(Value::as_str),
        Some("scene_detected")
    );

    drop(reader);
    drop(stream);

    // ── Host 검증 가드: 외부 호칭은 403 ──
    let mut evil = TcpStream::connect(("127.0.0.1", port)).expect("connect evil");
    write!(
        evil,
        "GET /events HTTP/1.1\r\nHost: attacker.example.com\r\n\r\n"
    )
    .unwrap();
    let mut evil_reader = BufReader::new(evil);
    let mut status = String::new();
    evil_reader.read_line(&mut status).unwrap();
    assert!(status.contains("403"), "host guard must reject: {status}");

    // ── JSON-RPC: get_current_context ──
    let mut rpc = TcpStream::connect(("127.0.0.1", port)).expect("connect rpc");
    let body = json!({"jsonrpc":"2.0","id":7,"method":"get_current_context"});
    let payload = body.to_string();
    write!(
        rpc,
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        payload.len(),
        payload
    )
    .unwrap();
    let mut rpc_reader = BufReader::new(rpc);
    let mut rpc_status = String::new();
    rpc_reader.read_line(&mut rpc_status).unwrap();
    assert!(rpc_status.contains("200"), "rpc failed: {rpc_status}");
    let _rpc_body = String::new();
    let mut content_len = 0usize;
    loop {
        let mut line = String::new();
        rpc_reader.read_line(&mut line).unwrap();
        let lower = line.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("content-length:") {
            content_len = v.trim().parse().unwrap_or(0);
        }
        if line.trim().is_empty() {
            break;
        }
    }
    let mut buf = vec![0u8; content_len];
    rpc_reader.read_exact(&mut buf).unwrap();
    let resp: Value = serde_json::from_slice(&buf).unwrap();
    assert_eq!(resp.get("id").and_then(Value::as_i64), Some(7));
    assert_eq!(
        resp["result"]["context"]["song_id"].as_i64(),
        Some(1234),
        "rpc must return latest snapshot context"
    );

    // ── Content-Type 강제 가드: text/plain POST는 415 ──
    let mut plain = TcpStream::connect(("127.0.0.1", port)).expect("connect plain");
    write!(
        plain,
        "POST /rpc HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: text/plain\r\nContent-Length: 2\r\n\r\n{{}}"
    )
    .unwrap();
    let mut plain_reader = BufReader::new(plain);
    let mut plain_status = String::new();
    plain_reader.read_line(&mut plain_status).unwrap();
    assert!(
        plain_status.contains("415"),
        "content-type guard failed: {plain_status}"
    );

    // ── 종료 처리: shutdown 플래그 설정이 오류 없이 완료되는지 확인 ──
    handle.shutdown();
}
