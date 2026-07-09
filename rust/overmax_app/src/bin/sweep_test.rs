use std::time::Instant;
use overmax_engine::detector::templates::digit::DIGIT_TEMPLATES;
use overmax_cv::CvTemplate;

// BGRA format image region replica
struct SyntheticImage {
    width: usize,
    height: usize,
    bgra: Vec<u8>,
}

#[derive(Copy, Clone, Debug)]
enum LumaMethod {
    Weighted, // Standard ITU-R BT.601: 0.299R + 0.587G + 0.114B
    MaxRGB,   // max(R, G, B)
    Average,  // (R + G + B) / 3
}

fn binarize_by_luminance(
    img: &SyntheticImage,
    method: LumaMethod,
    threshold_calc: impl FnOnce(u8, u8) -> u8,
    foreground_value: u8,
) -> (Vec<u8>, u8, u8) {
    let cv_method = match method {
        LumaMethod::Weighted => overmax_cv::LumaMethod::Weighted,
        LumaMethod::MaxRGB => overmax_cv::LumaMethod::MaxRGB,
        LumaMethod::Average => overmax_cv::LumaMethod::Average,
    };
    overmax_cv::binarize_by_luminance(
        &img.bgra,
        img.width,
        img.height,
        cv_method,
        threshold_calc,
        foreground_value,
    )
}


fn evaluate_sweep(templates: &[CvTemplate], method: LumaMethod) -> (usize, usize, std::time::Duration) {
    let mut sweep_values = Vec::new();
    for i in 0..=32 {
        let val = if i == 32 { 255 } else { i * 8 };
        sweep_values.push(val);
    }

    let mut total_cases = 0;
    let mut success_count = 0;
    
    let start = Instant::now();
    for &bg_color in &sweep_values {
        for &fg_color in &sweep_values {
            if bg_color == fg_color {
                continue;
            }
            
            for t in templates {
                total_cases += 1;
                
                let w = t.width;
                let h = t.height;
                let mut bgra = vec![0u8; w * h * 4];
                for y in 0..h {
                    for x in 0..w {
                        let idx = (y * w + x) * 4;
                        let mask_val = t.mask[y * w + x];
                        let pixel_color = if mask_val == 1 { fg_color } else { bg_color };
                        bgra[idx] = pixel_color;     // B
                        bgra[idx + 1] = pixel_color; // G
                        bgra[idx + 2] = pixel_color; // R
                        bgra[idx + 3] = 255;         // A
                    }
                }
                
                let img = SyntheticImage { width: w, height: h, bgra };
                let (binary, _, _) = binarize_by_luminance(
                    &img,
                    method,
                    |max, _| {
                        if max > 80 {
                            ((max as f32 * 0.80) as u8).max(max.saturating_sub(45))
                        } else {
                            180
                        }
                    },
                    255,
                );
                
                if let Some((matched_char, _)) = overmax_cv::image::match_character(&binary, w, h, templates) {
                    if matched_char == t.char_val {
                        success_count += 1;
                    }
                }
            }
        }
    }
    (success_count, total_cases, start.elapsed())
}

fn main() {
    println!("=== OVERMAX CV LUMINANCE METHOD COMPARISON HARNESS ===");
    
    let original_templates: Vec<CvTemplate<'static>> = DIGIT_TEMPLATES
        .iter()
        .map(|t| CvTemplate {
            char_val: t.char_val,
            width: t.width,
            height: t.height,
            mask: t.mask,
        })
        .collect();

    // 1. Run Sweep Benchmarks
    let methods = [
        (LumaMethod::Weighted, "Luminance-based (Weighted / Current)"),
        (LumaMethod::MaxRGB, "Max-RGB (Proposed Option C)"),
        (LumaMethod::Average, "Average (Option C Alternative)"),
    ];

    for &(method, name) in &methods {
        let (success, total, elapsed) = evaluate_sweep(&original_templates, method);
        println!("--------------------------------------------------");
        println!("Method: {}", name);
        println!("  Success Rate: {:.2}% ({} / {})", (success as f64 / total as f64) * 100.0, success, total);
        println!("  Elapsed Time: {:?}", elapsed);
        println!("  Per-image:    {:.2} us", elapsed.as_micros() as f64 / total as f64);
    }
    
    // 2. Evaluate on collected real-world failure cases in scratch/auto_failures
    let failures_dir = std::path::Path::new("scratch/auto_failures");
    if failures_dir.exists() {
        println!("\n=== EVALUATING REAL-WORLD FAILURE IMAGES (scratch/auto_failures) ===");
        let mut rate_files = Vec::new();
        let mut score_files = Vec::new();
        
        if let Ok(entries) = std::fs::read_dir(failures_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("png") {
                    let fname = path.file_name().unwrap().to_string_lossy().to_string();
                    if fname.contains("_rate") {
                        rate_files.push(path);
                    } else if fname.contains("_score") {
                        score_files.push(path);
                    }
                }
            }
        }
        
        println!("Found {} rate and {} score failure images.", rate_files.len(), score_files.len());
        
        use image::GenericImageView;
        
        for &(method, name) in &methods {
            let mut rate_valid = 0;
            let mut score_valid = 0;
            
            for path in &rate_files {
                if let Ok(img_data) = image::open(path) {
                    let (w, h) = img_data.dimensions();
                    let mut bgra = img_data.to_rgba8().into_raw();
                    for chunk in bgra.chunks_exact_mut(4) {
                        chunk.swap(0, 2);
                    }
                    
                    let img = SyntheticImage { width: w as usize, height: h as usize, bgra };
                    let (binary, _, _) = binarize_by_luminance(
                        &img,
                        method,
                        |max, _| {
                            if max > 80 {
                                ((max as f32 * 0.80) as u8).max(max.saturating_sub(45))
                            } else {
                                180
                            }
                        },
                        255,
                    );
                    
                    let segs = overmax_cv::segment_characters(&binary, w as usize, h as usize).unwrap_or_default();
                    let mut matched = String::new();
                    for &(x1, x2) in &segs {
                        let char_w = x2 - x1;
                        let mut char_bin = vec![0u8; char_w * h as usize];
                        for y in 0..h as usize {
                            for x in 0..char_w {
                                char_bin[y * char_w + x] = binary[y * w as usize + (x1 + x)];
                            }
                        }
                        if let Some((ch, _)) = overmax_cv::image::match_character(&char_bin, char_w, h as usize, &original_templates) {
                            matched.push(ch);
                        } else {
                            matched.push('?');
                        }
                    }
                    
                    if !matched.is_empty() && !matched.contains('?') {
                        rate_valid += 1;
                    }
                }
            }
            
            for path in &score_files {
                if let Ok(img_data) = image::open(path) {
                    let (w, h) = img_data.dimensions();
                    let mut bgra = img_data.to_rgba8().into_raw();
                    for chunk in bgra.chunks_exact_mut(4) {
                        chunk.swap(0, 2);
                    }
                    
                    let img = SyntheticImage { width: w as usize, height: h as usize, bgra };
                    let (binary, _, _) = binarize_by_luminance(
                        &img,
                        method,
                        |max, _| {
                            if max > 80 {
                                ((max as f32 * 0.80) as u8).max(max.saturating_sub(45))
                            } else {
                                180
                            }
                        },
                        255,
                    );
                    
                    let segs = overmax_cv::segment_characters(&binary, w as usize, h as usize).unwrap_or_default();
                    let mut matched = String::new();
                    for &(x1, x2) in &segs {
                        let char_w = x2 - x1;
                        let mut char_bin = vec![0u8; char_w * h as usize];
                        for y in 0..h as usize {
                            for x in 0..char_w {
                                char_bin[y * char_w + x] = binary[y * w as usize + (x1 + x)];
                            }
                        }
                        if let Some((ch, _)) = overmax_cv::image::match_character(&char_bin, char_w, h as usize, &original_templates) {
                            matched.push(ch);
                        } else {
                            matched.push('?');
                        }
                    }
                    
                    if !matched.is_empty() && !matched.contains('?') {
                        score_valid += 1;
                    }
                }
            }
            
            println!("--------------------------------------------------");
            println!("Method: {}", name);
            println!("  Real-world Rate Valid Match:  {} / {} ({:.2}%)", 
                rate_valid, rate_files.len(), (rate_valid as f64 / rate_files.len() as f64) * 100.0);
            println!("  Real-world Score Valid Match: {} / {} ({:.2}%)", 
                score_valid, score_files.len(), (score_valid as f64 / score_files.len() as f64) * 100.0);
        }
        println!("--------------------------------------------------");
    }
}
