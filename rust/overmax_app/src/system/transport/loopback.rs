//! Pure std-only Loopback HTTP Server Transport Engine.
//!
//! Handles TCP listener lifecycle (binding, port scanning, fallback),
//! background accept loop, endpoint file sync, HTTP parsing, DNS rebinding guards,
//! and routing for SSE upgrade (`/events`), discovery manifest (`/`), and RPC (`/rpc`).
//! This module contains ZERO Overmax domain logic.

use serde_json::{json, Value};
use std::fmt;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::ops::RangeInclusive;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Sender, SyncSender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const MAX_RPC_BODY: usize = 64 * 1024;
const POLL_MS: u64 = 250;
const HUB_CHANNEL_CAPACITY: usize = 64;

#[derive(Debug)]
pub enum TransportError {
    InvalidPeer,
    InvalidHost,
    Io(std::io::Error),
    BodyTooLarge,
}

impl fmt::Display for TransportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPeer => write!(f, "요청자가 루프백(localhost)이 아닙니다"),
            Self::InvalidHost => write!(f, "Host 헤더 검증 실패 (DNS Rebinding 가드)"),
            Self::Io(e) => write!(f, "I/O 오류: {e}"),
            Self::BodyTooLarge => write!(f, "요청 본문이 허용 한도를 초과했습니다"),
        }
    }
}

impl std::error::Error for TransportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<std::io::Error> for TransportError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Loopback server configuration parameters.
#[derive(Clone, Debug)]
pub struct LoopbackServerConfig {
    pub root: PathBuf,
    pub protocol_id: &'static str,
    pub manifest_name: &'static str,
    pub port_band: RangeInclusive<u16>,
}

/// Inbound HTTP request parsing result.
pub struct HttpRequest {
    pub method: String,
    pub path: String,
    pub content_type: String,
    pub content_length: usize,
    pub reader: BufReader<TcpStream>,
}

/// Handle to signal graceful shutdown of the transport background threads.
#[derive(Clone)]
pub struct TransportHandle {
    shutdown: Arc<AtomicBool>,
}

impl TransportHandle {
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);
    }
}

/// Returns the current Unix timestamp in milliseconds.
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}

/// Spawns the complete Loopback Transport service (SSE Hub + Loopback HTTP Manager).
///
/// Returns an event sender channel, transport handle for shutdown, and the currently bound port slot.
pub fn spawn_loopback_service<T, F, S, C, R>(
    config: LoopbackServerConfig,
    settings_reader: C,
    format_sse: F,
    initial_state_provider: S,
    rpc_handler: R,
) -> (SyncSender<T>, TransportHandle, Arc<Mutex<Option<u16>>>)
where
    T: Clone + Send + 'static,
    F: Fn(&T, u64) -> String + Send + Sync + 'static,
    S: Fn() -> Option<T> + Send + Sync + 'static,
    C: Fn() -> (bool, u16) + Send + 'static,
    R: Fn(&str) -> (u16, &'static str, Value) + Send + Sync + Clone + 'static,
{
    let shutdown = Arc::new(AtomicBool::new(false));
    let bound_slot: Arc<Mutex<Option<u16>>> = Arc::new(Mutex::new(None));

    let (event_tx, event_rx) = sync_channel::<T>(HUB_CHANNEL_CAPACITY);
    let (new_sse_client_tx, new_sse_client_rx) = std::sync::mpsc::channel::<TcpStream>();

    // 1. SSE Hub Thread
    {
        let shutdown = shutdown.clone();
        std::thread::Builder::new()
            .name("ipc-hub".into())
            .spawn(move || {
                crate::system::transport::sse::run_sse_hub_loop(
                    event_rx,
                    new_sse_client_rx,
                    shutdown,
                    format_sse,
                    initial_state_provider,
                );
            })
            .expect("ipc-hub thread");
    }

    // 2. HTTP Manager & Accept Thread
    {
        let shutdown = shutdown.clone();
        let bound_slot_clone = bound_slot.clone();
        std::thread::Builder::new()
            .name("ipc-manager".into())
            .spawn(move || {
                run_loopback_manager(
                    config,
                    settings_reader,
                    new_sse_client_tx,
                    bound_slot_clone,
                    shutdown,
                    rpc_handler,
                );
            })
            .expect("ipc-manager thread");
    }

    (event_tx, TransportHandle { shutdown }, bound_slot)
}

/// Runs the background loopback HTTP manager loop.
pub fn run_loopback_manager<F, R>(
    config: LoopbackServerConfig,
    settings_reader: F,
    new_sse_client_tx: Sender<TcpStream>,
    bound_slot: Arc<Mutex<Option<u16>>>,
    shutdown: Arc<AtomicBool>,
    rpc_handler: R,
) where
    F: Fn() -> (bool, u16) + Send + 'static,
    R: Fn(&str) -> (u16, &'static str, Value) + Send + Sync + Clone + 'static,
{
    let mut running_listener: Option<(TcpListener, u16)> = None;
    let tick = Duration::from_millis(POLL_MS);

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        let (enabled, desired_port) = settings_reader();
        let current_port = running_listener.as_ref().map(|(_, p)| *p);

        if !enabled {
            if running_listener.take().is_some() {
                set_bound_port(&bound_slot, None);
                write_endpoint_file(&config.root, config.protocol_id, None);
            }
            std::thread::sleep(tick);
            continue;
        }

        // Port change detection -> Rebind
        if running_listener.is_some() && current_port != Some(desired_port) {
            running_listener.take();
            set_bound_port(&bound_slot, None);
        }

        // Bind attempt with fallback band
        if running_listener.is_none() {
            match bind_with_fallback(desired_port, config.port_band.clone()) {
                Some((listener, port)) => {
                    set_bound_port(&bound_slot, Some(port));
                    write_endpoint_file(&config.root, config.protocol_id, Some(port));
                    running_listener = Some((listener, port));
                }
                None => {
                    std::thread::sleep(tick);
                    continue;
                }
            }
        }

        // Accept loop (non-blocking)
        if let Some((listener, _)) = running_listener.as_ref() {
            match listener.accept() {
                Ok((stream, addr)) => {
                    if addr.ip().is_loopback() {
                        let new_sse_client_tx = new_sse_client_tx.clone();
                        let rpc_handler = rpc_handler.clone();
                        let protocol_id = config.protocol_id;
                        let manifest_name = config.manifest_name;

                        std::thread::Builder::new()
                            .name("http-handshake".into())
                            .spawn(move || {
                                handle_connection(
                                    stream,
                                    &new_sse_client_tx,
                                    rpc_handler,
                                    protocol_id,
                                    manifest_name,
                                );
                            })
                            .ok();
                    }
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => {}
            }
        }

        std::thread::sleep(tick);
    }
}

fn set_bound_port(slot: &Arc<Mutex<Option<u16>>>, port: Option<u16>) {
    if let Ok(mut guard) = slot.lock() {
        *guard = port;
    }
}

fn write_endpoint_file(root: &Path, protocol_id: &str, port: Option<u16>) {
    let dir = root.join("cache");
    if !dir.exists() {
        let _ = std::fs::create_dir_all(&dir);
    }
    let body = match port {
        Some(p) => json!({ "protocol": protocol_id, "host": "127.0.0.1", "port": p }),
        None => json!({ "protocol": protocol_id, "host": null, "port": null }),
    };
    let tmp = dir.join("ipc_endpoint.json.tmp");
    let target = dir.join("ipc_endpoint.json");
    if std::fs::write(&tmp, body.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &target);
    }
}

fn bind_with_fallback(preferred: u16, band: RangeInclusive<u16>) -> Option<(TcpListener, u16)> {
    let candidates = std::iter::once(preferred).chain(band.filter(move |p| *p != preferred));
    for candidate in candidates {
        if let Ok(l) = TcpListener::bind(("127.0.0.1", candidate)) {
            let _ = l.set_nonblocking(true);
            return Some((l, candidate));
        }
    }
    None
}

fn handle_connection<R>(
    mut stream: TcpStream,
    new_sse_client_tx: &Sender<TcpStream>,
    rpc_handler: R,
    protocol_id: &str,
    manifest_name: &str,
) where
    R: Fn(&str) -> (u16, &'static str, Value),
{
    let Ok(stream_clone) = stream.try_clone() else {
        return;
    };
    let Ok(mut req) = parse_request(stream_clone) else {
        return;
    };

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/events") => {
            if respond_sse_upgrade(&mut stream).is_ok() {
                let _ = new_sse_client_tx.send(stream);
            }
        }
        ("GET", "/") => {
            let manifest = json!({
                "protocol": protocol_id,
                "name": manifest_name,
                "events": "/events",
                "rpc": "/rpc",
            });
            respond_json(&mut stream, 200, "OK", &manifest, true);
        }
        ("POST", "/rpc") => {
            if !req.content_type.starts_with("application/json") {
                respond_json(
                    &mut stream,
                    415,
                    "Unsupported Media Type",
                    &json!({"error": "content-type must be application/json"}),
                    false,
                );
                return;
            }
            if req.content_length == 0 || req.content_length > MAX_RPC_BODY {
                respond_json(
                    &mut stream,
                    400,
                    "Bad Request",
                    &json!({"error": "invalid body length"}),
                    false,
                );
                return;
            }

            let Ok(body) = read_body(&mut req.reader, req.content_length, MAX_RPC_BODY) else {
                return;
            };

            let (status, reason, response_json) = rpc_handler(&String::from_utf8_lossy(&body));
            respond_json(&mut stream, status, reason, &response_json, false);
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

pub fn parse_request(mut stream: TcpStream) -> Result<HttpRequest, TransportError> {
    stream.set_nodelay(true).ok();
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok();

    let peer = stream.peer_addr()?;
    if !peer.ip().is_loopback() {
        return Err(TransportError::InvalidPeer);
    }

    let reader_stream = stream.try_clone()?;
    let mut reader = BufReader::new(reader_stream);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Err(TransportError::Io(std::io::Error::new(
            std::io::ErrorKind::UnexpectedEof,
            "empty request line",
        )));
    }

    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or_default().to_ascii_uppercase();
    let raw_path = parts.next().unwrap_or("/");
    let path = raw_path.split('?').next().unwrap_or("/").to_string();

    let mut host_ok = false;
    let mut content_type = String::new();
    let mut content_length = 0usize;

    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
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
            &serde_json::json!({"error": "invalid host"}),
            false,
        );
        return Err(TransportError::InvalidHost);
    }

    Ok(HttpRequest {
        method,
        path,
        content_type,
        content_length,
        reader,
    })
}

pub fn read_body(
    reader: &mut BufReader<TcpStream>,
    content_length: usize,
    max_len: usize,
) -> Result<Vec<u8>, TransportError> {
    if content_length > max_len {
        return Err(TransportError::BodyTooLarge);
    }
    let mut body = vec![0u8; content_length];
    reader.read_exact(&mut body)?;
    Ok(body)
}

pub fn respond_json(
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

pub fn respond_sse_upgrade(stream: &mut TcpStream) -> Result<(), std::io::Error> {
    let headers = "HTTP/1.1 200 OK\r\n\
        Content-Type: text/event-stream\r\n\
        Cache-Control: no-cache\r\n\
        Connection: keep-alive\r\n\r\n";
    stream.write_all(headers.as_bytes())?;
    stream.flush()?;
    Ok(())
}
