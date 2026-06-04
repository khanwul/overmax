use crate::window_tracker::WindowRect;
use crate::screen_capture::CapturedFrame;

pub trait CaptureEngine: Send + Sync {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String>;
}
