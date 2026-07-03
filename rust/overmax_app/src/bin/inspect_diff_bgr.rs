use std::path::Path;
use image::GenericImageView;

/// 결과 화면 전용 모드 숫자 템플릿 생성기
/// ROI: (78, 28, 50, 68) — x_start, y_start, width, height (절대 좌표, 1920x1080 기준)
fn main() {
    let samples: Vec<(&str, &str)> = vec![
        ("4", "scratch/screenshots/20260701164242_1.jpg"),
        ("5", "scratch/screenshots/1783012896.jpg"),
        ("6", "scratch/screenshots/20260701165356_1.jpg"),
        ("8", "scratch/screenshots/20260703020235_1.jpg"),
    ];

    let roi_x = 78;
    let roi_y = 28;
    let roi_w = 50;
    let roi_h = 68;
    let threshold: u8 = 120;

    println!("=== Result Screen Mode Digit Template Generator ===");
    println!("ROI: x={}, y={}, w={}, h={}", roi_x, roi_y, roi_w, roi_h);
    println!("Threshold: {}", threshold);
    println!();

    for (label, path) in &samples {
        let img_path = Path::new(path);
        if !img_path.exists() {
            println!("[{}] File not found: {}", label, path);
            continue;
        }

        let img = image::open(img_path).expect("failed to open image");
        let (w, h) = img.dimensions();
        let img_resized = if w != 1920 || h != 1080 {
            img.resize_exact(1920, 1080, image::imageops::FilterType::Lanczos3)
        } else {
            img
        };

        // ROI 크롭
        let cropped = img_resized.crop_imm(roi_x, roi_y, roi_w, roi_h);
        let crop_path = format!("scratch/screenshots/result_mode_digit_{}.png", label);
        cropped.save(&crop_path).ok();
        println!("[{}] Saved raw crop to: {}", label, crop_path);

        // 이진화
        let gray = cropped.to_luma8();
        let mut binary = image::GrayImage::new(roi_w, roi_h);
        for y in 0..roi_h {
            for x in 0..roi_w {
                let v = gray.get_pixel(x, y)[0];
                binary.put_pixel(x, y, image::Luma([if v >= threshold { 255 } else { 0 }]));
            }
        }
        let bin_path = format!("scratch/screenshots/result_mode_digit_{}_bin.png", label);
        binary.save(&bin_path).ok();
        println!("[{}] Saved binarized to: {}", label, bin_path);

        // 마스크 배열 출력 (Rust 코드 생성용)
        println!("[{}] Mask ({}x{}):", label, roi_w, roi_h);
        print!("const RESULT_MODE_MASK_{}: [u8; {}] = [\n", label, roi_w as usize * roi_h as usize);
        for y in 0..roi_h {
            print!("    ");
            for x in 0..roi_w {
                let v = if gray.get_pixel(x, y)[0] >= threshold { 1 } else { 0 };
                print!("{}, ", v);
            }
            println!();
        }
        println!("];");
        println!();
    }
}
