//! Compositor-coordinate game window geometry sources.
//!
//! Wayland gives clients no global window coordinates, so per-compositor IPC adapters
//! publish the game window rect here. Contract: docs/plans/2026-10-28-linux-window-geometry-sources.md

use super::WindowRect;
use std::sync::{Arc, Mutex};

/// Game window geometry in the compositor's global logical coordinate space.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GameGeometry {
    /// `wl_output` name the window is on; the overlay ignores geometry for other outputs.
    pub output: String,
    pub rect: WindowRect,
    pub generation: u64,
}

/// Written by a source thread, read by the overlay. `None` means "unknown": sources must
/// clear it on lost tracking, ambiguous matches, hidden workspaces or IPC disconnects.
pub type SharedGameGeometry = Arc<Mutex<Option<GameGeometry>>>;

pub trait GameGeometrySource: Send {
    fn name(&self) -> &'static str;

    /// Starts publishing geometry for the window titled `title` into `sink`.
    fn start(self: Box<Self>, title: &str, sink: SharedGameGeometry) -> Result<(), String>;
}

/// Default for compositors without a position IPC: never publishes, so the overlay keeps
/// its current manual placement.
pub struct NoGeometrySource;

impl GameGeometrySource for NoGeometrySource {
    fn name(&self) -> &'static str {
        "none"
    }

    fn start(self: Box<Self>, _title: &str, _sink: SharedGameGeometry) -> Result<(), String> {
        Ok(())
    }
}

/// Picks the geometry source for the running session. Compositor adapters (TASKS 12.2)
/// register here by environment detection.
pub fn select_geometry_source() -> Box<dyn GameGeometrySource> {
    Box::new(NoGeometrySource)
}

/// Starts the selected source and returns the shared slot it writes to.
pub fn spawn_game_geometry_source(
    title: &str,
) -> (SharedGameGeometry, Result<&'static str, String>) {
    let sink: SharedGameGeometry = Arc::new(Mutex::new(None));
    let source = select_geometry_source();
    let name = source.name();
    let started = source.start(title, sink.clone()).map(|()| name);
    (sink, started)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_source_never_publishes() {
        let (sink, started) = spawn_game_geometry_source("DJMAX RESPECT V");
        assert_eq!(started, Ok("none"));
        assert!(sink.lock().unwrap().is_none());
    }
}
