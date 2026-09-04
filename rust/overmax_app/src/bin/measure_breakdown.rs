use std::time::Instant;

fn mean_and_p95(mut vals: Vec<u64>) -> (f64, u64, u64, u64) {
    if vals.is_empty() {
        return (0.0, 0, 0, 0);
    }
    vals.sort_unstable();
    let min = vals[0];
    let max = *vals.last().unwrap();
    let mean = vals.iter().sum::<u64>() as f64 / vals.len() as f64;
    let p95_idx = ((vals.len() as f64) * 0.95).floor() as usize;
    let p95 = vals[p95_idx.min(vals.len() - 1)];
    (mean, min, max, p95)
}

fn main() {
    println!("============================================================");
    println!("  OVERMAX CAPTURE & CV TIMING BREAKDOWN BENCHMARK");
    println!("============================================================\n");

    // ------------------------------------------------------------
    // 1. Direct3D11 Staging Texture Map & DMA Benchmark (1080p vs 512x512 Atlas)
    // ------------------------------------------------------------
    println!("[1] Measuring Direct3D11 GPU-to-CPU Staging Transfer (1080p vs 512x512 Atlas, 100 iterations)...");
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_FEATURE_LEVEL_11_0};
        use windows::Win32::Graphics::Direct3D11::{
            D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
            D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC,
            D3D11_USAGE_DEFAULT, D3D11_USAGE_STAGING,
        };
        use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_SAMPLE_DESC};

        unsafe {
            let mut device: Option<ID3D11Device> = None;
            let mut context: Option<ID3D11DeviceContext> = None;
            let mut feature_level = D3D_FEATURE_LEVEL_11_0;

            let res = D3D11CreateDevice(
                None,
                D3D_DRIVER_TYPE_HARDWARE,
                windows::Win32::Foundation::HMODULE::default(),
                D3D11_CREATE_DEVICE_FLAG(0),
                Some(&[D3D_FEATURE_LEVEL_11_0]),
                windows::Win32::Graphics::Direct3D11::D3D11_SDK_VERSION,
                Some(&mut device),
                Some(&mut feature_level),
                Some(&mut context),
            );

            if let (Ok(_), Some(device), Some(context)) = (&res, device, context) {
                // Helper to benchmark a texture size
                let bench_tex = |w: u32, h: u32, label: &str| {
                    let desc_gpu = D3D11_TEXTURE2D_DESC {
                        Width: w,
                        Height: h,
                        MipLevels: 1,
                        ArraySize: 1,
                        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                        SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                        Usage: D3D11_USAGE_DEFAULT,
                        BindFlags: 0,
                        CPUAccessFlags: 0,
                        MiscFlags: 0,
                    };
                    let desc_staging = D3D11_TEXTURE2D_DESC {
                        Usage: D3D11_USAGE_STAGING,
                        CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                        ..desc_gpu
                    };

                    let mut gpu_tex: Option<ID3D11Texture2D> = None;
                    let mut staging_tex: Option<ID3D11Texture2D> = None;
                    device.CreateTexture2D(&desc_gpu, None, Some(&mut gpu_tex)).unwrap();
                    device.CreateTexture2D(&desc_staging, None, Some(&mut staging_tex)).unwrap();
                    let gpu_tex = gpu_tex.unwrap();
                    let staging_tex = staging_tex.unwrap();

                    let iters = 100;
                    let mut copy_times = Vec::with_capacity(iters);
                    let mut map_times = Vec::with_capacity(iters);
                    let mut memcpy_times = Vec::with_capacity(iters);
                    let mut total_times = Vec::with_capacity(iters);

                    let mut cpu_buf: Vec<u8> = vec![0; (w * h * 4) as usize];

                    // Warmup 5
                    for _ in 0..5 {
                        context.CopyResource(&staging_tex, &gpu_tex);
                        let mut mapped = Default::default();
                        context.Map(&staging_tex, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).unwrap();
                        context.Unmap(&staging_tex, 0);
                    }

                    for _ in 0..iters {
                        let t_total = Instant::now();

                        let t_copy = Instant::now();
                        context.CopyResource(&staging_tex, &gpu_tex);
                        let copy_us = t_copy.elapsed().as_micros() as u64;

                        let t_map = Instant::now();
                        let mut mapped = Default::default();
                        context.Map(&staging_tex, 0, D3D11_MAP_READ, 0, Some(&mut mapped)).unwrap();
                        let map_us = t_map.elapsed().as_micros() as u64;

                        let t_memcpy = Instant::now();
                        let row_pitch = mapped.RowPitch as usize;
                        let src_ptr = mapped.pData as *const u8;
                        let dst_ptr = cpu_buf.as_mut_ptr();
                        let row_bytes = (w * 4) as usize;
                        for y in 0..(h as usize) {
                            std::ptr::copy_nonoverlapping(
                                src_ptr.add(y * row_pitch),
                                dst_ptr.add(y * row_bytes),
                                row_bytes,
                            );
                        }
                        let memcpy_us = t_memcpy.elapsed().as_micros() as u64;

                        context.Unmap(&staging_tex, 0);
                        let total_us = t_total.elapsed().as_micros() as u64;

                        copy_times.push(copy_us);
                        map_times.push(map_us);
                        memcpy_times.push(memcpy_us);
                        total_times.push(total_us);
                    }

                    let (c_mean, _, _, _) = mean_and_p95(copy_times);
                    let (m_mean, m_min, m_max, m_p95) = mean_and_p95(map_times);
                    let (mc_mean, mc_min, mc_max, mc_p95) = mean_and_p95(memcpy_times);
                    let (tot_mean, _, tot_max, tot_p95) = mean_and_p95(total_times);

                    println!("    --- {} ({} x {}, {:.2} MB) ---", label, w, h, (w * h * 4) as f64 / 1024.0 / 1024.0);
                    println!("    * GPU CopyResource       : {:>6.1} µs", c_mean);
                    println!("    * Map(D3D11_MAP_READ)    : {:>6.1} µs (min {:>4} µs, p95 {:>5} µs, max {:>5} µs)", m_mean, m_min, m_p95, m_max);
                    println!("    * CPU memcpy             : {:>6.1} µs (min {:>4} µs, p95 {:>5} µs, max {:>5} µs)", mc_mean, mc_min, mc_p95, mc_max);
                    println!("    * TOTAL Transfer Time    : {:>6.1} µs ({:.2} ms) (p95: {:.2} ms, max: {:.2} ms)", tot_mean, tot_mean / 1000.0, tot_p95 as f64 / 1000.0, tot_max as f64 / 1000.0);
                    tot_mean
                };

                let t_1080p = bench_tex(1920, 1080, "Current 1080p Full Frame");
                println!();
                let t_atlas = bench_tex(512, 512, "Proposed 512x512 ROI Atlas");
                println!();
                let t_360p = bench_tex(640, 360, "Experimental 360p Frame");

                println!("\n    >>> D3D11 TRANSFER SPEEDUP COMPARISON <<<");
                println!("    * 1080p (8.3 MB) ➔ {:.2} ms", t_1080p / 1000.0);
                println!("    * 512x512 Atlas (1.0 MB) ➔ {:.2} ms ({:.1}x Faster! {:.2} ms 절감)",
                    t_atlas / 1000.0, t_1080p / t_atlas, (t_1080p - t_atlas) / 1000.0);
                println!("    * 360p Frame (0.9 MB) ➔ {:.2} ms ({:.1}x Faster! {:.2} ms 절감)",
                    t_360p / 1000.0, t_1080p / t_360p, (t_1080p - t_360p) / 1000.0);
            } else {
                println!("    Failed to create D3D11 device: {:?}", res);
            }
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        println!("    DXGI benchmark is Windows-only.");
    }

    // ------------------------------------------------------------
    // 2. Jacket Recognition & Resize Breakdown (overmax_cv)
    // ------------------------------------------------------------
    println!("\n[2] Measuring Jacket Recognition & Resize Breakdown (1,000 iterations)...");
    {
        use overmax_cv::image::{resize_area_f32, resize_area_u8};

        // 60x60 BGR test jacket
        let jacket_w = 60;
        let jacket_h = 60;
        let gray_pixels: Vec<u8> = (0..jacket_w * jacket_h).map(|i| (i % 256) as u8).collect();

        let iters = 1000;

        // a) ahash resize (8x8)
        let t0 = Instant::now();
        for _ in 0..iters {
            let _ = resize_area_f32(&gray_pixels, jacket_w, jacket_h, 8, 8);
        }
        let ahash_resize_us = (t0.elapsed().as_micros() as f64) / iters as f64;

        // b) dhash resize (9x8)
        let t0 = Instant::now();
        for _ in 0..iters {
            let _ = resize_area_f32(&gray_pixels, jacket_w, jacket_h, 9, 8);
        }
        let dhash_resize_us = (t0.elapsed().as_micros() as f64) / iters as f64;

        // c) phash resize (32x32)
        let t0 = Instant::now();
        for _ in 0..iters {
            let _ = resize_area_f32(&gray_pixels, jacket_w, jacket_h, 32, 32);
        }
        let phash_resize_us = (t0.elapsed().as_micros() as f64) / iters as f64;

        // Full compute_image_hashes (includes resize + dct_2d_32 + bit packing)
        let bgr_pixels: Vec<u8> = vec![128; jacket_w * jacket_h * 3];
        let t0 = Instant::now();
        for _ in 0..iters {
            let _ = overmax_cv::compute_image_hashes(&bgr_pixels, jacket_w, jacket_h, 3);
        }
        let full_hashes_us = (t0.elapsed().as_micros() as f64) / iters as f64;

        // e) histogram 64x64 resize
        let t0 = Instant::now();
        for _ in 0..iters {
            let _ = resize_area_u8(&gray_pixels, jacket_w, jacket_h, 64, 64);
        }
        let hist_resize_us = (t0.elapsed().as_micros() as f64) / iters as f64;

        // f) Simulated 1,000 song DB L1 distance matching
        let target_hist = [100u16; 64];
        let db_hists: Vec<[u16; 64]> = (0..1000).map(|_| [102u16; 64]).collect();
        let t0 = Instant::now();
        for _ in 0..iters {
            let mut best_score = u32::MAX;
            for h in &db_hists {
                let diff: u32 = target_hist
                    .iter()
                    .zip(h.iter())
                    .map(|(a, b)| (*a as i32 - *b as i32).unsigned_abs())
                    .sum();
                if diff < best_score {
                    best_score = diff;
                }
            }
        }
        let db_l1_match_us = (t0.elapsed().as_micros() as f64) / iters as f64;

        let total_jacket_pipeline_us = full_hashes_us + hist_resize_us + db_l1_match_us;
        let total_resize_us = ahash_resize_us + dhash_resize_us + phash_resize_us + hist_resize_us;

        println!("    * Full compute_hashes (ahash+dhash+phash) : {:>6.1} µs", full_hashes_us);
        println!("      - ahash resize (8x8)                    : {:>6.1} µs", ahash_resize_us);
        println!("      - dhash resize (9x8)                    : {:>6.1} µs", dhash_resize_us);
        println!("      - phash resize (32x32)                  : {:>6.1} µs", phash_resize_us);
        println!("    * Histogram 64x64 resize                  : {:>6.1} µs", hist_resize_us);
        println!("    * 1,000 곡 DB L1 거리 매칭 루프           : {:>6.1} µs", db_l1_match_us);
        println!("    --------------------------------------------------");
        println!("    * Total Jacket Match Pipeline             : {:>6.1} µs ({:.2} ms)",
            total_jacket_pipeline_us, total_jacket_pipeline_us / 1000.0);
        println!("    * Total Resize Time in Jacket Match       : {:>6.1} µs ({:.1}%)",
            total_resize_us, (total_resize_us / total_jacket_pipeline_us) * 100.0);
    }

    // ------------------------------------------------------------
    // 3. PlayState / Template Matching Breakdown (resize_binary_nearest_into)
    // ------------------------------------------------------------
    println!("\n[3] Measuring PlayState & Template Matching Resize Breakdown (2,000 iterations)...");
    {
        use overmax_cv::image::resize_binary_nearest_into;

        // Result screen mode/diff template matching uses resize_binary_nearest_into
        let src_mask = vec![1u8; 16 * 32];
        let mut dst_mask = [0u8; 8 * 16];
        let iters = 2000;

        let t0 = Instant::now();
        for _ in 0..iters {
            resize_binary_nearest_into(&src_mask, 16, 32, &mut dst_mask, 8, 16);
        }
        let nearest_resize_us = (t0.elapsed().as_micros() as f64) / iters as f64;

        // Popcount bitmask matching (32x u32)
        let mask_a = [0xAAAA_AAAAu32; 32];
        let mask_b = [0x5555_5555u32; 32];
        let t0 = Instant::now();
        for _ in 0..iters {
            let mut diff = 0u32;
            for i in 0..32 {
                diff += (mask_a[i] ^ mask_b[i]).count_ones();
            }
            std::hint::black_box(diff);
        }
        let bitmask_match_us = (t0.elapsed().as_micros() as f64) / iters as f64;

        println!("    * resize_binary_nearest_into (글자당)      : {:>6.2} µs (nanoseconds: {:>4.0} ns)",
            nearest_resize_us, nearest_resize_us * 1000.0);
        println!("    * Bitmask XOR + Popcount (32 라인 대조)    : {:>6.2} µs (nanoseconds: {:>4.0} ns)",
            bitmask_match_us, bitmask_match_us * 1000.0);
        println!("    * 8글자 단어 매칭 시 총 리사이즈 소요시간  : {:>6.2} µs ({:.3} ms)",
            nearest_resize_us * 8.0, (nearest_resize_us * 8.0) / 1000.0);
    }

    println!("\n============================================================");
    println!("  BENCHMARK COMPLETE");
    println!("============================================================");
}
