use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

struct SavePayload {
    root: PathBuf,
    diff: serde_json::Value,
}

/// A dedicated background worker that debounces rapid settings changes
/// and writes them atomically to disk.
pub struct SettingsDebounceWriter {
    tx: Sender<SavePayload>,
    _worker: Option<JoinHandle<()>>,
}

impl SettingsDebounceWriter {
    pub const DEBOUNCE_DURATION: Duration = Duration::from_millis(100);

    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<SavePayload>();
        let handle = thread::Builder::new()
            .name("overmax-settings-writer".to_string())
            .spawn(move || {
                Self::run_worker(rx);
            })
            .ok();

        Self {
            tx,
            _worker: handle,
        }
    }

    /// Queues a debounced save of the settings delta.
    pub fn queue_save(&self, root: impl AsRef<Path>, diff: serde_json::Value) {
        let _ = self.tx.send(SavePayload {
            root: root.as_ref().to_path_buf(),
            diff,
        });
    }

    fn flush_payload(payload: &SavePayload) {
        if payload.root.extension().is_some_and(|ext| ext == "json") {
            let _ = overmax_data::save_user_settings_to_path(&payload.root, &payload.diff);
        } else {
            let _ = overmax_data::save_user_settings(&payload.root, &payload.diff);
        }
    }

    fn run_worker(rx: Receiver<SavePayload>) {
        let mut pending: Option<SavePayload> = None;
        let mut last_activity = Instant::now();

        loop {
            if let Some(payload) = pending.take() {
                let elapsed = last_activity.elapsed();
                if elapsed < Self::DEBOUNCE_DURATION {
                    let wait = Self::DEBOUNCE_DURATION - elapsed;
                    match rx.recv_timeout(wait) {
                        Ok(new_payload) => {
                            // Coalesce / update with the newest payload
                            pending = Some(new_payload);
                            last_activity = Instant::now();
                            continue;
                        }
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            // Debounce duration elapsed without new incoming payloads, write now
                            Self::flush_payload(&payload);
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => {
                            // Flush final payload on shutdown
                            Self::flush_payload(&payload);
                            break;
                        }
                    }
                } else {
                    Self::flush_payload(&payload);
                }
            } else {
                match rx.recv() {
                    Ok(new_payload) => {
                        pending = Some(new_payload);
                        last_activity = Instant::now();
                    }
                    Err(_) => {
                        // Channel disconnected
                        break;
                    }
                }
            }
        }
    }
}

impl Default for SettingsDebounceWriter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::fs;

    #[test]
    fn test_debounce_writer_coalesces_writes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let root = temp_dir.path().to_path_buf();
        let writer = SettingsDebounceWriter::new();

        for i in 0..10 {
            writer.queue_save(&root, json!({ "slider": i }));
            thread::sleep(Duration::from_millis(5));
        }

        // Wait for debounce timeout to flush
        thread::sleep(Duration::from_millis(200));

        let user_path = root.join("settings.user.json");
        assert!(user_path.exists());
        let text = fs::read_to_string(user_path).unwrap();
        let val: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(val["slider"], 9);
    }
}
