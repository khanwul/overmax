//! Single `eframe` app: overlay + deferred debug / settings / sync viewports.

use eframe::egui::{self, Color32, Frame, Vec2, ViewportBuilder, ViewportId};
use overmax_core::GameSessionState;
use overmax_data::{
    build_candidates, load_base_settings, load_merged_settings, normalize_settings, upsert_varchive_cache_record,
    DataCompatibility, RecordDB, SyncCandidate, VArchiveDB,
};
use serde_json::Value;
use std::collections::VecDeque;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use crate::debug_ui;
use crate::overlay_ui;
use crate::probe_worker;
use crate::settings_ui;
use crate::sync_ui;
use crate::varchive_upload;

fn vp_debug() -> ViewportId {
    ViewportId::from_hash_of("overmax_debug_vp")
}
fn vp_settings() -> ViewportId {
    ViewportId::from_hash_of("overmax_settings_vp")
}
fn vp_sync() -> ViewportId {
    ViewportId::from_hash_of("overmax_sync_vp")
}

pub fn run_native_app() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: ViewportBuilder::default()
            .with_title("Overmax")
            .with_inner_size([overlay_ui::WIDTH, overlay_ui::HEIGHT])
            .with_resizable(false)
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top(),
        ..Default::default()
    };

    eframe::run_native(
        "Overmax",
        options,
        Box::new(|cc| {
            overlay_ui::install_korean_font(&cc.egui_ctx);
            NativeApp::new()
                .map(|app| Box::new(app) as Box<dyn eframe::App>)
                .map_err(|e| {
                    eprintln!("native app init: {e}");
                    Box::new(std::io::Error::new(std::io::ErrorKind::Other, e))
                        as Box<dyn std::error::Error + Send + Sync>
                })
        }),
    )
}

pub struct NativeApp {
    root: Arc<std::path::PathBuf>,
    defaults: Arc<Value>,
    base_settings: Arc<Mutex<Value>>,
    merged_settings: Arc<Mutex<Value>>,
    settings_draft: Arc<Mutex<Value>>,
    debug_open: Arc<AtomicBool>,
    settings_open: Arc<AtomicBool>,
    sync_open: Arc<AtomicBool>,
    scan_pending: Arc<AtomicBool>,
    log_lines: Arc<Mutex<VecDeque<String>>>,
    log_rx: Receiver<String>,
    session: GameSessionState,
    confidence: f32,
    sync_steam_id: Arc<Mutex<String>>,
    sync_status: Arc<Mutex<String>>,
    sync_candidates: Arc<Mutex<Vec<SyncCandidate>>>,
    sync_rx: Receiver<Result<Vec<SyncCandidate>, String>>,
    sync_tx: Sender<Result<Vec<SyncCandidate>, String>>,
    upload_req_rx: Receiver<usize>,
    upload_req_tx: Sender<usize>,
    upload_res_rx: Receiver<(usize, String, String)>,
    upload_res_tx: Sender<(usize, String, String)>,
    prev_settings_open: bool,
}

impl NativeApp {
    fn new() -> Result<Self, String> {
        let root = std::env::current_dir().map_err(|e| e.to_string())?;
        let root = Arc::new(root);
        let defaults: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../settings.json"
        )))
        .unwrap_or_else(|_| Value::Object(serde_json::Map::new()));
        let defaults = Arc::new(defaults);

        let base_settings = Arc::new(Mutex::new(load_base_settings(root.as_ref(), (*defaults).clone())));
        let mut merged = load_merged_settings(root.as_ref(), (*defaults).clone());
        normalize_settings(&mut merged);
        let merged_settings = Arc::new(Mutex::new(merged.clone()));
        let settings_draft = Arc::new(Mutex::new(merged));

        let (log_tx, log_rx) = mpsc::channel();
        probe_worker::spawn((*root).clone(), log_tx.clone());

        let compat = DataCompatibility::current();
        let mut record_db = RecordDB::new(root.join(compat.record_db), None);
        record_db.initialize();

        let mut varchive_db = VArchiveDB::new();
        let songs_path = root.join(compat.songs_json);
        if let Err(e) = varchive_db.load_from_file(&songs_path) {
            let _ = log_tx.send(format!("[VArchive] songs load failed: {e}"));
        }

        let steam0 = {
            let mg = merged_settings.lock().map_err(|_| "settings lock poisoned")?;
            first_steam_from_settings(mg.clone())
        };

        let (sync_tx, sync_rx) = mpsc::channel();
        let (upload_req_tx, upload_req_rx) = mpsc::channel();
        let (upload_res_tx, upload_res_rx) = mpsc::channel();

        Ok(Self {
            root,
            defaults,
            base_settings,
            merged_settings,
            settings_draft,
            debug_open: Arc::new(AtomicBool::new(false)),
            settings_open: Arc::new(AtomicBool::new(false)),
            sync_open: Arc::new(AtomicBool::new(false)),
            scan_pending: Arc::new(AtomicBool::new(false)),
            log_lines: Arc::new(Mutex::new(VecDeque::new())),
            log_rx,
            session: GameSessionState::detecting(),
            confidence: 0.0,
            sync_steam_id: Arc::new(Mutex::new(steam0)),
            sync_status: Arc::new(Mutex::new(String::new())),
            sync_candidates: Arc::new(Mutex::new(Vec::new())),
            sync_rx,
            sync_tx,
            upload_req_rx,
            upload_req_tx,
            upload_res_rx,
            upload_res_tx,
            prev_settings_open: false,
        })
    }

    fn max_log_lines(&self) -> usize {
        let Ok(m) = self.merged_settings.lock() else {
            return 500;
        };
        m.get("debug_window")
            .and_then(|d| d.get("max_lines"))
            .and_then(|v| v.as_u64())
            .unwrap_or(500) as usize
    }

    fn debug_title(&self) -> String {
        let Ok(m) = self.merged_settings.lock() else {
            return "Overmax Debug Log".into();
        };
        m.get("debug_window")
            .and_then(|d| d.get("title"))
            .and_then(|v| v.as_str())
            .unwrap_or("Overmax Debug Log")
            .to_string()
    }

    fn apply_overlay_visual(&self, ctx: &egui::Context) {
        let Ok(merged) = self.merged_settings.lock() else {
            return;
        };
        let opacity = merged
            .get("overlay")
            .and_then(|o| o.get("base_opacity"))
            .and_then(|v| v.as_f64())
            .unwrap_or(0.8) as f32;
        let scale = merged
            .get("overlay")
            .and_then(|o| o.get("scale"))
            .and_then(|v| v.as_f64())
            .unwrap_or(1.0) as f32;
        ctx.set_pixels_per_point(scale);
        ctx.style_mut(|s| {
            s.visuals.widgets.noninteractive.bg_fill =
                Color32::from_rgba_unmultiplied(18, 24, 38, (255.0 * opacity.clamp(0.1, 1.0)) as u8);
        });
    }

    fn drain_logs(&self) {
        let max = self.max_log_lines();
        debug_ui::drain_channel(&self.log_lines, &self.log_rx, max);
    }

    fn poll_scan_requests(&mut self) {
        if self.scan_pending.swap(false, Ordering::Relaxed) {
            if let Ok(mut s) = self.sync_status.lock() {
                *s = "스캔 중…".into();
            }
            self.spawn_scan();
        }
    }

    fn poll_upload_requests(&mut self) {
        while let Ok(idx) = self.upload_req_rx.try_recv() {
            let cand = self
                .sync_candidates
                .lock()
                .ok()
                .and_then(|g| g.get(idx).cloned());
            if let Some(c) = cand {
                self.spawn_upload(idx, c);
            }
        }
    }

    fn drain_sync_scan(&self) {
        while let Ok(res) = self.sync_rx.try_recv() {
            match res {
                Ok(list) => {
                    let n = list.len();
                    if let Ok(mut g) = self.sync_candidates.lock() {
                        *g = list;
                    }
                    if let Ok(mut s) = self.sync_status.lock() {
                        *s = format!("후보 {n}건");
                    }
                }
                Err(msg) => {
                    if let Ok(mut s) = self.sync_status.lock() {
                        *s = msg;
                    }
                }
            }
        }
    }

    fn drain_upload_results(&self) {
        while let Ok((idx, status, msg)) = self.upload_res_rx.try_recv() {
            if let Ok(mut list) = self.sync_candidates.lock() {
                if let Some(c) = list.get_mut(idx) {
                    c.upload_status = status;
                    c.upload_message = msg;
                }
            }
        }
    }

    fn spawn_scan(&self) {
        let steam = self.sync_steam_id.lock().map(|g| g.clone()).unwrap_or_default();
        let tx = self.sync_tx.clone();
        let root = self.root.clone();
        std::thread::spawn(move || {
            let compat = DataCompatibility::current();
            let songs_path = root.join(compat.songs_json);
            let mut db = VArchiveDB::new();
            if let Err(e) = db.load_from_file(&songs_path) {
                let _ = tx.send(Err(format!("songs.json: {e}")));
                return;
            }
            let mut rdb = RecordDB::new(root.join(compat.record_db), None);
            rdb.initialize();
            let cache_root = root.join("cache").join("varchive");
            let list = build_candidates(&db, &rdb, &steam, &cache_root);
            let _ = tx.send(Ok(list));
        });
    }

    fn spawn_upload(&self, index: usize, candidate: SyncCandidate) {
        let merged = match self.merged_settings.lock() {
            Ok(g) => g.clone(),
            Err(_) => return,
        };
        let steam = self.sync_steam_id.lock().map(|g| g.clone()).unwrap_or_default();
        let account_path = account_path_for_steam(&merged, &steam);
        let tx = self.upload_res_tx.clone();
        let root = self.root.clone();

        std::thread::spawn(move || {
            let path = Path::new(&account_path);
            if account_path.is_empty() || !path.exists() {
                let _ = tx.send((index, "error".into(), "account.txt 경로 없음".into()));
                return;
            }
            let Some(account) = varchive_upload::parse_account_file(path) else {
                let _ = tx.send((index, "error".into(), "account.txt 파싱 실패".into()));
                return;
            };
            let res = varchive_upload::upload_score_blocking(
                &account,
                &candidate.song_name,
                &candidate.button_mode,
                &candidate.difficulty,
                candidate.overmax_rate,
                candidate.overmax_mc,
                &candidate.composer,
            );
            if res.success {
                let btn = button_num(&candidate.button_mode);
                let cache_root = root.join("cache").join("varchive");
                if let Err(e) = upsert_varchive_cache_record(
                    &cache_root,
                    &steam,
                    btn,
                    candidate.song_id,
                    &candidate.difficulty,
                    candidate.overmax_rate,
                    candidate.overmax_mc,
                ) {
                    let _ = tx.send((index, "success".into(), format!("업로드 OK, 캐시 갱신 실패: {e}")));
                } else {
                    let _ = tx.send((index, "success".into(), "등록 완료".into()));
                }
            } else {
                let _ = tx.send((index, "error".into(), res.message));
            }
        });
    }

    fn show_debug_viewport(&self, ctx: &egui::Context) {
        if !self.debug_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.debug_open.clone();
        let lines = self.log_lines.clone();
        let title = self.debug_title();
        ctx.show_viewport_deferred(
            vp_debug(),
            ViewportBuilder::default()
                .with_title(&title)
                .with_inner_size([720.0, 420.0]),
            move |ctx, class| {
                debug_ui::render_debug(ctx, class, &title, &lines);
                debug_ui::close_if_requested(ctx, &open);
            },
        );
    }

    fn show_settings_viewport(&self, ctx: &egui::Context) {
        if !self.settings_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.settings_open.clone();
        let draft = self.settings_draft.clone();
        let root = self.root.clone();
        let defaults = self.defaults.clone();
        let base = self.base_settings.clone();
        let merged = self.merged_settings.clone();
        ctx.show_viewport_deferred(
            vp_settings(),
            ViewportBuilder::default()
                .with_title("Overmax 설정")
                .with_inner_size([440.0, 520.0]),
            move |ctx, class| {
                egui::TopBottomPanel::bottom("sett_actions").show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("저장").clicked() {
                            let base_g = base.lock().map(|g| g.clone()).unwrap_or_default();
                            let mut merged_g = merged.lock().map(|g| g.clone()).unwrap_or_default();
                            if let Ok(mut d) = draft.lock() {
                                let _ = settings_ui::save_settings_to_disk(
                                    root.as_ref(),
                                    defaults.as_ref(),
                                    &base_g,
                                    &mut *d,
                                    &mut merged_g,
                                );
                                if let Ok(mut m) = merged.lock() {
                                    *m = merged_g;
                                }
                            }
                        }
                        if ui.button("닫기").clicked() {
                            open.store(false, Ordering::Relaxed);
                        }
                    });
                });
                if let Ok(mut d) = draft.lock() {
                    settings_ui::render_settings_deferred(ctx, class, "설정", &mut *d);
                }
                settings_ui::close_if_requested(ctx, &open);
            },
        );
    }

    fn show_sync_viewport(&self, ctx: &egui::Context) {
        if !self.sync_open.load(Ordering::Relaxed) {
            return;
        }
        let open = self.sync_open.clone();
        let scan_pending = self.scan_pending.clone();
        let steam = self.sync_steam_id.clone();
        let status = self.sync_status.clone();
        let candidates = self.sync_candidates.clone();
        let upload_tx = self.upload_req_tx.clone();
        ctx.show_viewport_deferred(
            vp_sync(),
            ViewportBuilder::default()
                .with_title("V-Archive 동기화")
                .with_inner_size([520.0, 560.0]),
            move |ctx, class| {
                let list = candidates.lock().map(|g| g.clone()).unwrap_or_default();
                let mut steam_g = steam.lock().unwrap_or_else(|e| e.into_inner());
                let status_s = status.lock().map(|g| g.clone()).unwrap_or_default();
                sync_ui::render_sync(
                    ctx,
                    class,
                    &mut *steam_g,
                    &status_s,
                    &list,
                    || {
                        scan_pending.store(true, Ordering::Relaxed);
                    },
                    |i| {
                        let _ = upload_tx.send(i);
                    },
                );
                sync_ui::close_if_requested(ctx, &open);
            },
        );
    }
}

impl eframe::App for NativeApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let settings_on = self.settings_open.load(Ordering::Relaxed);
        if settings_on && !self.prev_settings_open {
            if let (Ok(m), Ok(mut d)) = (self.merged_settings.lock(), self.settings_draft.lock()) {
                *d = m.clone();
            }
        }
        self.prev_settings_open = settings_on;

        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        self.drain_logs();
        self.poll_scan_requests();
        self.poll_upload_requests();
        self.drain_sync_scan();
        self.drain_upload_results();
        self.apply_overlay_visual(ctx);

        self.show_debug_viewport(ctx);
        self.show_settings_viewport(ctx);
        self.show_sync_viewport(ctx);

        egui::CentralPanel::default()
            .frame(Frame::NONE.fill(Color32::TRANSPARENT))
            .show(ctx, |ui| {
                ui.set_min_size(Vec2::new(overlay_ui::WIDTH, overlay_ui::HEIGHT));
                overlay_ui::draw_overlay_panel(
                    ui,
                    &self.session,
                    self.confidence,
                    self.settings_open.clone(),
                    self.debug_open.clone(),
                    self.sync_open.clone(),
                );
            });
    }
}

fn first_steam_from_settings(settings: Value) -> String {
    let Some(Value::Object(map)) = settings.get("varchive").and_then(|v| v.get("user_map")) else {
        return String::new();
    };
    map.keys().next().cloned().unwrap_or_default()
}

fn account_path_for_steam(settings: &Value, steam: &str) -> String {
    settings
        .get("varchive")
        .and_then(|v| v.get("user_map"))
        .and_then(|m| m.get(steam))
        .and_then(|entry| {
            if let Some(s) = entry.as_str() {
                return Some(s.to_string());
            }
            entry
                .get("account_path")
                .and_then(|p| p.as_str())
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

fn button_num(mode: &str) -> i32 {
    match mode {
        "4B" => 4,
        "5B" => 5,
        "6B" => 6,
        "8B" => 8,
        _ => 4,
    }
}
