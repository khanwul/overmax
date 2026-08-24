use std::time::Instant;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TimingAggregator {
    pub total_us: u64,
    pub max_us: u64,
    pub count: u32,
}

impl TimingAggregator {
    pub fn update(&mut self, elapsed_us: u64) {
        self.total_us += elapsed_us;
        if elapsed_us > self.max_us {
            self.max_us = elapsed_us;
        }
        self.count += 1;
    }

    pub fn avg_us(&self) -> u64 {
        if self.count == 0 {
            0
        } else {
            self.total_us / self.count as u64
        }
    }

    pub fn avg_ms(&self) -> f32 {
        self.avg_us() as f32 / 1000.0
    }

    pub fn max_ms(&self) -> f32 {
        self.max_us as f32 / 1000.0
    }

    pub fn reset(&mut self) {
        self.total_us = 0;
        self.max_us = 0;
        self.count = 0;
    }
}

pub fn format_duration_us(us: u64) -> String {
    if us >= 10_000 {
        format!("{:.1}ms", us as f64 / 1000.0)
    } else if us >= 1_000 {
        format!("{:.2}ms", us as f64 / 1000.0)
    } else {
        format!("{}µs", us)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PipelineTelemetrySnapshot {
    pub period_sec: f32,
    pub capture_avg_us: u64,
    pub capture_max_us: u64,
    pub detect_avg_us: u64,
    pub detect_max_us: u64,
    pub scene_avg_us: u64,
    pub scene_max_us: u64,
    pub jacket_avg_us: u64,
    pub jacket_max_us: u64,
    pub play_state_avg_us: u64,
    pub play_state_max_us: u64,
    pub active_frames: u32,
    pub unknown_frames: u32,
    /// 씬 폴링 파싱 실패(미스) 횟수
    pub scene_poll_misses: u32,
    /// 미스 시 관측된 썸네일 diff 최댓값 (게이트 임계 2.5 대조용)
    pub scene_miss_max_thumb_diff: f32,
    /// Centroid Kernel 게이트 동시 거절 횟수
    pub scene_miss_centroid_rejects: u32,
    /// 카테고리 띠 검사 동시 탈락 횟수
    pub scene_miss_band_rejects: u32,
    /// 유사도 미달로 탈락한 경우의 최저 top-similarity (0.0 = 해당 없음)
    pub scene_miss_min_top_similarity: f32,
    pub match_jacket_count: u32,
}

#[derive(Debug)]
pub struct PipelineStatsCollector {
    pub capture: TimingAggregator,
    pub detect: TimingAggregator,
    pub scene: TimingAggregator,
    pub jacket: TimingAggregator,
    pub play_state: TimingAggregator,
    pub active_frames: u32,
    pub unknown_frames: u32,
    pub scene_poll_misses: u32,
    pub scene_miss_max_thumb_diff: f32,
    pub scene_miss_centroid_rejects: u32,
    pub scene_miss_band_rejects: u32,
    pub scene_miss_min_top_similarity: f32,
    pub match_jacket_count: u32,
    pub last_snapshot_ts: Instant,
}

impl Default for PipelineStatsCollector {
    fn default() -> Self {
        Self::new()
    }
}

impl PipelineStatsCollector {
    pub fn new() -> Self {
        Self {
            capture: TimingAggregator::default(),
            detect: TimingAggregator::default(),
            scene: TimingAggregator::default(),
            jacket: TimingAggregator::default(),
            play_state: TimingAggregator::default(),
            active_frames: 0,
            unknown_frames: 0,
            scene_poll_misses: 0,
            scene_miss_max_thumb_diff: 0.0,
            scene_miss_centroid_rejects: 0,
            scene_miss_band_rejects: 0,
            scene_miss_min_top_similarity: 0.0,
            match_jacket_count: 0,
            last_snapshot_ts: Instant::now(),
        }
    }

    /// 씬 폴링 미스 기록. thumb_diff는 참조 썸네일과의 평균 픽셀 차이 (없으면 None).
    pub fn record_scene_miss(
        &mut self,
        thumb_diff: Option<f32>,
        diag: crate::detector::detection_pipeline::SceneMissDiag,
    ) {
        self.scene_poll_misses += 1;
        if let Some(diff) = thumb_diff {
            if diff > self.scene_miss_max_thumb_diff {
                self.scene_miss_max_thumb_diff = diff;
            }
        }
        if diag.centroid_rejected {
            self.scene_miss_centroid_rejects += 1;
        } else if diag.band_rejected {
            self.scene_miss_band_rejects += 1;
        }
        if let Some(sim) = diag.top_similarity {
            let min = if self.scene_miss_min_top_similarity > 0.0 {
                self.scene_miss_min_top_similarity
            } else {
                f32::MAX
            };
            self.scene_miss_min_top_similarity = sim.min(min);
        }
    }

    pub fn record_match_jacket(&mut self) {
        self.match_jacket_count += 1;
    }

    pub fn record_frame_status(&mut self, is_active: bool) {
        if is_active {
            self.active_frames += 1;
        } else {
            self.unknown_frames += 1;
        }
    }

    pub fn maybe_take_snapshot(&mut self, interval_sec: f32) -> Option<PipelineTelemetrySnapshot> {
        let elapsed = self.last_snapshot_ts.elapsed().as_secs_f32();
        if elapsed < interval_sec {
            return None;
        }

        let snapshot = PipelineTelemetrySnapshot {
            period_sec: elapsed,
            capture_avg_us: self.capture.avg_us(),
            capture_max_us: self.capture.max_us,
            detect_avg_us: self.detect.avg_us(),
            detect_max_us: self.detect.max_us,
            scene_avg_us: self.scene.avg_us(),
            scene_max_us: self.scene.max_us,
            jacket_avg_us: self.jacket.avg_us(),
            jacket_max_us: self.jacket.max_us,
            play_state_avg_us: self.play_state.avg_us(),
            play_state_max_us: self.play_state.max_us,
            active_frames: self.active_frames,
            unknown_frames: self.unknown_frames,
            scene_poll_misses: self.scene_poll_misses,
            scene_miss_max_thumb_diff: self.scene_miss_max_thumb_diff,
            scene_miss_centroid_rejects: self.scene_miss_centroid_rejects,
            scene_miss_band_rejects: self.scene_miss_band_rejects,
            scene_miss_min_top_similarity: if self.scene_miss_min_top_similarity == f32::MAX {
                0.0
            } else {
                self.scene_miss_min_top_similarity
            },
            match_jacket_count: self.match_jacket_count,
        };

        self.reset();
        Some(snapshot)
    }

    pub fn reset(&mut self) {
        self.capture.reset();
        self.detect.reset();
        self.scene.reset();
        self.jacket.reset();
        self.play_state.reset();
        self.active_frames = 0;
        self.unknown_frames = 0;
        self.scene_poll_misses = 0;
        self.scene_miss_max_thumb_diff = 0.0;
        self.scene_miss_centroid_rejects = 0;
        self.scene_miss_band_rejects = 0;
        self.scene_miss_min_top_similarity = 0.0;
        self.match_jacket_count = 0;
        self.last_snapshot_ts = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn timing_aggregator_avg_and_max() {
        let mut agg = TimingAggregator::default();
        agg.update(1000);
        agg.update(3000);
        agg.update(2000);

        assert_eq!(agg.count, 3);
        assert_eq!(agg.total_us, 6000);
        assert_eq!(agg.max_us, 3000);
        assert_eq!(agg.avg_us(), 2000);
        assert_eq!(agg.avg_ms(), 2.0);
        assert_eq!(agg.max_ms(), 3.0);

        agg.reset();
        assert_eq!(agg.count, 0);
        assert_eq!(agg.total_us, 0);
        assert_eq!(agg.max_us, 0);
        assert_eq!(agg.avg_us(), 0);
    }

    #[test]
    fn collector_snapshot_interval() {
        let mut collector = PipelineStatsCollector::new();
        collector.capture.update(1000);
        collector.record_frame_status(true);
        collector.record_match_jacket();

        assert!(collector.maybe_take_snapshot(10.0).is_none());
        assert_eq!(collector.capture.count, 1);

        let snapshot = collector.maybe_take_snapshot(0.0).unwrap();
        assert_eq!(snapshot.active_frames, 1);
        assert_eq!(snapshot.match_jacket_count, 1);
        assert_eq!(collector.capture.count, 0);
    }
}
