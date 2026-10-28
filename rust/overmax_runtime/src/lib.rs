//! Overmax runtime: UI-independent flow that ties detection output to data services.
//!
//! Hosts the runtime hub and its observers (IPC, auto upload) so frontends other than
//! `overmax_app` can reuse them. Must not depend on egui/eframe.
//! Layering: `app → runtime → engine/data → core` (docs/plans/2026-10-28-app-boundary-inventory.md)

pub mod auto_upload;
pub mod derived;
pub mod hub;
pub mod ipc_observer;
pub mod ipc_server;
pub mod transport;
pub mod upload_plan;
pub mod varchive_jobs;
