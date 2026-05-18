//! Background probe: window + paths (keeps UI thread free for egui).

use overmax_data::DataCompatibility;
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::time::Duration;

pub fn spawn(root: PathBuf, tx: Sender<String>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(Duration::from_secs(3));
        let compat = DataCompatibility::current();
        let record = root.join(compat.record_db);
        let _ = tx.send(format!("[Main] record.db exists={}", record.exists()));

        let tracker = crate::window_tracker::WindowTracker::new("DJMAX RESPECT V");
        if let Some(r) = tracker.game_rect() {
            let fg = tracker.is_foreground();
            let _ = tx.send(format!(
                "[WindowTracker] rect {}x{} @ ({},{}) foreground={fg}",
                r.width, r.height, r.left, r.top
            ));
        } else {
            let _ = tx.send("[WindowTracker] game window not found".into());
        }
    });
}
