use crate::image_index::{ImageEntry, ImageMatch};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct JacketMatcherConfig {
    pub similarity_threshold: f32,
    pub margin_threshold: f32,
    pub disable_hog: bool,
}

pub struct JacketMatcher {
    entries: Arc<Vec<ImageEntry>>,
    config: JacketMatcherConfig,
}

impl JacketMatcher {
    pub fn new(entries: Vec<ImageEntry>, config: JacketMatcherConfig) -> Self {
        Self {
            entries: Arc::new(entries),
            config,
        }
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

    pub fn match_jacket_with_top_k(
        &self,
        data: &[u8],
        width: usize,
        height: usize,
        channels: usize,
        top_k: usize,
    ) -> Option<ImageMatch> {
        if self.entries.is_empty() || top_k == 0 {
            return None;
        }

        // 1단계: 해시 특징량 계산
        let (q_phash, q_dhash, q_ahash) =
            overmax_cv::compute_image_hashes(data, width, height, channels).ok()?;

        // 2단계: 전체 DB 곡에 대해 해시 거리(Hamming Distance) 스코어링
        let mut candidates = self
            .entries
            .iter()
            .enumerate()
            .map(|(idx, entry)| {
                let p_dist = (entry.phash ^ q_phash).count_ones() as f32;
                let d_dist = (entry.dhash ^ q_dhash).count_ones() as f32;
                let a_dist = (entry.ahash ^ q_ahash).count_ones() as f32;
                let score = 0.5 * p_dist + 0.3 * d_dist + 0.2 * a_dist;
                (idx, score)
            })
            .collect::<Vec<_>>();

        // 해시 스코어 정렬 (낮을수록 가까움)
        candidates.sort_by(|a, b| a.1.total_cmp(&b.1));

        if candidates.is_empty() {
            return None;
        }

        let first_idx = candidates[0].0;
        let first_score = candidates[0].1;
        let first_hash_sim = (1.0 - first_score / 64.0).clamp(0.0, 1.0);

        // 3단계: HOG 연산 스킵 여부 판정
        let skip_hog = if self.config.disable_hog {
            true
        } else if candidates.len() > 1 {
            let second_score = candidates[1].1;
            let margin = second_score - first_score;
            margin >= self.config.margin_threshold || first_score == 0.0
        } else {
            true
        };

        if skip_hog {
            // HOG 생략 시 최종 유사도는 해시 유사도 자체로 평가
            let similarity = first_hash_sim;
            return (similarity >= self.config.similarity_threshold).then(|| ImageMatch {
                image_id: self.entries[first_idx].image_id.clone(),
                similarity,
            });
        }

        // 4단계: HOG 정밀 매칭 (Margin이 좁은 경우에만 게으르게 HOG 피처 계산)
        let q_hog = overmax_cv::compute_image_hog(data, width, height, channels).ok()?;
        let q_hog_norm = vector_norm(&q_hog).max(1.0);

        // 상위 top_k개 후보군에 대해서만 HOG Dot product 연산 적용
        let mut final_candidates = candidates
            .into_iter()
            .take(top_k.min(self.entries.len()))
            .map(|(idx, score)| {
                let entry = &self.entries[idx];
                let hash_sim = (1.0 - score / 64.0).clamp(0.0, 1.0);
                let hog_sim = dot(&entry.hog, &q_hog) / (entry.hog_norm * q_hog_norm);
                let similarity = 0.45 * hash_sim + 0.55 * hog_sim;
                (idx, similarity)
            })
            .collect::<Vec<_>>();

        // 최종 유사도 기준 내림차순 정렬 (높을수록 좋음)
        final_candidates.sort_by(|a, b| b.1.total_cmp(&a.1));

        final_candidates
            .into_iter()
            .next()
            .and_then(|(idx, similarity)| {
                (similarity >= self.config.similarity_threshold).then(|| ImageMatch {
                    image_id: self.entries[idx].image_id.clone(),
                    similarity,
                })
            })
    }
}

fn dot(left: &[f32], right: &[f32]) -> f32 {
    left.iter().zip(right).map(|(a, b)| a * b).sum()
}

fn vector_norm(values: &[f32]) -> f32 {
    values.iter().map(|value| value * value).sum::<f32>().sqrt()
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
        }
    }

    #[test]
    fn test_jacket_matcher_basic_match() {
        let entries = vec![
            dummy_entry("song-a", 0x0000_0000_0000_0000, 0.1),
            dummy_entry("song-b", 0xFFFF_FFFF_FFFF_FFFF, 0.2),
        ];
        let config = JacketMatcherConfig {
            similarity_threshold: 0.7,
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
