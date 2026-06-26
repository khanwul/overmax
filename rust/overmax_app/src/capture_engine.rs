use crate::window_tracker::{WindowRect, WindowTracker};
use crate::screen_capture::{CapturedFrame, GdiCaptureEngine};
use crate::dxgi_capture::DxgiCaptureEngine;

pub trait CaptureEngine: Send + Sync {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String>;
    fn capture_bgra_inplace(
        &mut self,
        rect: WindowRect,
        out_frame: &mut CapturedFrame,
    ) -> Result<(), String>;
}

pub struct AdaptiveCaptureEngine {
    tracker: WindowTracker,
    gdi_backend: Option<GdiCaptureEngine>,
    dxgi_backend: Option<DxgiCaptureEngine>,
    current_is_fullscreen: bool,
    last_dxgi_init_attempt: std::time::Instant,
}

impl AdaptiveCaptureEngine {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            tracker: WindowTracker::new("DJMAX RESPECT V"),
            gdi_backend: Some(GdiCaptureEngine::new()?),
            dxgi_backend: None,
            current_is_fullscreen: false,
            last_dxgi_init_attempt: std::time::Instant::now()
                .checked_sub(std::time::Duration::from_secs(5))
                .unwrap_or_else(std::time::Instant::now),
        })
    }
}

impl CaptureEngine for AdaptiveCaptureEngine {
    fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String> {
        // 런타임에 전체화면(Fullscreen) 모드 여부 실시간 확인
        let is_fs = self.tracker.is_fullscreen();
        self.current_is_fullscreen = is_fs;

        if is_fs {
            // 전체화면인 경우: DXGI 백엔드 사용
            if self.dxgi_backend.is_none() {
                if self.last_dxgi_init_attempt.elapsed() >= std::time::Duration::from_secs(3) {
                    self.last_dxgi_init_attempt = std::time::Instant::now();
                    match DxgiCaptureEngine::new() {
                        Ok(dxgi) => self.dxgi_backend = Some(dxgi),
                        Err(e) => {
                            // DXGI 디바이스 생성 실패 시 GDI로 즉시 임시 폴백
                            if let Some(ref mut gdi) = self.gdi_backend {
                                return gdi.capture_bgra(rect);
                            }
                            return Err(format!("DXGI init failed ({e}) and GDI fallback unavailable"));
                        }
                    }
                } else {
                    // 쿨다운 대기 중인 경우 생성을 시도하지 않고 즉시 GDI 폴백
                    if let Some(ref mut gdi) = self.gdi_backend {
                        return gdi.capture_bgra(rect);
                    }
                    return Err("DXGI retry cooldown active and GDI fallback unavailable".to_string());
                }
            }

            if let Some(ref mut dxgi) = self.dxgi_backend {
                match dxgi.capture_bgra(rect) {
                    Ok(frame) => Ok(frame),
                    Err(e) => {
                        // DXGI 캡처 에러 시 (Device Lost 등) 백엔드 파괴 및 GDI 임시 폴백
                        self.dxgi_backend = None;
                        if let Some(ref mut gdi) = self.gdi_backend {
                            gdi.capture_bgra(rect)
                        } else {
                            Err(format!("DXGI capture failed ({e}) and GDI fallback unavailable"))
                        }
                    }
                }
            } else {
                Err("DXGI backend initialized but missing".to_string())
            }
        } else {
            // 창 모드인 경우: GDI 백엔드 사용 (불필요한 DXGI 리소스는 즉시 해제)
            if self.dxgi_backend.is_some() {
                self.dxgi_backend = None;
            }

            if let Some(ref mut gdi) = self.gdi_backend {
                gdi.capture_bgra(rect)
            } else {
                Err("GdiCaptureEngine not initialized".to_string())
            }
        }
    }

    fn capture_bgra_inplace(
        &mut self,
        rect: WindowRect,
        out_frame: &mut CapturedFrame,
    ) -> Result<(), String> {
        let is_fs = self.tracker.is_fullscreen();
        self.current_is_fullscreen = is_fs;

        if is_fs {
            if self.dxgi_backend.is_none() {
                if self.last_dxgi_init_attempt.elapsed() >= std::time::Duration::from_secs(3) {
                    self.last_dxgi_init_attempt = std::time::Instant::now();
                    match DxgiCaptureEngine::new() {
                        Ok(dxgi) => self.dxgi_backend = Some(dxgi),
                        Err(e) => {
                            if let Some(ref mut gdi) = self.gdi_backend {
                                return gdi.capture_bgra_inplace(rect, out_frame);
                            }
                            return Err(format!("DXGI init failed ({e}) and GDI fallback unavailable"));
                        }
                    }
                } else {
                    if let Some(ref mut gdi) = self.gdi_backend {
                        return gdi.capture_bgra_inplace(rect, out_frame);
                    }
                    return Err("DXGI retry cooldown active and GDI fallback unavailable".to_string());
                }
            }

            if let Some(ref mut dxgi) = self.dxgi_backend {
                match dxgi.capture_bgra_inplace(rect, out_frame) {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        self.dxgi_backend = None;
                        if let Some(ref mut gdi) = self.gdi_backend {
                            gdi.capture_bgra_inplace(rect, out_frame)
                        } else {
                            Err(format!("DXGI capture failed ({e}) and GDI fallback unavailable"))
                        }
                    }
                }
            } else {
                Err("DXGI backend initialized but missing".to_string())
            }
        } else {
            if self.dxgi_backend.is_some() {
                self.dxgi_backend = None;
            }

            if let Some(ref mut gdi) = self.gdi_backend {
                gdi.capture_bgra_inplace(rect, out_frame)
            } else {
                Err("GdiCaptureEngine not initialized".to_string())
            }
        }
    }
}
