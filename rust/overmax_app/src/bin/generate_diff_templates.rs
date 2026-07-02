use std::fs;
use std::path::{Path, PathBuf};
use image::GenericImageView;

// 고휘도 임계값 필터링 (휘도 Y >= threshold 이면 255, 아니면 0)
fn threshold_luminance(bgra: &[u8], width: usize, height: usize, sub_val: u8) -> Vec<u8> {
    let mut binary = vec![0u8; width * height];
    let mut max_y = 0u8;
    let mut y_vals = vec![0u8; width * height];
    
    for y in 0..height {
        for x in 0..width {
            let idx = (y * width + x) * 4;
            let b = bgra[idx];
            let g = bgra[idx + 1];
            let r = bgra[idx + 2];
            let y_val = ((77 * r as u32 + 150 * g as u32 + 29 * b as u32) >> 8) as u8;
            y_vals[y * width + x] = y_val;
            if y_val > max_y {
                max_y = y_val;
            }
        }
    }
    
    let threshold = 120;

    for idx in 0..(width * height) {
        binary[idx] = if y_vals[idx] >= threshold { 255 } else { 0 };
    }
    binary
}

struct DiffCandidate {
    label: &'static str,
    filename: &'static str,
    is_result: bool,
    sub_val: u8,
}

fn main() {
    let candidates = [
        DiffCandidate { label: "NM", filename: "select_diff_20260701123442_1.jpg", is_result: false, sub_val: 38 },
        DiffCandidate { label: "HD", filename: "result_freestyle_diff_20260702233605_1.jpg", is_result: true, sub_val: 38 },
        DiffCandidate { label: "MX", filename: "result_freestyle_diff_20260702233310_1.jpg", is_result: true, sub_val: 38 },
        DiffCandidate { label: "SC", filename: "result_freestyle_diff_20260702233900_1.jpg", is_result: true, sub_val: 38 },
    ];
    
    let input_dir = Path::new("scratch/screenshots/diff_rois");
    let target_height = 20usize; // 난이도 패널 텍스트의 세로 크기를 20px로 정규화
    
    let mut code = String::new();
    code.push_str("// Auto-generated diff templates. Do not edit.\n\n");
    code.push_str("pub struct DiffTemplate {\n");
    code.push_str("    pub name: &'static str,\n");
    code.push_str("    pub width: usize,\n");
    code.push_str("    pub height: usize,\n");
    code.push_str("    pub mask: &'static [u8],\n");
    code.push_str("}\n\n");
    code.push_str("pub static DIFF_TEMPLATES: &[DiffTemplate] = &[\n");

    for cand in &candidates {
        let path = input_dir.join(cand.filename);
        if !path.exists() {
            println!("Error: File missing: {}", cand.filename);
            return;
        }
        
        let img = image::open(&path).expect("failed to open image");
        let (w, h) = img.dimensions();
        
        // RGBA 변환 후 BGRA 채널 정돈
        let mut rgba = img.to_rgba8().into_raw();
        for chunk in rgba.chunks_exact_mut(4) {
            chunk.swap(0, 2);
        }
        
        // 이진화
        let mut binary = threshold_luminance(&rgba, w as usize, h as usize, cand.sub_val);
        
        // 선곡창의 경우 (글자가 어둡고 배경이 밝으므로) 비트 반전
        if !cand.is_result {
            for val in &mut binary {
                *val = if *val == 255 { 0 } else { 255 };
            }
        }
        
        // 수직 프로젝션으로 텍스트 시작과 끝(글자 경계) 크롭
        let mut x_start = 0usize;
        let mut x_end = w as usize;
        
        // 좌측 여백 제거
        for x in 0..w as usize {
            let mut col_has_pixels = false;
            for y in 0..h as usize {
                if binary[y * w as usize + x] == 255 {
                    col_has_pixels = true;
                    break;
                }
            }
            if col_has_pixels {
                x_start = x;
                break;
            }
        }
        // 우측 여백 제거
        for x in (0..w as usize).rev() {
            let mut col_has_pixels = false;
            for y in 0..h as usize {
                if binary[y * w as usize + x] == 255 {
                    col_has_pixels = true;
                    break;
                }
            }
            if col_has_pixels {
                x_end = x + 1;
                break;
            }
        }
        
        let cropped_w = x_end - x_start;
        let cropped_h = h as usize;
        let mut cropped_bin = vec![0u8; cropped_w * cropped_h];
        for y in 0..cropped_h {
            for x in 0..cropped_w {
                cropped_bin[y * cropped_w + x] = binary[y * w as usize + (x_start + x)];
            }
        }
        
        // 20px 세로 높이로 리사이징
        let target_width = ((cropped_w as f32 * target_height as f32 / cropped_h as f32).round()) as usize;
        
        // 임시 이미지 작성하여 Lanczos 리사이징 수행
        let mut luma_buf = vec![0u8; cropped_w * cropped_h];
        for i in 0..(cropped_w * cropped_h) {
            luma_buf[i] = cropped_bin[i];
        }
        let luma_img = image::GrayImage::from_raw(cropped_w as u32, cropped_h as u32, luma_buf).unwrap();
        let resized = image::DynamicImage::ImageLuma8(luma_img).resize_exact(target_width as u32, target_height as u32, image::imageops::FilterType::Lanczos3);
        let luma_resized = resized.to_luma8();
        
        let mut final_mask = Vec::new();
        for y in 0..target_height {
            for x in 0..target_width {
                let pixel = luma_resized.get_pixel(x as u32, y as u32)[0];
                final_mask.push(if pixel >= 128 { 1 } else { 0 });
            }
        }
        
        // 코드 문자열 조립
        code.push_str("    DiffTemplate {\n");
        code.push_str(&format!("        name: \"{}\",\n", cand.label));
        code.push_str(&format!("        width: {},\n", target_width));
        code.push_str(&format!("        height: {},\n", target_height));
        code.push_str("        mask: &[\n            ");
        for (idx, &val) in final_mask.iter().enumerate() {
            code.push_str(&format!("{},", val));
            if (idx + 1) % 20 == 0 {
                code.push_str("\n            ");
            }
        }
        code.push_str("\n        ],\n");
        code.push_str("    },\n");
        
        println!("Processed Diff '{}' -> {}x{}", cand.label, target_width, target_height);
    }
    
    code.push_str("];\n");
    
    fs::write("rust/overmax_app/src/diff_templates.rs", code).expect("failed to write diff_templates.rs");
    println!("Successfully generated rust/overmax_app/src/diff_templates.rs!");
}
