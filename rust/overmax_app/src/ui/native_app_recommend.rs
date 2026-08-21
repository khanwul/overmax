use crate::ui::debug_ui;
use crate::ui::native_app::NativeApp;
use crate::ui::overlay_recommend_ui::PatternTabInfo;
use overmax_core::{Difficulty, GameSessionState};
use overmax_data::{RecommendContext, RecommendResult, RecordSource};

impl NativeApp {
    pub(crate) fn drain_detection_results(&mut self, ctx: &egui::Context) {
        let mut changed = false;
        while let Ok(output) = self.detection_rx.try_recv() {
            if let Some(snap) = output.telemetry_snapshot {
                self.last_telemetry_snapshot = Some(snap);
            }
            self.last_detection_output = Some(output.clone());
            if let Ok(mut r) = self.game_rect.lock() {
                *r = output.game_rect;
            }
            self.window_snapshot = output.window_snapshot;
            self.capture_fatal = output.capture_fatal.clone();

            if output.state.scene.is_result() {
                if let Some(ctx_val) = &output.state.context {
                    if self.session_initial_record.is_none() {
                        let song_id = ctx_val.song_id;
                        let rate_map = self.record_manager.get_rate_map(&[song_id]);
                        if let Some(&(r, mc)) = rate_map.get(&(song_id, ctx_val.mode, ctx_val.diff))
                        {
                            self.session_initial_record = Some((r, mc));
                        } else {
                            self.session_initial_record = Some((0.0, false));
                        }
                    }
                }
            } else {
                self.session_initial_record = None;
            }

            self.confidence = output.confidence;
            if self.session != output.state {
                changed = true;
                self.session = output.state.clone();
            }

            if let Some(event) = output.event {
                if self.record_manager.handle_verified_play(&event) {
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
        }

        if changed {
            self.refresh_overlay_data();
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
    }

    fn recommend_for_state(&self, state: &GameSessionState) -> RecommendResult {
        let Some(ctx) = &state.context else {
            return RecommendResult::empty();
        };
        let smart_recommend = self.settings.get_merged().recommend().smart_recommend;
        let rec_ctx = RecommendContext {
            song_id: ctx.song_id,
            button_mode: ctx.mode,
            difficulty: ctx.diff,
            floor_range: 0.0,
            max_results: 6,
            same_mode_only: true,
            v_id: self.varchive_user_id(),
            smart_recommend,
        };

        let provider_settings = self.settings.get_merged().recommend_provider();
        if provider_settings.enabled {
            if let Some(url) = &provider_settings.url {
                let clean_url = url.trim();
                if !clean_url.is_empty() {
                    let provider_name = provider_settings
                        .name
                        .as_deref()
                        .filter(|n| !n.is_empty())
                        .unwrap_or("external_provider");
                    let cache_dir = self
                        .root
                        .join("cache")
                        .join("recommend_provider")
                        .join(provider_name);

                    let manifest =
                        crate::system::recommend_provider_fetch::get_cached_manifest(clean_url);

                    let reader = overmax_data::ProviderCacheReader::new(
                        provider_name,
                        provider_name,
                        cache_dir.clone(),
                        manifest.vary,
                        std::time::Duration::from_secs(manifest.ttl_sec),
                    );

                    let clean_url_clone = clean_url.to_string();
                    let rec_ctx_clone = rec_ctx.clone();
                    let cache_dir_clone = cache_dir.clone();
                    let cache_key = reader.cache_key(&rec_ctx);
                    let cache_path = cache_dir_clone.join(format!("{}.json", cache_key));

                    let should_fetch = match std::fs::metadata(&cache_path) {
                        Ok(meta) => meta
                            .modified()
                            .ok()
                            .and_then(|m| m.elapsed().ok())
                            .map(|e| e.as_secs() > 10)
                            .unwrap_or(true),
                        Err(_) => true,
                    };

                    if should_fetch {
                        std::thread::spawn(move || {
                            let manifest =
                                crate::system::recommend_provider_fetch::fetch_manifest_blocking(
                                    &clean_url_clone,
                                );
                            let _ =
                                crate::system::recommend_provider_fetch::fetch_recommend_blocking(
                                    &clean_url_clone,
                                    &manifest,
                                    &rec_ctx_clone,
                                    &cache_path,
                                );
                        });
                    }

                    let composite = (*self.recommender).clone().with_provider(reader);
                    return composite.recommend_panel(&rec_ctx).as_legacy_result();
                }
            }
        }

        self.recommender
            .recommend_panel(&rec_ctx)
            .as_legacy_result()
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
