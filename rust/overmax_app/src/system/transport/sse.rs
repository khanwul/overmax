//! Pure SSE (Server-Sent Events) Stream Hub and Framing Engine.
//!
//! Manages SSE client connections, keep-alive heartbeats, frame serialization,
//! and non-blocking fan-out broadcast with automatic cleanup of dead connections.

use std::io::Write;
use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Maximum simultaneous SSE clients.
pub const MAX_CLIENTS: usize = 16;
/// Capacity of client per-connection event queue.
pub const CLIENT_QUEUE_CAPACITY: usize = 32;
/// SSE heartbeat interval in seconds.
pub const HEARTBEAT_SECS: u64 = 15;

/// Formats a single Server-Sent Events frame.
pub fn format_sse_frame(event_name: &str, data: &str) -> String {
    format!("event: {event_name}\ndata: {data}\n\n")
}

/// Runs the SSE broadcast hub loop with custom frame formatting.
pub fn run_sse_hub_loop<T, F, S>(
    msg_rx: Receiver<T>,
    new_client_rx: Receiver<TcpStream>,
    shutdown: Arc<AtomicBool>,
    format_frame: F,
    mut initial_state_provider: S,
) where
    T: Clone + Send + 'static,
    F: Fn(&T, u64) -> String + Send + Sync + 'static,
    S: FnMut() -> Option<T>,
{
    let format_frame = Arc::new(format_frame);
    let mut clients: Vec<SyncSender<T>> = Vec::new();
    let tick = Duration::from_millis(50);

    loop {
        if shutdown.load(Ordering::Relaxed) {
            break;
        }

        // Accept new SSE streams
        while let Ok(stream) = new_client_rx.try_recv() {
            if clients.len() >= MAX_CLIENTS {
                let _ = stream.shutdown(std::net::Shutdown::Both);
                continue;
            }

            let (tx, rx) = sync_channel::<T>(CLIENT_QUEUE_CAPACITY);
            if let Some(initial_msg) = initial_state_provider() {
                let _ = tx.try_send(initial_msg);
            }

            clients.push(tx);

            let formatter = format_frame.clone();
            std::thread::Builder::new()
                .name("sse-writer".into())
                .spawn(move || client_writer_loop(rx, stream, formatter))
                .ok();
        }

        // Receive broadcast messages
        match msg_rx.recv_timeout(tick) {
            Ok(msg) => {
                clients.retain(|client_tx| client_tx.try_send(msg.clone()).is_ok());
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }
}

fn client_writer_loop<T, F>(
    rx: Receiver<T>,
    mut stream: TcpStream,
    formatter: Arc<F>,
) where
    F: Fn(&T, u64) -> String,
{
    let mut seq: u64 = 0;
    let mut last_heartbeat = Instant::now();

    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(msg) => {
                let frame = formatter(&msg, seq);
                if stream.write_all(frame.as_bytes()).is_err() || stream.flush().is_err() {
                    return;
                }
                seq += 1;
                last_heartbeat = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {
                if last_heartbeat.elapsed() >= Duration::from_secs(HEARTBEAT_SECS) {
                    if stream.write_all(b": ping\n\n").is_err() || stream.flush().is_err() {
                        return;
                    }
                    last_heartbeat = Instant::now();
                }
            }
            Err(RecvTimeoutError::Disconnected) => {
                let _ = stream.shutdown(std::net::Shutdown::Both);
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_framing_formats_spec_compliant_frame() {
        let frame = format_sse_frame("SceneDetected", r#"{"scene":"SongSelect"}"#);
        assert_eq!(
            frame,
            "event: SceneDetected\ndata: {\"scene\":\"SongSelect\"}\n\n"
        );
    }
}
