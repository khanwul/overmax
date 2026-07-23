use crate::store::image_index::{ImageEntry, ImageMatch};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct JacketMatcherConfig {
    pub similarity_threshold: f32,
    /// HOG 매칭이 완전히 제거됨에 따라, `margin_threshold`와 `disable_hog`는
    /// 더 이상 런타임 매칭에 실질적 영향을 미치지 않지만, 사용자 설정 파일(`settings.user.json`)
    /// 호환성을 깨지 않고 무해하게 유지하기 위해 필드를 보존합니다.
    pub margin_threshold: f32,
    pub disable_hog: bool,
}

#[derive(Debug)]
struct MatchCache {
    recent_indices: Vec<usize>,
}

pub struct JacketMatcher {
    entries: Arc<Vec<ImageEntry>>,
    config: JacketMatcherConfig,
    cache: std::sync::Mutex<MatchCache>,
}

impl JacketMatcher {
    /// 즐겨찾기(Favorite) 및 테두리 마스킹이 적용된 총 비교 비트(160비트) 중,
    /// 노이즈가 가장 심한 특수 이미지들(예: Fundamental 등)에서 발생할 수 있는
    /// 최대 Hamming Distance 불일치 거리가 약 38~40비트 수준입니다.
    /// 정답이 잘못 걸러지는 누락(False Negative)을 방지하기 위해 통계 마진을 두어
    /// Early Exit 필터 임계치를 42비트로 정의합니다.
    /// 95% 이상의 완전 불일치 곡 후보군들은 POPCNT 3번으로 즉시 탈락(Early Exit)됩니다.
    const HAMMING_EARLY_EXIT_THRESHOLD: u32 = 42;

    pub fn new(entries: Vec<ImageEntry>, config: JacketMatcherConfig) -> Self {
        Self {
            entries: Arc::new(entries),
            config,
            cache: std::sync::Mutex::new(MatchCache {
                recent_indices: Vec::new(),
            }),
        }
    }

    pub fn similarity_threshold(&self) -> f32 {
        self.config.similarity_threshold
    }

    pub fn match_jacket(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        channels: usize,
    ) -> Option<ImageMatch> {
        self.match_jacket_with_top_k(data, width, height, channels, 10)
    }

    fn update_cache(&self, idx: usize) {
        if let Ok(mut guard) = self.cache.lock() {
            if let Some(pos) = guard.recent_indices.iter().position(|&x| x == idx) {
                guard.recent_indices.remove(pos);
            }
            guard.recent_indices.insert(0, idx);
            if guard.recent_indices.len() > 8 {
                guard.recent_indices.truncate(8);
            }
        }
    }

    /// 구버전 매칭 엔진의 public API 시그니처 호환성을 유지하기 위한 메서드입니다.
    /// HOG Cosine 유사도 매칭이 100% 배제되어 top_k 정렬 후 재대조할 필요가 없어져
    /// 내부적으로 `_top_k` 매개변수는 무시하고 1-Pass WTA 매칭을 수행합니다.
    pub fn match_jacket_with_top_k(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        channels: usize,
        _top_k: usize,
    ) -> Option<ImageMatch> {
        if self.entries.is_empty() {
            return None;
        }

        // 1. 3종 해시 추출
        let (q_phash, q_dhash, q_ahash) =
            overmax_cv::compute_image_hashes(data, width, height, channels).ok()?;

        // 2. 4x4 분할 RGB 그리드 히스토그램 추출 (BGRA 직접 입력, grayscale 변환 불필요)
        let q_grid_hist = overmax_cv::compute_grid_histogram(data, width, height, channels);

        // 오염 영역 비트 마스킹 (상단 y=0, 우측 x=7, 즐겨찾기 y=1, x=0)
        let mut mask_bits: u64 = 0;
        for x in 0..8 {
            mask_bits |= 1 << x; // y = 0
        }
        for y in 0..8 {
            mask_bits |= 1 << (y * 8 + 7); // x = 7
        }
        mask_bits |= 1 << 8; // y = 1, x = 0

        let hash_mask: u64 = !mask_bits;
        let compare_bits = hash_mask.count_ones() as f32; // 48.0
        let total_compare_bits = 64.0 + compare_bits * 2.0; // 160.0

        // 3. 싱글 스레드 순차 최적화 매칭 순회 (1차 Early Exit + 2차 WTA 유사도 계산)
        let matched = self
            .entries
            .iter()
            .enumerate()
            .filter_map(|(idx, entry)| {
                let p_dist = (entry.phash ^ q_phash).count_ones();
                let d_dist = ((entry.dhash ^ q_dhash) & hash_mask).count_ones();
                let a_dist = ((entry.ahash ^ q_ahash) & hash_mask).count_ones();

                let hamming_sum = p_dist + d_dist + a_dist;

                // 1차 필터: Early Exit (임계치 42비트)
                if hamming_sum > Self::HAMMING_EARLY_EXIT_THRESHOLD {
                    return None;
                }

                // 2차 필터: 히스토그램 L1 유사도 산출 (레거시 DB 하위 호환 보장)
                let hist_sim = if let Some(e_hist) = entry.grid_hist {
                    let mut hist_diff = 0u32;
                    for (&e_h, &q_h) in e_hist.iter().zip(q_grid_hist.iter()) {
                        hist_diff += (e_h as i32 - q_h as i32).unsigned_abs();
                    }
                    // 4x4 RGB 히스토그램 L1 정규화 상수 3072
                    // (256 × 384/32 = 3072, 2x2 grayscale 대비 동일한 bin당 민감도 유지)
                    1.0 - (hist_diff as f32 / 3072.0).clamp(0.0, 1.0)
                } else {
                    1.0 // 히스토그램이 없는 레거시 DB는 해시 유사도로만 판단
                };

                let hash_sim = 1.0 - (hamming_sum as f32 / total_compare_bits);

                // 가중합 유사도 산출 (50:50 비율로 이미지 해시와 분할 히스토그램 가중)
                let similarity = if entry.grid_hist.is_some() {
                    0.5 * hash_sim + 0.5 * hist_sim
                } else {
                    hash_sim
                };

                Some((idx, similarity))
            })
            .max_by(|a, b| a.1.total_cmp(&b.1));

        if let Some((idx, similarity)) = matched {
            if similarity >= self.config.similarity_threshold {
                self.update_cache(idx);
                return Some(ImageMatch {
                    image_id: self.entries[idx].image_id.clone(),
                    similarity,
                });
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_entry(image_id: &str, phash: u64, hog_val: f32) -> ImageEntry {
        let hog = vec![hog_val; 1764];
        let hog_norm = (1764.0 * hog_val * hog_val).sqrt().max(1.0);
        ImageEntry {
            image_id: image_id.to_string(),
            phash,
            dhash: phash,
            ahash: phash,
            hog,
            hog_norm,
            grid_hist: None,
        }
    }

    #[test]
    fn test_jacket_matcher_basic_match() {
        let entries = vec![
            dummy_entry("song-a", 0x0000_0000_0000_0000, 0.1),
            dummy_entry("song-b", 0xFFFF_FFFF_FFFF_FFFF, 0.2),
        ];
        let config = JacketMatcherConfig {
            similarity_threshold: 0.75,
            margin_threshold: 3.0,
            disable_hog: false,
        };
        let matcher = JacketMatcher::new(entries, config);

        // 8x8 그레이스케일 이미지 모킹 (전부 0)
        let query_data = vec![0u8; 64];

        let matched = matcher.match_jacket(&query_data, 8, 8, 1).unwrap();
        assert_eq!(matched.image_id, "song-a");
        assert!(matched.similarity >= 0.9);
    }
}
