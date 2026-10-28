//! Pure Loopback Inbound Transport Subsystem.
//!
//! Provides std-only HTTP parsing, localhost listener management, DNS rebinding guards,
//! and Server-Sent Events (SSE) streaming engines.

pub mod loopback;
pub mod sse;

pub use loopback::{
    now_ms, parse_request, read_body, respond_json, respond_sse_upgrade, run_loopback_manager,
    spawn_loopback_service, HttpRequest, LoopbackServerConfig, TransportError, TransportHandle,
};
pub use sse::{format_sse_frame, run_sse_hub_loop};
