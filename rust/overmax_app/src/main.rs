mod debug_ui;
mod detection_pipeline;
mod frame_utils;
mod hysteresis;
mod native_app;
mod ocr_engine;
mod overlay_ui;
mod play_state;
mod probe_worker;
mod roi;
mod screen_capture;
mod settings_ui;
mod sync_ui;
mod varchive_upload;
mod window_tracker;

#[cfg(target_os = "windows")]
fn main() {
    if let Err(err) = native_app::run_native_app() {
        eprintln!("overmax-rs failed: {err}");
        std::process::exit(1);
    }
}

#[cfg(not(target_os = "windows"))]
fn main() {
    eprintln!("overmax-rs is Windows-only because Overmax depends on Win32 window tracking, capture, hotkey, and OCR APIs.");
    std::process::exit(1);
}
