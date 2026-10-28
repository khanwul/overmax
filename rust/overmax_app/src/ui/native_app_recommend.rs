use crate::ui::debug_ui;
use crate::ui::native_app::NativeApp;
use crate::ui::overlay_recommend_ui::PatternTabInfo;
use overmax_core::{Difficulty, GameSessionState};
use overmax_data::RecommendResult;

impl NativeApp {
    pub(crate) fn drain_detection_results(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        while let Ok(output) = self.detection_rx.try_recv() {
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            let mut output = output;
            #[cfg(any(debug_assertions, feature = "telemetry"))]
            if let (Some(telemetry), Some(delivery)) =
                (&self.runtime_telemetry, &mut output.delivery_telemetry)
            {
                telemetry.record_output_drained(delivery);
            }
            if let Some(snap) = output.telemetry_snapshot {
                self.last_telemetry_snapshot = Some(snap);
            }
            self.last_detection_output = Some(output.clone());
            if let Ok(mut r) = self.game_rect.lock() {
                *r = output.game_rect;
            }
            self.window_snapshot = output.window_snapshot;
            self.capture_fatal = output.capture_fatal.clone();

            self.confidence = output.confidence;
            if self.session != output.state {
                changed = true;
                self.session = output.state.clone();
            }

            if let Some(event) =
                self.runtime
                    .on_detection(&output.state, output.event.as_ref(), &self.varchive_db)
            {
                debug_ui::push_log(
                    &self.debug_state.log_lines,
                    self.max_log_lines(),
                    format!(
                        "[Main] 기록 갱신: {}, {}, {}, {:.2}%, MaxCombo: {}",
                        event.song_id, event.mode, event.diff, event.rate, event.is_max_combo
                    ),
                );
                changed = true;
            }
        }

        if changed {
            self.refresh_overlay_data();
            if let Ok(recs) = serde_json::to_value(&self.recommendations) {
                crate::system::ipc_server::update_latest_recommendations(recs);
            }
            self.log_overlay_state();
            ctx.request_repaint();
        }
    }

    pub(crate) fn current_song_label(&self) -> String {
        let Some(ctx) = &self.session.context else {
            return crate::t!("rec-select-song").into();
        };
        let Some(song) = self.varchive_db.search_by_id(ctx.song_id) else {
            return format!("Song #{}", ctx.song_id);
        };
        song.name.to_string()
    }

    pub(crate) fn refresh_overlay_data(&mut self) {
        self.pattern_tabs = self.pattern_tabs_for_state(&self.session);
        self.recommendations = self.recommend_for_state(&self.session);
        // 업로드 필요 여부의 입력(세션 컨텍스트, record_manager 상태)이 바뀌는 모든
        // 경로(감지 결과, 업로드 결과, V-Archive 조회, Steam 세션 전환 등)가 이 함수를
        // 거치므로 여기서 1회 계산해 캐시한다. 렌더 경로는 필드만 참조한다.
        self.overlay_upload_needed = self.current_pattern_needs_upload();
    }

    fn recommend_for_state(&self, state: &GameSessionState) -> RecommendResult {
        overmax_runtime::derived::recommend_for_state(
            &self.recommender,
            &self.settings.get_merged(),
            &self.root,
            self.varchive_user_id(),
            state,
        )
    }

    fn pattern_tabs_for_state(&self, state: &GameSessionState) -> Vec<PatternTabInfo> {
        let Some(ctx) = &state.context else {
            return Vec::new();
        };
        let Some(song) = self.varchive_db.search_by_id(ctx.song_id) else {
            return Vec::new();
        };
        let m = ctx.mode;
        let patterns = &song.patterns[m as usize];
        Difficulty::ALL
            .iter()
            .filter_map(|&d| {
                let pattern = patterns[d as usize].as_ref()?;
                let meta = self.sheet_meta.get(&song.title, m, d);
                Some(PatternTabInfo {
                    diff: d,
                    level: pattern.level,
                    floor_name: pattern.floor_name.as_ref().map(|s| s.to_string()),
                    gold: meta.gold,
                    note: meta.note,
                    assist_key: meta.assist_key,
                    keypart: meta.keypart,
                })
            })
            .collect()
    }

    fn log_overlay_state(&self) {
        debug_ui::push_log(
            &self.debug_state.log_lines,
            self.max_log_lines(),
            format!(
                "[UI] overlay state <- {} / recs={}",
                self.session,
                self.recommendations.entries.len()
            ),
        );
    }
}
