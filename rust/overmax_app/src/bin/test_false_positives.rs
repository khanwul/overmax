use image::GenericImageView;
use serde_json::Value;
use std::fs;
use std::path::Path;

use overmax_data::store::image_index::ImageIndexDb;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(".");
    let defaults: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../settings.json"
    )))
    .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));

    let mut merged = overmax_data::config::settings::load_merged_settings(root, defaults);
    overmax_data::config::settings::normalize_settings(&mut merged);
    let settings: overmax_data::Settings = serde_json::from_value(merged).unwrap_or_default();
    let jm_settings = settings.jacket_matcher.unwrap_or_default();

    // 1. Load DB using ImageIndexDb
    let db_path = "cache/image_index.db";
    println!("[Tester] Loading DB from {}...", db_path);

    // threshold = 0.0 으로 강제 설정하여 1위 매칭 스코어를 항상 받아오게 함
    let mut image_db_raw = ImageIndexDb::new(db_path, 0.0)
        .with_disable_hog(jm_settings.disable_hog)
        .with_margin_threshold(jm_settings.margin_threshold as f32);

    if let Err(e) = image_db_raw.load() {
        eprintln!("[Error] Failed to load image index DB: {:?}", e);
        return Err(Box::new(e));
    }
    println!(
        "[Tester] Loaded {} image entries.",
        image_db_raw.song_count()
    );

    let raw_matcher = image_db_raw.matcher();

    // 2. Prepare Output Directory (Fuzzy 전용 폴더로 변경하거나 병합)
    let out_dir = Path::new("scratch/non_jacket_rois");
    // 기존 디렉토리가 없으면 생성 (이번엔 지우지 않고 유지하여 이전 테스트와 누적 관리)
    fs::create_dir_all(out_dir)?;
    println!(
        "[Tester] Output directory ensured at: {}",
        out_dir.display()
    );

    // 3. Scan files in freestyle_songselect
    let scan_dir = Path::new("scratch/freestyle_songselect");
    if !scan_dir.exists() {
        println!("[Error] Scan directory scratch/freestyle_songselect does not exist!");
        return Ok(());
    }

    let mut files = Vec::new();
    for entry in fs::read_dir(scan_dir)? {
        let path = entry?.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lower = ext.to_lowercase();
                if ext_lower == "jpg" || ext_lower == "jpeg" || ext_lower == "png" {
                    files.push(path);
                }
            }
        }
    }
    files.sort();

    println!(
        "[Tester] Running Fuzzy Random ROI Matching on {} screenshots...",
        files.len()
    );

    // LCG 의사난수 생성기 (외부 rand 라이브러리 추가 방지)
    let mut seed = 987654321u64;
    let mut next_rand = || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        seed
    };

    let mut fuzzy_tests = 0;
    let mut max_similarity: f32 = 0.0;
    let mut sum_similarity: f32 = 0.0;
    let mut false_positives = 0;
    let threshold = jm_settings.similarity_threshold as f32;

    for f in &files {
        let fname = f.file_name().unwrap().to_string_lossy().to_string();
        let stem = f.file_stem().unwrap().to_string_lossy().to_string();

        let img = match image::open(f) {
            Ok(i) => i,
            Err(e) => {
                eprintln!("[Tester] Failed to open {}: {}", fname, e);
                continue;
            }
        };

        // Resize image to FHD if not already
        let (w, h) = img.dimensions();
        let img_resized = if w != 1920 || h != 1080 {
            img.resize_exact(1920, 1080, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };

        // 이미지당 10개의 무작위 좌표 테스트
        for i in 0..10 {
            fuzzy_tests += 1;

            // 크롭 좌표 (60x60 크기이므로 최대 1860, 1020 범위로 조정)
            let rx = (next_rand() % 1860) as u32;
            let ry = (next_rand() % 1020) as u32;
            let rw = 60;
            let rh = 60;

            // Crop ROI
            let cropped = img_resized.crop_imm(rx, ry, rw, rh);
            let mut rgba = cropped.to_rgba8().into_raw();
            for chunk in rgba.chunks_exact_mut(4) {
                chunk.swap(0, 2); // RGBA to BGRA
            }

            // Run jacket matcher (similarity_threshold = 0.0 이므로 무조건 매칭 성공값 리턴)
            let match_res = raw_matcher.match_jacket(&rgba, rw as usize, rh as usize, 4);

            let (song_id, similarity) = match match_res {
                Some(ref m) => (m.image_id.clone(), m.similarity),
                None => ("None".to_string(), 0.0),
            };

            // 통계 누적
            if similarity > max_similarity {
                max_similarity = similarity;
            }
            sum_similarity += similarity;

            let is_fp = similarity >= threshold;
            if is_fp {
                false_positives += 1;
            }

            let status_label = if is_fp {
                "FUZZY_FALSE_POSITIVE"
            } else {
                "FUZZY_OK"
            };
            let sim_percentage = (similarity * 100.0).round() as usize;

            // Save cropped Fuzzy ROI
            let out_name = format!(
                "{}_{}_rand{}_x{}_y{}_match{}_sim{}pct.png",
                status_label, stem, i, rx, ry, song_id, sim_percentage
            );
            let out_path = out_dir.join(&out_name);
            cropped.save(&out_path)?;
        }
    }

    let avg_similarity = if fuzzy_tests > 0 {
        sum_similarity / fuzzy_tests as f32
    } else {
        0.0
    };

    println!("\n=== Fuzzy Random ROI Test Done ===");
    println!("Total Random ROIs Tested: {}", fuzzy_tests);
    println!(
        "Similarity Threshold limit: {:.4} ({}%)",
        threshold,
        (threshold * 100.0) as usize
    );
    println!(
        "Max Similarity Observed:  {:.4} ({}%)",
        max_similarity,
        (max_similarity * 100.0).round() as usize
    );
    println!(
        "Avg Similarity Observed:  {:.4} ({}%)",
        avg_similarity,
        (avg_similarity * 100.0).round() as usize
    );
    println!(
        "Fuzzy False Positives:    {} / {}",
        false_positives, fuzzy_tests
    );
    println!("All fuzzy crops and similarity results logged to scratch/non_jacket_rois/");

    Ok(())
}
