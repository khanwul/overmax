use crate::window_tracker::WindowRect;
use std::ptr::null_mut;
use windows_sys::Win32::Graphics::Gdi::{
    BitBlt, CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject, BITMAPINFO, BITMAPINFOHEADER, BI_RGB, CAPTUREBLT, DIB_RGB_COLORS, HBITMAP, HDC,
    RGBQUAD, SRCCOPY,
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CapturedFrame {
    pub width: i32,
    pub height: i32,
    pub bgra: Vec<u8>,
}

pub struct ScreenCapturer {
    screen_dc: Option<HDC>,
    memory_dc: Option<HDC>,
    hbitmap: Option<HBITMAP>,
    bits: *mut u8,
    width: i32,
    height: i32,
}

unsafe impl Send for ScreenCapturer {}
unsafe impl Sync for ScreenCapturer {}

impl ScreenCapturer {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            screen_dc: None,
            memory_dc: None,
            hbitmap: None,
            bits: null_mut(),
            width: 0,
            height: 0,
        })
    }

    pub fn capture_bgra(&mut self, rect: WindowRect) -> Result<CapturedFrame, String> {
        if !rect.is_valid() {
            return Err("capture rect must have positive dimensions".to_string());
        }

        // 해상도가 변경되었거나 리소스가 아예 초기화되지 않은 상태면 재생성
        if self.width != rect.width || self.height != rect.height || self.hbitmap.is_none() {
            self.release_resources();
            self.init_resources(rect.width, rect.height)?;
        }

        let screen_dc = self.screen_dc.ok_or("Screen DC not initialized")?;
        let memory_dc = self.memory_dc.ok_or("Memory DC not initialized")?;

        let ok = unsafe {
            BitBlt(
                memory_dc,
                0,
                0,
                rect.width,
                rect.height,
                screen_dc,
                rect.left,
                rect.top,
                SRCCOPY | CAPTUREBLT,
            )
        };

        if ok == 0 {
            return Err("BitBlt failed".to_string());
        }

        let len = (rect.width as usize) * (rect.height as usize) * 4;
        let bgra = unsafe { std::slice::from_raw_parts(self.bits, len).to_vec() };

        Ok(CapturedFrame {
            width: rect.width,
            height: rect.height,
            bgra,
        })
    }

    fn init_resources(&mut self, width: i32, height: i32) -> Result<(), String> {
        unsafe {
            let screen_dc = GetDC(null_mut());
            if screen_dc.is_null() {
                return Err("GetDC failed".to_string());
            }
            self.screen_dc = Some(screen_dc);

            let memory_dc = CreateCompatibleDC(screen_dc);
            if memory_dc.is_null() {
                ReleaseDC(null_mut(), screen_dc);
                self.screen_dc = None;
                return Err("CreateCompatibleDC failed".to_string());
            }
            self.memory_dc = Some(memory_dc);

            let mut bits = null_mut();
            let info = bitmap_info(width, height);
            let hbitmap = CreateDIBSection(memory_dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
            if hbitmap.is_null() || bits.is_null() {
                DeleteDC(memory_dc);
                ReleaseDC(null_mut(), screen_dc);
                self.screen_dc = None;
                self.memory_dc = None;
                return Err("CreateDIBSection failed".to_string());
            }
            self.hbitmap = Some(hbitmap);
            self.bits = bits.cast();
            self.width = width;
            self.height = height;

            let previous = SelectObject(memory_dc, hbitmap);
            if previous.is_null() {
                self.release_resources();
                return Err("SelectObject failed".to_string());
            }
        }
        Ok(())
    }

    fn release_resources(&mut self) {
        unsafe {
            if let Some(hbitmap) = self.hbitmap.take() {
                DeleteObject(hbitmap);
            }
            if let Some(memory_dc) = self.memory_dc.take() {
                DeleteDC(memory_dc);
            }
            if let Some(screen_dc) = self.screen_dc.take() {
                ReleaseDC(null_mut(), screen_dc);
            }
            self.bits = null_mut();
            self.width = 0;
            self.height = 0;
        }
    }
}

impl Drop for ScreenCapturer {
    fn drop(&mut self) {
        self.release_resources();
    }
}

fn bitmap_info(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..BITMAPINFOHEADER::default()
        },
        bmiColors: [RGBQUAD::default(); 1],
    }
}

#[cfg(test)]
mod tests {
    use super::ScreenCapturer;
    use crate::window_tracker::WindowRect;

    #[test]
    fn rejects_invalid_capture_rect() {
        let mut capturer = ScreenCapturer::new().unwrap();
        let result = capturer.capture_bgra(WindowRect {
            left: 0,
            top: 0,
            width: 0,
            height: 10,
        });

        assert!(result.is_err());
    }
}
