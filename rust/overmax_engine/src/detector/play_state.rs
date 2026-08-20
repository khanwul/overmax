use crate::capture::frame::CapturedFrame;
use crate::capture::frame_utils::{compute_pixel_checksum, region_mean_bgr};
use crate::detector::roi::RoiManager;
use crate::detector::templates;
use overmax_core::{
    Changed, Difficulty, GameSessionState, Mode, PlayContext, RecordKey, VerifiedPlayEvent,
};
use std::collections::VecDeque;

pub const MIN_VALID_RATE: f32 = 80.0;

const BTN_MODE_MAX_DIST: f32 = 60.0;
const DIFF_MIN_BRIGHTNESS: f32 = 45.0;
const DIFF_CONFIDENT_MARGIN: f32 = 15.0;
const RATE_DETECTION_INTERVAL_SEC: f64 = 0.20;
const RATE_CHECKSUM_CHANGE_THRESHOLD: u64 = 50;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct RateInputChecksums {
    score: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ModeDiffChecksums {
    mode: u64,
    diff: u64,
}

fn rate_inputs_changed(previous: Option<RateInputChecksums>, current: RateInputChecksums) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    previous.score.abs_diff(current.score) > RATE_CHECKSUM_CHANGE_THRESHOLD
}

#[derive(Clone, Debug, PartialEq)]
struct RawPlayState {
    context: Option<PlayContext>,
}

/// 결과창 mode·diff 인식 결과 캐시.
struct ModeDiffCache {
    result_mode: Changed<Option<Mode>>,
    result_diff: Changed<Option<Difficulty>>,
}

impl ModeDiffCache {
    fn new() -> Self {
        Self {
            result_mode: Changed::new(None),
            result_diff: Changed::new(None),
        }
    }

    /// 결과창 -> 선곡창 복귀 시 결과창 인식값을 초기화한다.
    fn clear_result_cache(&mut self) {
        self.result_mode.update(None);
        self.result_diff.update(None);
    }
}

pub struct PlayStateDetector {
    history_size: usize,
    history: VecDeque<Option<RawPlayState>>,
    last_stable_state: Option<GameSessionState>,
    last_rate_checksums: Option<RateInputChecksums>,
    last_rate_result: Option<f32>,
    last_rate_detection_ts: f64,
    last_mode_diff_checksums: Option<ModeDiffChecksums>,
    last_mode_diff_result: Option<(Option<Mode>, Option<Difficulty>, bool)>,
    last_mode_diff_detection_ts: f64,
    cache: ModeDiffCache,
    last_song_id: Changed<Option<i32>>,
    last_target_pattern: Option<RecordKey>,
    result_rate_window: VecDeque<f32>,
    event_emitted_for_session: bool,
}

impl PlayStateDetector {
    pub fn stable_hits(&self) -> u32 {
        self.history.iter().filter(|h| h.is_some()).count() as u32
    }

    fn should_run_rate_detection(&self, now: f64) -> bool {
        now - self.last_rate_detection_ts >= RATE_DETECTION_INTERVAL_SEC
    }

    fn rate_input_checksums(
        frame: &CapturedFrame,
        rois: &RoiManager,
    ) -> Option<RateInputChecksums> {
        let score = rois
            .get_roi("score")
            .and_then(|roi| compute_pixel_checksum(frame, roi))
            .or_else(|| {
                rois.get_roi("rate")
                    .and_then(|roi| compute_pixel_checksum(frame, roi))
            })?;
        Some(RateInputChecksums { score })
    }

    fn process_rate_detection(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        scene: overmax_core::SceneType,
        is_result: bool,
        now: f64,
    ) -> f32 {
        if self.should_run_rate_detection(now) {
            let checksums = Self::rate_input_checksums(frame, rois);
            let inputs_changed = checksums
                .map(|current| rate_inputs_changed(self.last_rate_checksums, current))
                .unwrap_or(true);

            if inputs_changed {
                let is_song_select = matches!(
                    scene,
                    overmax_core::SceneType::Freestyle | overmax_core::SceneType::OpenMatch
                );

                let mut detected_rate = None;
                if let Some(score_val) = rois.and_then_roi(frame, "score", templates::detect_score)
                {
                    let calc_rate = (score_val as f32 / 10000.0 * 100.0).floor() / 100.0;
                    let is_valid_range = if is_song_select {
                        (MIN_VALID_RATE..=100.0).contains(&calc_rate)
                    } else {
                        (0.0..=100.0).contains(&calc_rate)
                    };

                    if is_valid_range {
                        debug_println!(
                            "    [detect] score run. score={}, rate={:.2}%",
                            score_val,
                            calc_rate
                        );
                        detected_rate = Some(calc_rate);
                    }
                } else if let Some(rate_res) =
                    rois.and_then_roi(frame, "rate", |img| templates::detect_rate(img))
                {
                    detected_rate = Some(rate_res);
                }

                self.last_rate_detection_ts = now;
                self.last_rate_checksums = checksums;
                self.apply_rate_detection_result(is_result, detected_rate);
            }
        }

        self.last_rate_result.unwrap_or(0.0)
    }

    fn apply_rate_detection_result(&mut self, is_result: bool, mut res: Option<f32>) {
        if is_result {
            if let Some(new_r) = res {
                self.push_result_rate_sample(new_r);
                res = self.median_result_rate();
                self.last_rate_result = res;
            }
        } else {
            self.result_rate_window.clear();
            self.last_rate_result = res;
        }
    }

    fn push_result_rate_sample(&mut self, r: f32) {
        self.result_rate_window.push_back(r);
        if self.result_rate_window.len() > 7 {
            self.result_rate_window.pop_front();
        }
    }

    fn median_result_rate(&self) -> Option<f32> {
        let mut sorted: Vec<f32> = self.result_rate_window.iter().cloned().collect();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        sorted.get(sorted.len() / 2).copied()
    }

    fn mode_diff_checksums(frame: &CapturedFrame, rois: &RoiManager) -> Option<ModeDiffChecksums> {
        let mode = rois
            .get_roi("btn_mode")
            .and_then(|roi| compute_pixel_checksum(frame, roi))?;

        let mut diff: u64 = 0;
        for d in Difficulty::ALL {
            if let Some(roi) = rois.get_diff_panel_roi(d) {
                if let Some(cs) = compute_pixel_checksum(frame, roi) {
                    diff = diff.wrapping_add(cs);
                }
            }
        }
        Some(ModeDiffChecksums { mode, diff })
    }

    fn process_mode_diff_detection(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        now: f64,
    ) -> (Option<Mode>, Option<Difficulty>, bool) {
        let checksums = Self::mode_diff_checksums(frame, rois);
        let checksums_changed = checksums.is_none() || checksums != self.last_mode_diff_checksums;

        if checksums_changed || self.last_mode_diff_result.is_none() {
            let m = detect_button_mode(frame, rois);
            let (d, conf) = detect_difficulty(frame, rois);
            self.last_mode_diff_checksums = checksums;
            self.last_mode_diff_detection_ts = now;
            self.last_mode_diff_result = Some((m, d, conf));
            (m, d, conf)
        } else if let Some(res) = self.last_mode_diff_result {
            res
        } else {
            let m = detect_button_mode(frame, rois);
            let (d, conf) = detect_difficulty(frame, rois);
            (m, d, conf)
        }
    }

    pub fn new(history_size: usize) -> Self {
        Self {
            history_size: history_size.max(1),
            history: VecDeque::new(),
            last_stable_state: None,
            last_rate_checksums: None,
            last_rate_result: None,
            last_rate_detection_ts: 0.0,
            last_mode_diff_checksums: None,
            last_mode_diff_result: None,
            last_mode_diff_detection_ts: 0.0,
            cache: ModeDiffCache::new(),
            last_song_id: Changed::new(None),
            last_target_pattern: None,
            result_rate_window: VecDeque::new(),
            event_emitted_for_session: false,
        }
    }

    pub fn reset(&mut self) {
        self.history.clear();
        self.last_stable_state = None;
        self.last_rate_checksums = None;
        self.last_rate_result = None;
        self.last_rate_detection_ts = 0.0;
        self.last_mode_diff_checksums = None;
        self.last_mode_diff_result = None;
        self.last_mode_diff_detection_ts = 0.0;
        // 결과창 진입 시 복구용(result_mode/diff) 캐시는 reset 시에도 보존합니다.
        self.last_song_id.update(None);
        self.last_target_pattern = None;
        self.result_rate_window.clear();
        self.event_emitted_for_session = false;
    }

    pub fn clear_detected_cache(&mut self) {
        self.cache.clear_result_cache();
        self.event_emitted_for_session = false;
        self.last_rate_result = None;
        self.last_rate_checksums = None;
        self.result_rate_window.clear();
        self.last_target_pattern = None;
    }

    #[cfg(test)]
    pub(crate) fn seed_detected_cache_for_test(&mut self) {
        self.cache.result_mode.update(Some(Mode::B4));
        self.cache.result_diff.update(Some(Difficulty::NM));
    }

    #[cfg(test)]
    pub(crate) fn detected_cache_is_empty_for_test(&self) -> bool {
        self.cache.result_mode.get().is_none() && self.cache.result_diff.get().is_none()
    }

    fn resolve_result_mode_diff(
        &mut self,
        scene: overmax_core::SceneType,
        frame: &CapturedFrame,
        rois: &RoiManager,
    ) -> (Option<Mode>, Option<Difficulty>) {
        let mut detected_mode = None;
        let mut detected_diff = None;

        // 1. 결과창 실시간 템플릿 매칭 우선 시도
        match scene {
            overmax_core::SceneType::ResultFreestyle => {
                detected_mode =
                    rois.and_then_roi(frame, "mode_digit", templates::detect_freestyle_mode);
                detected_diff =
                    rois.and_then_roi(frame, "diff_panel", templates::detect_result_difficulty);
            }
            overmax_core::SceneType::ResultOpen3 | overmax_core::SceneType::ResultOpen2 => {
                detected_mode = detect_button_mode_from_roi(frame, rois, "openmatch_mode");
                detected_diff = rois.and_then_roi(
                    frame,
                    "openmatch_diff",
                    templates::detect_openmatch_result_difficulty,
                );
            }
            _ => {}
        }

        // 2. 결과창 템플릿 매칭 성공 시, 결과창 캐시를 업데이트 (자가 보정 가능)
        if detected_mode.is_some() {
            self.cache.result_mode.update(detected_mode);
        }
        if detected_diff.is_some() {
            self.cache.result_diff.update(detected_diff);
        }

        // 3. 최종 반환값 결정: 결과창 캐시가 존재하면 우선 사용
        let final_mode = *self.cache.result_mode.get();
        let final_diff = *self.cache.result_diff.get();

        (final_mode, final_diff)
    }

    pub fn detect(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        song_id: Option<i32>,
        now: f64,
    ) -> (GameSessionState, Option<VerifiedPlayEvent>) {
        let scene = rois.current_scene();
        let is_result = scene.is_result();

        let mode;
        let diff;
        let mut confident = true;
        let is_max_combo;

        if is_result {
            is_max_combo = detect_max_combo_result(frame, rois);
            let (m, d) = self.resolve_result_mode_diff(scene, frame, rois);
            mode = m;
            diff = d;
        } else {
            self.cache.result_mode.update(None);
            self.cache.result_diff.update(None);
            self.event_emitted_for_session = false;

            let (m, d, conf) = self.process_mode_diff_detection(frame, rois, now);
            mode = m;
            diff = d;
            confident = conf;
            is_max_combo = detect_max_combo(frame, rois);
        }

        let current_pattern = match (song_id, mode, diff) {
            (Some(sid), Some(m), Some(d)) => Some((sid, m, d)),
            _ => None,
        };

        if !is_result && current_pattern != self.last_target_pattern {
            self.last_target_pattern = current_pattern;
            self.last_rate_result = None;
            self.last_rate_checksums = None;
        }

        self.last_song_id.update(song_id);

        debug_println!(
            "    [detect] song_id={:?}, mode={:?}, diff={:?}, confident={}",
            song_id,
            mode,
            diff,
            confident
        );
        let context = if let (Some(sid), Some(m), Some(d)) = (song_id, mode, diff) {
            if confident {
                let rate = self.process_rate_detection(frame, rois, scene, is_result, now);

                let rate_valid = !is_result
                    || self
                        .last_rate_result
                        .map(|r| r >= MIN_VALID_RATE)
                        .unwrap_or(false);

                Some(PlayContext {
                    song_id: sid,
                    mode: m,
                    diff: d,
                    rate: if rate_valid { rate } else { 0.0 },
                    is_max_combo: if rate_valid && rate > 0.0 {
                        is_max_combo
                    } else {
                        false
                    },
                })
            } else {
                None
            }
        } else {
            None
        };

        let raw = RawPlayState {
            context: context.clone(),
        };
        self.push_raw(raw);

        let stable_context = self.stable_raw().map(|s| s.context.clone());
        if let Some(stable_ctx) = stable_context {
            let state = GameSessionState {
                scene,
                context: stable_ctx.clone(),
                is_stable: true,
                is_fullscreen: false, // will be overwritten/updated by detection worker
            };
            self.last_stable_state = Some(state.clone());

            let mut verified_event = None;
            if is_result && !self.event_emitted_for_session {
                if let Some(ctx) = &stable_ctx {
                    if ctx.rate >= MIN_VALID_RATE {
                        self.event_emitted_for_session = true;
                        verified_event = Some(VerifiedPlayEvent {
                            song_id: ctx.song_id,
                            mode: ctx.mode,
                            diff: ctx.diff,
                            rate: ctx.rate,
                            is_max_combo: ctx.is_max_combo,
                            is_result_screen: true,
                        });
                    }
                }
            }

            return (state, verified_event);
        }

        (
            GameSessionState {
                scene,
                context,
                is_stable: false,
                is_fullscreen: false,
            },
            None,
        )
    }

    fn push_raw(&mut self, raw: RawPlayState) {
        if self.history.len() == self.history_size {
            self.history.pop_front();
        }
        self.history.push_back(raw.context.is_some().then_some(raw));
    }

    fn stable_raw(&self) -> Option<&RawPlayState> {
        if self.history.len() != self.history_size {
            return None;
        }
        let first = self.history.front()?.as_ref()?;
        self.history
            .iter()
            .all(|item| item.as_ref() == Some(first))
            .then_some(first)
    }
}

pub fn detect_button_mode_from_roi(
    frame: &CapturedFrame,
    rois: &RoiManager,
    roi_name: &str,
) -> Option<Mode> {
    let roi = rois.get_roi(roi_name)?;
    let mean = region_mean_bgr(frame, roi);
    let mut best = (None, f32::INFINITY);

    let colors_table = if roi_name == "openmatch_mode" {
        &templates::OPENMATCH_MODE_COLORS
    } else {
        &templates::FREESTYLE_MODE_COLORS
    };

    for (idx, &color) in colors_table.iter().enumerate() {
        let dist = mean.distance_f32(color);
        if dist < best.1 {
            best = (Some(Mode::ALL[idx]), dist);
        }
    }
    (best.1 <= BTN_MODE_MAX_DIST).then_some(best.0).flatten()
}

pub fn detect_button_mode(frame: &CapturedFrame, rois: &RoiManager) -> Option<Mode> {
    detect_button_mode_from_roi(frame, rois, "btn_mode")
}

pub fn detect_difficulty(frame: &CapturedFrame, rois: &RoiManager) -> (Option<Difficulty>, bool) {
    let mut brightnesses = Difficulty::ALL
        .iter()
        .filter_map(|&diff| {
            let roi = rois.get_diff_panel_roi(diff)?;
            let mean = region_mean_bgr(frame, roi);
            Some((diff, mean.to_f64().average() as f32))
        })
        .collect::<Vec<_>>();
    brightnesses.sort_by(|a, b| b.1.total_cmp(&a.1));
    let Some((best, max_bright)) = brightnesses.first().copied() else {
        return (None, false);
    };
    if max_bright < DIFF_MIN_BRIGHTNESS {
        return (None, false);
    }
    let second = brightnesses.get(1).map_or(0.0, |item| item.1);
    (Some(best), max_bright - second >= DIFF_CONFIDENT_MARGIN)
}

// 선곡창 Perfect Play (100.0%) 뱃지 대표 해시
const TEMPLATE_SELECT_PERFECT_PHASH: u64 = 0xdca6ef1001714f9e;
const TEMPLATE_SELECT_PERFECT_DHASH: u64 = 0xe4a5a484b4551545;
const TEMPLATE_SELECT_PERFECT_AHASH: u64 = 0x3ffdf4600cdcdcb8;

// 선곡창 Max Combo 뱃지 대표 해시
const TEMPLATE_SELECT_MC_PHASH: u64 = 0xc25a6a8e372b67c8;
const TEMPLATE_SELECT_MC_DHASH: u64 = 0x4909a11e9266a98f;
const TEMPLATE_SELECT_MC_AHASH: u64 = 0x15f4f0073effff03;

// 결과창 Perfect Play (100.0%) 뱃지 대표 해시
const TEMPLATE_RESULT_PERFECT_PHASH: u64 = 0xdea7c998117c851e;
const TEMPLATE_RESULT_PERFECT_DHASH: u64 = 0xd455544439b5b5a5;
const TEMPLATE_RESULT_PERFECT_AHASH: u64 = 0x3fbdf4e014ddd450;

// 결과창 Max Combo 뱃지 대표 해시
const TEMPLATE_RESULT_MC_PHASH: u64 = 0xda5a52d2123b2fe8;
const TEMPLATE_RESULT_MC_DHASH: u64 = 0x2929137dd4ef210f;
const TEMPLATE_RESULT_MC_AHASH: u64 = 0xd4fce007fffffc00;

fn calculate_hash_score(
    phash: u64,
    dhash: u64,
    ahash: u64,
    t_phash: u64,
    t_dhash: u64,
    t_ahash: u64,
) -> f32 {
    let p_dist = (phash ^ t_phash).count_ones() as f32;
    let d_dist = (dhash ^ t_dhash).count_ones() as f32;
    let a_dist = (ahash ^ t_ahash).count_ones() as f32;
    0.5 * p_dist + 0.3 * d_dist + 0.2 * a_dist
}

pub fn detect_max_combo(frame: &CapturedFrame, rois: &RoiManager) -> bool {
    let hashes = rois.and_then_roi(frame, "max_combo_badge", |img| img.compute_hashes(4).ok());

    let Some((phash, dhash, ahash)) = hashes else {
        return false;
    };
    let score_perfect = calculate_hash_score(
        phash,
        dhash,
        ahash,
        TEMPLATE_SELECT_PERFECT_PHASH,
        TEMPLATE_SELECT_PERFECT_DHASH,
        TEMPLATE_SELECT_PERFECT_AHASH,
    );
    let score_mc = calculate_hash_score(
        phash,
        dhash,
        ahash,
        TEMPLATE_SELECT_MC_PHASH,
        TEMPLATE_SELECT_MC_DHASH,
        TEMPLATE_SELECT_MC_AHASH,
    );
    score_perfect <= 10.0 || score_mc <= 10.0
}

pub fn detect_max_combo_result(frame: &CapturedFrame, rois: &RoiManager) -> bool {
    let hashes = rois.and_then_roi(frame, "max_combo_badge", |img| img.compute_hashes(4).ok());

    let Some((phash, dhash, ahash)) = hashes else {
        return false;
    };
    let score_perfect = calculate_hash_score(
        phash,
        dhash,
        ahash,
        TEMPLATE_RESULT_PERFECT_PHASH,
        TEMPLATE_RESULT_PERFECT_DHASH,
        TEMPLATE_RESULT_PERFECT_AHASH,
    );
    let score_mc = calculate_hash_score(
        phash,
        dhash,
        ahash,
        TEMPLATE_RESULT_MC_PHASH,
        TEMPLATE_RESULT_MC_DHASH,
        TEMPLATE_RESULT_MC_AHASH,
    );
    score_perfect <= 20.0 || score_mc <= 20.0
}

#[cfg(test)]
mod tests {
    use super::{detect_button_mode, rate_inputs_changed, PlayStateDetector, RateInputChecksums};
    use crate::capture::frame::CapturedFrame;
    use crate::detector::roi::RoiManager;
    use overmax_core::SceneType;
    use overmax_cv::Bgr;

    #[test]
    fn detects_button_mode_from_reference_color() {
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // #2D4F55
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);
        assert_eq!(detect_button_mode(&frame, &rois), Some(super::Mode::B4));
    }

    #[test]
    fn marks_state_stable_after_repeated_valid_frames() {
        let mut detector = PlayStateDetector::new(3);
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // #2D4F55
        paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // #DCDCDC
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);

        assert!(!detector.detect(&frame, &rois, Some(7), 1.0).0.is_stable);
        assert!(!detector.detect(&frame, &rois, Some(7), 2.0).0.is_stable);
        assert!(detector.detect(&frame, &rois, Some(7), 3.0).0.is_stable);
    }

    #[test]
    fn result_mode_diff_remains_none_without_match() {
        let mut detector = PlayStateDetector::new(3);
        detector.cache.clear_result_cache();

        let frame = blank_frame();
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::ResultFreestyle);

        // 결과창에서 mode_digit/diff_panel ROI가 없어 인식에 실패하면 None이어야 함
        let (state, event) = detector.detect(&frame, &rois, Some(7), 1.0);
        assert!(state.context.is_none());
        assert!(event.is_none());
    }

    #[test]
    fn mode_diff_checksum_cache_only_refreshes_on_input_change() {
        let mut detector = PlayStateDetector::new(3);
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
        paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NORMAL diff
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);

        let (state1, _) = detector.detect(&frame, &rois, Some(1), 1.0);
        assert_eq!(detector.last_mode_diff_detection_ts, 1.0);
        assert!(detector.last_mode_diff_checksums.is_some());

        // 동일 프레임에서 시간만 지나도 체크섬이 안 바뀌면 timestamp가 1.0으로 유지됨 (재계산 스킵)
        let (state2, _) = detector.detect(&frame, &rois, Some(1), 2.0);
        assert_eq!(detector.last_mode_diff_detection_ts, 1.0);
        assert_eq!(
            state1.context.as_ref().map(|c| c.mode),
            state2.context.as_ref().map(|c| c.mode)
        );

        // 난이도 카드 하이라이트 변경 -> checksum 변경으로 3.0s 시점에 갱신됨
        paint_rect(&mut frame, 218, 488, 328, 516, Bgr::from_rgb_hex(0xFFFFFF));
        let (_state3, _) = detector.detect(&frame, &rois, Some(1), 3.0);
        assert_eq!(detector.last_mode_diff_detection_ts, 3.0);
    }

    fn blank_frame() -> CapturedFrame {
        CapturedFrame {
            width: 1920,
            height: 1080,
            bgra: vec![0; 1920 * 1080 * 4],
        }
    }

    fn paint_rect(frame: &mut CapturedFrame, x1: i32, y1: i32, x2: i32, y2: i32, bgr: Bgr) {
        for y in y1..y2 {
            for x in x1..x2 {
                let idx = ((y * frame.width + x) * 4) as usize;
                bgr.write_to_bgra(&mut frame.bgra[idx..idx + 4], 255);
            }
        }
    }

    #[test]
    fn rate_checksum_cache_only_refreshes_on_meaningful_input_change() {
        let previous = RateInputChecksums { score: 2_000 };

        assert!(!rate_inputs_changed(
            Some(previous),
            RateInputChecksums { score: 2_049 }
        ));
        assert!(rate_inputs_changed(
            Some(previous),
            RateInputChecksums { score: 2_051 }
        ));
    }

    #[test]
    fn test_verified_play_event_emits_once_per_result_session() {
        let mut detector = PlayStateDetector::new(3);
        detector.cache.result_mode.update(Some(super::Mode::B4));
        detector
            .cache
            .result_diff
            .update(Some(super::Difficulty::NM));
        detector.last_rate_result = Some(99.5); // MIN_VALID_RATE (80.0) 이상

        let frame = blank_frame();
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::ResultFreestyle);

        // Frame 1: Detecting (History 1/3) -> Event None
        let (state1, event1) = detector.detect(&frame, &rois, Some(10), 1.0);
        assert!(!state1.is_stable);
        assert!(event1.is_none());

        // Frame 2: Detecting (History 2/3) -> Event None
        let (state2, event2) = detector.detect(&frame, &rois, Some(10), 1.1);
        assert!(!state2.is_stable);
        assert!(event2.is_none());

        // Frame 3: Stable 확정 (History 3/3) -> Event 단 1회 방출!
        let (state3, event3) = detector.detect(&frame, &rois, Some(10), 1.2);
        assert!(state3.is_stable);
        assert!(event3.is_some());
        let event = event3.unwrap();
        assert_eq!(event.song_id, 10);
        assert_eq!(event.mode, super::Mode::B4);
        assert_eq!(event.diff, super::Difficulty::NM);
        assert_eq!(event.rate, 99.5);
        assert!(event.is_result_screen);

        // Frame 4: 결과창 체류 지속 -> Event 중복 방출 0회 (None)
        let (state4, event4) = detector.detect(&frame, &rois, Some(10), 1.3);
        assert!(state4.is_stable);
        assert!(
            event4.is_none(),
            "결과창 체류 중 중복 이벤트가 방출되면 안 됨"
        );

        // Frame 5: 결과창 체류 지속 -> Event 중복 방출 0회 (None)
        let (state5, event5) = detector.detect(&frame, &rois, Some(10), 1.4);
        assert!(state5.is_stable);
        assert!(event5.is_none());

        // 선곡 화면으로 복귀 -> 래치 자동 리셋
        rois.set_scene(SceneType::Freestyle);
        detector.clear_detected_cache();
        let (state_select, event_select) = detector.detect(&frame, &rois, Some(10), 2.0);
        assert!(event_select.is_none());
        assert!(!state_select.scene.is_result());

        // 다음 곡 플레이 후 새 결과창 진입
        rois.set_scene(SceneType::ResultFreestyle);
        detector.cache.result_mode.update(Some(super::Mode::B6));
        detector
            .cache
            .result_diff
            .update(Some(super::Difficulty::HD));
        detector.last_rate_result = Some(98.0);

        let _ = detector.detect(&frame, &rois, Some(20), 3.0);
        let _ = detector.detect(&frame, &rois, Some(20), 3.1);
        let (state_new_res, event_new_res) = detector.detect(&frame, &rois, Some(20), 3.2);

        assert!(state_new_res.is_stable);
        assert!(
            event_new_res.is_some(),
            "새 결과창 진입 시 래치가 리셋되어 새 이벤트가 방출되어야 함"
        );
        let new_event = event_new_res.unwrap();
        assert_eq!(new_event.song_id, 20);
        assert_eq!(new_event.mode, super::Mode::B6);
        assert_eq!(new_event.diff, super::Difficulty::HD);
        assert_eq!(new_event.rate, 98.0);
    }

    #[test]
    fn unplayed_song_after_recorded_song_does_not_retain_previous_rate() {
        let mut detector = PlayStateDetector::new(1);
        let mut frame = blank_frame();
        // 4B Mode Color & NM Diff Card
        paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
        paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NM
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);

        // 1. 곡 1 (기록 있음: 98.5%)
        detector.last_rate_result = Some(98.5);
        detector.last_target_pattern = Some((1, super::Mode::B4, super::Difficulty::NM));
        detector.last_rate_checksums = PlayStateDetector::rate_input_checksums(&frame, &rois);

        let (state1, _) = detector.detect(&frame, &rois, Some(1), 1.0);
        assert_eq!(state1.context.as_ref().unwrap().rate, 98.5);

        // 2. 곡 2 (기록 없음 0.00% / 미플레이)로 변경
        let (state2, _) = detector.detect(&frame, &rois, Some(2), 2.0);
        assert_eq!(
            state2.context.as_ref().unwrap().rate,
            0.0,
            "이전 곡의 기록(98.5%)이 새 미플레이 곡(0.00%)에 잔류하면 안 됨"
        );
        assert_eq!(detector.last_rate_result, None);
    }

    #[test]
    fn rate_input_change_without_valid_score_clears_cached_rate() {
        let mut detector = PlayStateDetector::new(1);
        let mut frame = blank_frame();
        paint_rect(&mut frame, 80, 130, 85, 135, Bgr::from_rgb_hex(0x2D4F55)); // 4B
        paint_rect(&mut frame, 98, 488, 208, 516, Bgr::from_rgb_hex(0xDCDCDC)); // NM
        let mut rois = RoiManager::new(1920, 1080);
        rois.set_scene(SceneType::Freestyle);

        // 임의의 이전 rate 캐시와 체크섬 설정
        detector.last_rate_result = Some(95.0);
        detector.last_rate_checksums = Some(RateInputChecksums { score: 100 });
        detector.last_target_pattern = Some((1, super::Mode::B4, super::Difficulty::NM));

        // score 영역에 체크섬 변경 유발 (하지만 빈 화면이라 유효한 점수가 검출되지 않음)
        paint_rect(
            &mut frame,
            1000,
            100,
            1050,
            120,
            Bgr::from_rgb_hex(0xFFFFFF),
        );

        let (state, _) = detector.detect(&frame, &rois, Some(1), 1.0);
        assert_eq!(
            state.context.as_ref().unwrap().rate,
            0.0,
            "유효한 점수가 없는 새로운 입력 감지 시 캐시된 점수가 0.0으로 클리어되어야 함"
        );
        assert_eq!(detector.last_rate_result, None);
    }
}
