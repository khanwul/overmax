use overmax_core::GameSessionState;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// 동시 SSE 클라이언트 상한 (localhost 소수 가정의 안전 가드)
const MAX_CLIENTS: usize = 16;
/// GUI → 허브 이벤트 큐 크기. 가득 차면 drop (원칙 ①: 게임은 절대 기다리지 않는다)
const HUB_CHANNEL_CAPACITY: usize = 64;
/// 매니저/허브 폴링 주기 (설정 변경 감지 겸용)
const POLL_MS: u64 = 250;
/// SSE 하트비트 간격
const HEARTBEAT_SECS: u64 = 15;
/// RPC 본문 상한
const MAX_RPC_BODY: usize = 64 * 1024;

pub const PROTOCOL_ID: &str = "overmax-ipc/1";

// ─────────────────────────────────────────────────────────────────────────────
// 퍼블릭 엔트리포인트
// ─────────────────────────────────────────────────────────────────────────────

/// GUI 스레드가 이벤트를 발행하는 핸들.
///
/// 허브 채널은 앱 수명 전체에서 유지되므로 이 핸들은 항상 유효하다.
/// `publish`는 절대 블록하지 않으며(try_send), 큐가 가득 차거나
/// 구독자가 없으면 이벤트를 조용히 drop한다 (telemetry 성격의 최소 보장).
#[derive(Clone)]
pub struct IpcPublisher {
    tx: SyncSender<IpcEvent>,
}

impl IpcPublisher {
    pub fn publish(&self, event: IpcEvent) {
        let _ = self.tx.try_send(event);
    }
}

/// IPC 매니저 종료 플래그 (앱 종료 시 호출)
#[derive(Clone)]
pub struct IpcServerHandle {
    shutdown: Arc<AtomicBool>,
}

impl IpcServerHandle {
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

/// 현재 바인딩 상태. `None` = IPC 비활성 (설정 OFF 또는 바인딩 실패).
pub type BoundPortSlot = Arc<Mutex<Option<u16>>>;

/// IPC 매니저 기동 (매니저 스레드 1개 + 허브 스레드 1개 + 접속별 단기 스레드).
///
/// - 허브는 앱 수명 내내 유지되어 `IpcPublisher`가 항상 유효하다.
/// - 리스너는 설정(`ipc.enabled`, `ipc.port`)을 폴링하여 ON/OFF·포트 변경에
///   따라 런타임에 바인딩/해제/재바인딩한다 (§6.1).
/// - 바인딩 실패 시 대역 내 순차 재시도 후 전부 실패하면 fail-closed로
///   비활성 상태를 유지하고 다음 틱에 재시도한다 (게임/오버레이 무영향).
pub fn spawn_ipc_manager(
    root: PathBuf,
    settings: Arc<Mutex<Value>>,
    app_version: &'static str,
) -> (IpcPublisher, IpcServerHandle, BoundPortSlot) {
    let shutdown = Arc::new(AtomicBool::new(false));
    let bound_slot: BoundPortSlot = Arc::new(Mutex::new(None));

    // 허브 채널: GUI(publish) → 허브 → 클라이언트 writer fan-out
    let (hub_tx, hub_rx) = sync_channel::<IpcEvent>(HUB_CHANNEL_CAPACITY);
    // 접속 완료된 SSE 소켓: 매니저 → 허브
    let (new_client_tx, new_client_rx) = std::sync::mpsc::channel::<TcpStream>();

    // 허브 스레드 (수명: 프로세스 전체)
    {
        let shutdown = shutdown.clone();
        std::thread::Builder::new()
            .name("ipc-hub".into())
            .spawn(move || hub_loop(hub_rx, new_client_rx, app_version, shutdown))
            .expect("ipc-hub thread");
    }

    // 매니저 스레드 (설정 폴링 + 리스너 소유 + accept)
    {
        let shutdown = shutdown.clone();
        let bound_slot = bound_slot.clone();
        let settings = settings.clone();
        std::thread::Builder::new()
            .name("ipc-manager".into())
            .spawn(move || {
                manager_loop(root, settings, new_client_tx, bound_slot, shutdown);
            })
            .expect("ipc-manager thread");
    }

    (
        IpcPublisher { tx: hub_tx },
        IpcServerHandle { shutdown },
        bound_slot,
    )
}

// ─────────────────────────────────────────────────────────────────────────────
// 이벤트 타입 (엔벨로프 §5.3)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum IpcEvent {
    /// 씬 감지 통지 (관찰된 '감지'를 '통지' — 원칙 ②)
    SceneDetected { scene: String },
    /// 곡 컨텍스트 확정 통지 (stable 상태만)
    SongDetected {
        song_id: i32,
        mode: String,
        diff: String,
        rate: f32,
        is_max_combo: bool,
    },
    /// 결과창 플레이 확정 통지 (verified flow 용어 계승)
    PlayVerified {
        song_id: i32,
        mode: String,
        diff: String,
        rate: f32,
        is_max_combo: bool,
    },
    /// 접속 직후 초기 상태 선송신
    StateSnapshot { payload: Value },
}

impl IpcEvent {
    fn sse_name(&self) -> &'static str {
        match self {
            IpcEvent::SceneDetected { .. } => "scene_detected",
            IpcEvent::SongDetected { .. } => "song_detected",
            IpcEvent::PlayVerified { .. } => "play_verified",
            IpcEvent::StateSnapshot { .. } => "state_snapshot",
        }
    }

    fn payload(&self) -> Value {
        match self {
            IpcEvent::SceneDetected { scene } => json!({ "scene": scene }),
            IpcEvent::SongDetected {
                song_id,
                mode,
                diff,
                rate,
                is_max_combo,
            }
            | IpcEvent::PlayVerified {
                song_id,
                mode,
                diff,
                rate,
                is_max_combo,
            } => json!({
                "song_id": song_id,
                "mode": mode,
                "diff": diff,
                "rate": rate,
                "is_max_combo": is_max_combo,
            }),
            IpcEvent::StateSnapshot { payload } => payload.clone(),
        }
    }
}

fn format_sse_frame(name: &str, seq: u64, payload: Value, app_version: &str) -> String {
    let data = json!({
        "protocol": PROTOCOL_ID,
        "type": name,
        "seq": seq,
        "ts_ms": now_ms(),
        "app_version": app_version,
        "payload": payload,
    });
    format!("event: {name}\ndata: {data}\n\n")
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

// ─────────────────────────────────────────────────────────────────────────────
// 허브: 이벤트 fan-out + 클라이언트 레지스트리
// ─────────────────────────────────────────────────────────────────────────────

struct HubClient {
    writer: SyncSender<IpcEvent>,
}

fn hub_loop(
    hub_rx: std::sync::mpsc::Receiver<IpcEvent>,
    new_client_rx: std::sync::mpsc::Receiver<TcpStream>,
    app_version: &'static str,
    shutdown: Arc<AtomicBool>,
) {
    let mut clients: HashMap<u64, HubClient> = HashMap::new();
    let mut next_client_id = 0u64;

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        // 새 SSE 클라이언트 수용 → 등록 즉시 state_snapshot 선송신 (§5.3)
        while let Ok(stream) = new_client_rx.try_recv() {
            if clients.len() >= MAX_CLIENTS {
                log("client rejected: max clients reached");
                drop(stream);
                continue;
            }
            let id = next_client_id;
            next_client_id += 1;

            let (writer_tx, writer_rx) = sync_channel::<IpcEvent>(HUB_CHANNEL_CAPACITY);
            let version = app_version;
            std::thread::Builder::new()
                .name(format!("ipc-client-{id}"))
                .spawn(move || client_writer_loop(writer_rx, stream, version))
                .ok();
            clients.insert(id, HubClient { writer: writer_tx });

            // 초기 스냅샷: 해당 클라이언트 채널로만 1회 선송신
            if let Some(state) = latest_snapshot() {
                let snapshot = IpcEvent::StateSnapshot {
                    payload: snapshot_json(&state),
                };
                let _ = clients.get_mut(&id).unwrap().writer.try_send(snapshot);
            }
        }

        // 이벤트 fan-out
        match hub_rx.recv_timeout(Duration::from_millis(POLL_MS)) {
            Ok(event) => {
                // 느린/끊긴 클라이언트는 try_send 실패 시 즉시 정리 —
                // 한 클라이언트가 다른 클라이언트의 배포를 막지 않는다.
                let mut dead_ids = Vec::new();
                for (&id, client) in clients.iter_mut() {
                    if client.writer.try_send(event.clone()).is_err() {
                        dead_ids.push(id);
                    }
                }
                for id in dead_ids {
                    clients.remove(&id);
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                // 발행자(GUI) 소멸 — 앱 종료 경로
                break;
            }
        }
    }
    log("hub stopped");
}

fn client_writer_loop(
    rx: std::sync::mpsc::Receiver<IpcEvent>,
    mut stream: TcpStream,
    app_version: &'static str,
) {
    // SSE 응답 헤더는 handshake 단계에서 이미 기록되어 있다.
    let mut seq: u64 = 0;
    let mut last_heartbeat = std::time::Instant::now();

    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(event) => {
                let name = event.sse_name();
                let frame = format_sse_frame(name, seq, event.payload(), app_version);
                if stream.write_all(frame.as_bytes()).is_err() || stream.flush().is_err() {
                    return; // 소켓 사망 — 허브가 try_send 실패로 정리
                }
                seq += 1; // 연결별 단조 증가
                last_heartbeat = std::time::Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {
                // 하트비트: 무활동 구간에 주석 행 1회 (§5.3)
                if last_heartbeat.elapsed() >= Duration::from_secs(HEARTBEAT_SECS) {
                    if stream.write_all(b": ping\n\n").is_err() || stream.flush().is_err() {
                        return;
                    }
                    last_heartbeat = std::time::Instant::now();
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = stream.shutdown(std::net::Shutdown::Both);
                return;
            }
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 매니저: 설정 폴링 + 리스너 바인딩/재바인딩 + accept
// ─────────────────────────────────────────────────────────────────────────────

struct RunningListener {
    listener: TcpListener,
    port: u16,
}

fn manager_loop(
    root: PathBuf,
    settings: Arc<Mutex<Value>>,
    new_client_tx: std::sync::mpsc::Sender<TcpStream>,
    bound_slot: BoundPortSlot,
    shutdown: Arc<AtomicBool>,
) {
    let mut running: Option<RunningListener> = None;
    let tick = Duration::from_millis(POLL_MS);

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        let desired = read_ipc_settings(&settings);
        let current_port = running.as_ref().map(|r| r.port);

        if !desired.enabled {
            if running.take().is_some() {
                log(format!(
                    "IPC disabled by settings (was 127.0.0.1:{})",
                    current_port.unwrap_or(0)
                ));
                set_bound_port(&bound_slot, None);
                write_endpoint_file(&root, None);
            }
            std::thread::sleep(tick);
            continue;
        }

        // 포트 변경 감지 → 재바인딩
        if running.is_some() && current_port != Some(desired.port) {
            running.take();
            set_bound_port(&bound_slot, None);
        }

        // 바인딩 시도 (실패 시 대역 스캔 → 전부 실패하면 다음 틱에 재시도)
        if running.is_none() {
            match bind_with_fallback(desired.port) {
                Some((listener, port)) => {
                    log(format!("IPC listening on 127.0.0.1:{port}"));
                    set_bound_port(&bound_slot, Some(port));
                    write_endpoint_file(&root, Some(port));
                    running = Some(RunningListener { listener, port });
                }
                None => {
                    log("IPC bind failed across band; retrying next tick");
                    std::thread::sleep(tick);
                    continue;
                }
            }
        }

        // accept 틱 (nonblocking)
        if let Some(r) = running.as_ref() {
            match r.listener.accept() {
                Ok((stream, addr)) => {
                    if addr.ip().is_loopback() {
                        // 핸드셰이크는 단기 스레드에서 — 느린 클라이언트가
                        // accept 루프 전체를 막지 않도록 한다.
                        let tx = new_client_tx.clone();
                        std::thread::Builder::new()
                            .name("ipc-handshake".into())
                            .spawn(move || {
                                handshake(stream, &tx);
                            })
                            .ok();
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => {
                    // 일시적 오류: 리스너를 재생성하지 말고 다음 틱에 재시도
                }
            }
        }

        std::thread::sleep(tick);
    }

    log("manager stopped");
}

fn set_bound_port(slot: &BoundPortSlot, port: Option<u16>) {
    if let Ok(mut guard) = slot.lock() {
        *guard = port;
    }
}

fn write_endpoint_file(root: &std::path::Path, port: Option<u16>) {
    let dir = root.join("cache");
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
    }
    let body = match port {
        Some(p) => json!({ "protocol": PROTOCOL_ID, "host": "127.0.0.1", "port": p }),
        None => json!({ "protocol": PROTOCOL_ID, "host": null, "port": null }),
    };
    let tmp = dir.join("ipc_endpoint.json.tmp");
    let target = dir.join("ipc_endpoint.json");
    if std::fs::write(&tmp, body.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &target); // 원자적 교체
    }
}

struct DesiredIpcSettings {
    enabled: bool,
    port: u16,
}

fn read_ipc_settings(settings: &Arc<Mutex<Value>>) -> DesiredIpcSettings {
    let default_port = overmax_data::config::settings::IpcSettings::default().port;
    let read = |settings: &Arc<Mutex<Value>>| -> (bool, Option<u16>) {
        let guard = settings.lock().ok();
        let Some(v) = guard else { return (false, None) };
        let ipc = v.get("ipc");
        let enabled = ipc
            .and_then(|i| i.get("enabled"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let port = ipc
            .and_then(|i| i.get("port"))
            .and_then(Value::as_u64)
            .and_then(|p| u16::try_from(p).ok())
            .filter(|&p| (1024..=65535).contains(&p));
        (enabled, port)
    };
    let (enabled, port) = read(settings);
    DesiredIpcSettings {
        enabled,
        port: port.unwrap_or(default_port),
    }
}

fn bind_with_fallback(preferred: u16) -> Option<(TcpListener, u16)> {
    let band = overmax_data::config::settings::IPC_PORT_BAND;
    let candidates = std::iter::once(preferred).chain(band.clone().filter(|p| *p != preferred));
    for candidate in candidates {
        if let Ok(l) = TcpListener::bind(("127.0.0.1", candidate)) {
            return Some((l, candidate));
        }
    }
    None
}

// ─────────────────────────────────────────────────────────────────────────────
// HTTP 핸드셰이크 + 라우팅 (/events, /rpc, /)
// ─────────────────────────────────────────────────────────────────────────────

fn handshake(mut stream: TcpStream, new_client_tx: &std::sync::mpsc::Sender<TcpStream>) {
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let Ok(peer) = stream.peer_addr() else { return };
    if !peer.ip().is_loopback() {
        return; // 원칙 ④: 로컬 신뢰 — 외부 출처 원천 거절
    }

    let mut reader = BufReader::new(match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    });

    let mut request_line = String::new();
    if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
        return;
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_ascii_uppercase();
    let raw_path = parts.next().unwrap_or("/");
    let route = raw_path.split('?').next().unwrap_or("/").to_string();

    // 헤더 파싱: Host 검증(DNS 리바인딩 차단) + Content-Type/Length (RPC용)
    let mut host_ok = false;
    let mut content_type = String::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            break;
        }
        let lower = trimmed.to_ascii_lowercase();
        if let Some(v) = lower.strip_prefix("host:") {
            let value = trimmed[trimmed.len() - v.len()..].trim();
            host_ok = value.starts_with("127.0.0.1")
                || value.starts_with("localhost")
                || value.starts_with("[::1]");
        } else if let Some(v) = lower.strip_prefix("content-type:") {
            content_type = trimmed[trimmed.len() - v.len()..]
                .trim()
                .to_ascii_lowercase();
        } else if let Some(v) = lower.strip_prefix("content-length:") {
            content_length = trimmed[trimmed.len() - v.len()..]
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }

    if !host_ok {
        respond_json(
            &mut stream,
            403,
            "Forbidden",
            &json!({"error": "invalid host"}),
            false,
        );
        return;
    }

    match (method.as_str(), route.as_str()) {
        ("GET", "/events") => {
            // SSE 업그레이드: ACAO 헤더를 의도적으로 생략하여 웹페이지의
            // cross-origin 구독을 브라우저 기본 정책으로 차단한다.
            let headers = "HTTP/1.1 200 OK\r\n\
                Content-Type: text/event-stream\r\n\
                Cache-Control: no-cache\r\n\
                Connection: keep-alive\r\n\r\n";
            if stream.write_all(headers.as_bytes()).is_ok() && stream.flush().is_ok() {
                let _ = new_client_tx.send(stream);
            }
        }
        ("GET", "/") => {
            let manifest = json!({
                "protocol": PROTOCOL_ID,
                "name": "overmax",
                "events": "/events",
                "rpc": "/rpc",
            });
            respond_json(&mut stream, 200, "OK", &manifest, true);
        }
        ("POST", "/rpc") => {
            // Content-Type 강제 → 악성 웹페이지의 simple-request RPC 차단
            // (application/json은 CORS preflight를 유발하고 우리는 OPTIONS에
            // 응답하지 않으므로 preflight가 실패한다)
            if !content_type.starts_with("application/json") {
                respond_json(
                    &mut stream,
                    415,
                    "Unsupported Media Type",
                    &json!({"error": "content-type must be application/json"}),
                    false,
                );
                return;
            }
            handle_rpc(&mut reader, &mut stream, content_length);
        }
        _ => {
            respond_json(
                &mut stream,
                404,
                "Not Found",
                &json!({"error": "not found"}),
                false,
            );
        }
    }
}

fn respond_json(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    body: &Value,
    allow_any_origin: bool,
) {
    let payload = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
    let acao = if allow_any_origin {
        "Access-Control-Allow-Origin: *\r\n"
    } else {
        ""
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{acao}Connection: close\r\n\r\n",
        payload.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(&payload);
    let _ = stream.flush();
}

// ─────────────────────────────────────────────────────────────────────────────
// RPC (JSON-RPC 2.0 서브셋 §5.4)
// ─────────────────────────────────────────────────────────────────────────────

fn handle_rpc<R: Read>(reader: &mut BufReader<R>, stream: &mut TcpStream, content_length: usize) {
    if content_length == 0 || content_length > MAX_RPC_BODY {
        respond_json(
            stream,
            400,
            "Bad Request",
            &json!({"error": "invalid body length"}),
            false,
        );
        return;
    }

    let mut body = vec![0u8; content_length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let outcome = dispatch_rpc(&String::from_utf8_lossy(&body));
    let (status, reason) = if outcome.is_error {
        (500, "Internal Server Error")
    } else {
        (200, "OK")
    };
    respond_json(stream, status, reason, &outcome.body, false);
}

struct RpcOutcome {
    body: Value,
    is_error: bool,
}

fn rpc_error(id: Value, code: i32, message: &str) -> RpcOutcome {
    RpcOutcome {
        body: json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}}),
        is_error: true,
    }
}

/// JSON-RPC 2.0 디스패치. v1 메서드:
/// - `get_current_context`: 최근 세션 상태 조회 (스냅샷 캐시 기반)
/// - `list_methods`: 지원 메서드 목록 (발견용)
fn dispatch_rpc(body: &str) -> RpcOutcome {
    let Ok(req) = serde_json::from_str::<Value>(body) else {
        return rpc_error(Value::Null, -32700, "Parse error");
    };

    let id = req.get("id").cloned().unwrap_or(Value::Null);
    let Some(method) = req.get("method").and_then(Value::as_str) else {
        return rpc_error(id, -32600, "Invalid Request");
    };

    match method {
        "list_methods" => RpcOutcome {
            body: json!({
                "jsonrpc":"2.0","id":id,
                "result": {"methods": ["get_current_context", "list_methods"],
                           "protocol": PROTOCOL_ID}
            }),
            is_error: false,
        },
        "get_current_context" => match latest_snapshot() {
            Some(state) => RpcOutcome {
                body: json!({"jsonrpc":"2.0","id":id,"result":snapshot_json(&state)}),
                is_error: false,
            },
            None => rpc_error(id, -32001, "no session snapshot available"),
        },
        other => rpc_error(id, -32601, &format!("method not found: {other}")),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 최신 세션 스냅샷 캐시 (get_current_context 및 state_snapshot 원천)
// ─────────────────────────────────────────────────────────────────────────────

static LATEST_STATE: Mutex<Option<GameSessionState>> = Mutex::new(None);

/// GUI drain 루프가 안정화된 상태 확정 시 호출 (논블로킹 try_lock — 원칙 ①)
pub fn update_latest_state(state: GameSessionState) {
    if let Ok(mut slot) = LATEST_STATE.try_lock() {
        *slot = Some(state);
    }
}

fn latest_snapshot() -> Option<GameSessionState> {
    LATEST_STATE.lock().ok().and_then(|s| s.clone())
}

fn snapshot_json(state: &GameSessionState) -> Value {
    let context = state.context.as_ref().map(|ctx| {
        json!({
            "song_id": ctx.song_id,
            "mode": ctx.mode.as_str(),
            "diff": ctx.diff.as_str(),
            "rate": ctx.rate,
            "is_max_combo": ctx.is_max_combo,
        })
    });
    json!({
        "scene": format!("{:?}", state.scene),
        "stable": state.is_stable,
        "fullscreen": state.is_fullscreen,
        "context": context,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// 유틸
// ─────────────────────────────────────────────────────────────────────────────

fn log(msg: impl AsRef<str>) {
    println!("[IPC] {}", msg.as_ref());
}
