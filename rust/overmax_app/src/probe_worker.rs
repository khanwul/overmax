//! Background probe: window + paths (keeps UI thread free for egui).

use overmax_data::DataCompatibility;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

pub fn spawn(root: PathBuf, log_tx: Sender<String>, game_found_tx: Sender<()>) {
    std::thread::spawn(move || {
        let mut was_found = false;
        loop {
            std::thread::sleep(Duration::from_secs(3));
            let compat = DataCompatibility::current();
            let record = root.join(compat.record_db);
            let _ = log_tx.send(format!("[Main] record.db exists={}", record.exists()));

            let tracker = crate::window_tracker::WindowTracker::new("DJMAX RESPECT V");
            let found = tracker.game_rect().is_some();
            if found && !was_found {
                let _ = game_found_tx.send(());
            }
            was_found = found;

            if let Some(r) = tracker.game_rect() {
                let fg = tracker.is_foreground();
                let _ = log_tx.send(format!(
                    "[WindowTracker] rect {}x{} @ ({},{}) foreground={fg}",
                    r.width, r.height, r.left, r.top
                ));
            } else {
                let _ = log_tx.send("[WindowTracker] game window not found".into());
            }
        }
    });
}
