use std::fs;
use std::path::{Path, PathBuf};
use image::GenericImageView;

fn main() {
    let dir = Path::new("scratch/screenshots");
    if !dir.exists() {
        println!("Folder scratch/screenshots missing!");
        return;
    }
    
    let mut paths = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            let path = entry.path();
            let fname = path.file_name().unwrap().to_string_lossy().to_string();
            if fname.starts_with("result_open3_mcbadge_") || fname.starts_with("result_open2_mcbadge_") {
                paths.push(path);
            }
        }
    }
    paths.sort();
    
    fs::create_dir_all("scratch/screenshots/diff_bin").ok();
    
    println!("=== Exporting Binarized Debug Images ===");
    for path in paths {
        let filename = path.file_name().unwrap().to_string_lossy().to_string();
        let img = match image::open(&path) {
            Ok(i) => i,
            Err(_) => continue,
        };
        let w = img.width();
        let h = img.height();
        
        let mut rgba = img.to_rgba8().into_raw();
        for chunk in rgba.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
        
        // 오픈매치 난이도 크롭
        let crop_w = (w as f32 * 0.14) as usize;
        let crop_h = h as usize;
        let x_offset = (w as f32 * 0.81) as usize;
        let mut cropped_bgra = vec![0u8; crop_w * crop_h * 4];
        
        let y_start = 4usize;
        let y_end = crop_h.saturating_sub(4);
        
        for y in y_start..y_end {
            for x in 0..crop_w {
                let src_idx = (y * w as usize + (x_offset + x)) * 4;
                let dst_idx = (y * crop_w + x) * 4;
                cropped_bgra[dst_idx..dst_idx+4].copy_from_slice(&rgba[src_idx..src_idx+4]);
            }
        }
        
        // 이진화
        let mut max_y = 0u8;
        let mut y_vals = vec![0u8; crop_w * crop_h];
        for y in 0..crop_h {
            for x in 0..crop_w {
                let idx = (y * crop_w + x) * 4;
                let b = cropped_bgra[idx];
                let g = cropped_bgra[idx + 1];
                let r = cropped_bgra[idx + 2];
                let y_val = ((77 * r as u32 + 150 * g as u32 + 29 * b as u32) >> 8) as u8;
                y_vals[y * crop_w + x] = y_val;
                if y_val > max_y {
                    max_y = y_val;
                }
            }
        }
        
        let threshold = if max_y > 80 {
            ((max_y as f32 * 0.80) as u8).max(max_y.saturating_sub(45))
        } else {
            180
        };
        
        let mut binary = vec![0u8; crop_w * crop_h];
        let mut active_count = 0usize;
        for idx in 0..(crop_w * crop_h) {
            let is_active = y_vals[idx] >= threshold;
            binary[idx] = if is_active { 255 } else { 0 };
            if is_active {
                active_count += 1;
            }
        }
        
        let mut inverted = false;
        if active_count > (crop_w * crop_h * 55) / 100 { // 60% -> 55% 로 마진 소폭 조정
            inverted = true;
            for val in &mut binary {
                *val = if *val == 255 { 0 } else { 255 };
            }
        }
        
        // 이미지 저장
        let mut luma_buf = vec![0u8; crop_w * crop_h];
        for i in 0..(crop_w * crop_h) {
            luma_buf[i] = binary[i];
        }
        let luma_img = image::GrayImage::from_raw(crop_w as u32, crop_h as u32, luma_buf).unwrap();
        let save_path = format!("scratch/screenshots/diff_bin/bin_{}", filename);
        luma_img.save(&save_path).ok();
        println!("Saved debug bin to: {} (inverted={})", save_path, inverted);
    }
}
