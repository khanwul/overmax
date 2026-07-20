use clap::Parser;
use rayon::prelude::*;
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Parser, Debug)]
#[command(author, version, about = "V-Archive Jacket Image Feature DB Builder")]
struct Args {
    /// Newly downloaded jacket images directory
    #[arg(short, long)]
    image_dir: PathBuf,

    /// Target SQLite image_index.db file path
    #[arg(short, long, default_value = "image_index.db")]
    db_path: PathBuf,
}

struct ProcessTask {
    song_id: String,
    path: PathBuf,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();

    // 1. Open Database & Ensure Schema
    let mut conn = Connection::open(&args.db_path)?;
    ensure_schema(&mut conn)?;

    // 2. Scan Temporary Directory for Images
    let mut tasks = Vec::new();
    if args.image_dir.exists() {
        for entry in fs::read_dir(&args.image_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_lower = ext.to_lowercase();
                    if ext_lower == "jpg" || ext_lower == "jpeg" || ext_lower == "png" {
                        if let Some(song_id) = path.file_stem().and_then(|s| s.to_str()) {
                            tasks.push(ProcessTask {
                                song_id: song_id.to_string(),
                                path,
                            });
                        }
                    }
                }
            }
        }
    }

    if tasks.is_empty() {
        println!("[Builder] No images found to process.");
        return Ok(());
    }

    println!(
        "[Builder] Start processing {} images in parallel...",
        tasks.len()
    );

    // 3. Process Features in Parallel (phash, dhash, ahash)
    let results: Vec<(String, Result<ProcessResult, String>)> = tasks
        .into_par_iter()
        .map(|task| {
            let res = process_image(&task.path);
            (task.song_id, res)
        })
        .collect();

    // 4. Batch Upsert into Database (Single Transaction)
    let tx = conn.transaction()?;
    let mut success_count = 0;
    let total_tasks = results.len();

    for (song_id, feat_res) in results {
        match feat_res {
            Ok(res) => {
                let phash_str = format!("{:016x}", res.orig_phash);
                let dhash_str = format!("{:016x}", res.orig_dhash);
                let ahash_str = format!("{:016x}", res.orig_ahash);

                // 구버전 클라이언트의 코사인 유사도 연산을 만족하기 위한 물리적 HOG 데이터 직렬화
                let hog_bytes = f32_vec_to_bytes(&res.hog);

                // 히스토그램을 JSON 직렬화하여 metadata 컬럼에 저장 (serde는 [u8;384] 미지원 → Vec로 변환)
                let meta_json = serde_json::json!({
                    "histogram": res.grid_hist.to_vec()
                });
                let meta_str = serde_json::to_string(&meta_json).unwrap_or_default();

                tx.execute(
                    "INSERT INTO images (image_id, phash, dhash, ahash, hog, orb, metadata)
                     VALUES (?1, ?2, ?3, ?4, ?5, NULL, ?6)
                     ON CONFLICT(image_id) DO UPDATE SET
                         phash = excluded.phash,
                         dhash = excluded.dhash,
                         ahash = excluded.ahash,
                         hog   = excluded.hog,
                         orb   = NULL,
                         metadata = excluded.metadata",
                    params![song_id, phash_str, dhash_str, ahash_str, hog_bytes, meta_str],
                )?;
                success_count += 1;
            }
            Err(e) => {
                eprintln!("[Builder] Failed to process {}: {}", song_id, e);
            }
        }
    }
    tx.commit()?;

    println!(
        "[Builder] Completed. Successfully indexed {}/{} images.",
        success_count, total_tasks
    );
    Ok(())
}

struct ProcessResult {
    orig_phash: u64,
    orig_dhash: u64,
    orig_ahash: u64,
    hog: Vec<f32>,
    grid_hist: [u8; 384],
}

fn process_image(path: &Path) -> Result<ProcessResult, String> {
    // 1. Read Raw File Bytes
    let bytes = fs::read(path).map_err(|e| e.to_string())?;

    // 2. Decode using the image crate
    let img = image::load_from_memory(&bytes).map_err(|e| e.to_string())?;

    // 런타임 쿼리 재킷(60x60 크롭을 64x64로 리사이즈)과의 도메인 미스매치(해상도/블러 특성 차이)를
    // 원천 방어하기 위해, DB 빌드 시에도 동일하게 64x64 규격으로 Lanczos3 축소 정규화하여 추출합니다.
    let img_64 = img.resize_exact(64, 64, image::imageops::FilterType::Lanczos3);
    let rgba = img_64.to_rgba8();
    let mut bgra = rgba.into_raw();
    for chunk in bgra.chunks_exact_mut(4) {
        chunk.swap(0, 2); // Swap Red and Blue to get BGRA
    }

    // 3. Compute Hashes via overmax_cv (HOG 계산을 완전히 우회하여 리소스 방지)
    let (orig_phash, orig_dhash, orig_ahash) =
        overmax_cv::compute_image_hashes(&bgra, 64, 64, 4).map_err(|e| format!("{:?}", e))?;

    // 4. Compute Grid Histogram (4x4 RGB, 동일한 64x64 해상도의 정규화 공간에서 추출)
    let grid_hist = overmax_cv::compute_grid_histogram(&bgra, 64, 64, 4);

    // HOG 데이터는 100% 제거되었으므로 빈 벡터를 전달
    let hog = Vec::new();

    Ok(ProcessResult {
        orig_phash,
        orig_dhash,
        orig_ahash,
        hog,
        grid_hist,
    })
}

fn ensure_schema(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    conn.execute(
        "CREATE TABLE IF NOT EXISTS images (
            id       INTEGER PRIMARY KEY AUTOINCREMENT,
            image_id TEXT NOT NULL,
            phash    TEXT NOT NULL,
            dhash    TEXT NOT NULL,
            ahash    TEXT NOT NULL,
            hog      BLOB NOT NULL,
            orb      BLOB,
            metadata TEXT
        )",
        [],
    )?;
    conn.execute(
        "CREATE UNIQUE INDEX IF NOT EXISTS uq_images_image_id ON images (image_id)",
        [],
    )?;
    let _ = conn.execute("ALTER TABLE images ADD COLUMN metadata TEXT", []);
    Ok(())
}

fn f32_vec_to_bytes(vec: &[f32]) -> Vec<u8> {
    vec.iter().flat_map(|&val| val.to_le_bytes()).collect()
}
