//! Runtime hub: distributes each detection output to record storage and IPC.
//!
//! Ordering invariants (docs/decisions/data_and_sync.md, 2026-10-28):
//! - the result-screen record snapshot is taken before the verified play is stored,
//!   so PB is judged against the record from before this play;
//! - `PlayVerified` is published regardless of whether the DB commit succeeded.

use crate::ipc_server::{update_latest_state, IpcEvent, IpcPublisher, SongMeta};
use overmax_core::{Difficulty, GameSessionState, Mode, RecordValue, VerifiedPlayEvent};
use overmax_data::{RecordManager, RecordSource, VArchiveDB};
use std::sync::Arc;

pub struct Runtime {
    record_manager: Arc<RecordManager>,
    ipc_publisher: IpcPublisher,
    session_initial_record: Option<RecordValue>,
    last_ipc_scene_key: Option<String>,
    last_ipc_context_key: Option<(i32, Mode, Difficulty, u32, bool)>,
}

impl Runtime {
    pub fn new(record_manager: Arc<RecordManager>, ipc_publisher: IpcPublisher) -> Self {
        Self {
            record_manager,
            ipc_publisher,
            session_initial_record: None,
            last_ipc_scene_key: None,
            last_ipc_context_key: None,
        }
    }

    /// Record of the current pattern captured on result-screen entry.
    pub fn session_initial_record(&self) -> Option<RecordValue> {
        self.session_initial_record
    }

    /// Applies one detection output. Returns the event when it changed the stored record.
    /// `varchive_db` is passed per call because the app swaps it on cache updates.
    pub fn on_detection(
        &mut self,
        state: &GameSessionState,
        event: Option<&VerifiedPlayEvent>,
        varchive_db: &VArchiveDB,
    ) -> Option<VerifiedPlayEvent> {
        if state.scene.is_result() {
            if let Some(ctx_val) = &state.context {
                if self.session_initial_record.is_none() {
                    let song_id = ctx_val.song_id;
                    let rate_map = self.record_manager.get_rate_map(&[song_id]);
                    if let Some(&(r, mc)) = rate_map.get(&(song_id, ctx_val.mode, ctx_val.diff)) {
                        self.session_initial_record = Some((r, mc));
                    } else {
                        self.session_initial_record = Some((0.0, false));
                    }
                }
            }
        } else {
            self.session_initial_record = None;
        }

        // ── IPC 이벤트 발행 (관찰자 — 파이프라인/DB 경로 무변경) ──
        // Publish current scene status; the IPC cache retains only stable song context.
        update_latest_state(state.clone());
        self.publish_ipc_events(state, varchive_db);

        let event = event?;
        // PB 판정: 결과창 진입 시점의 기존 기록(session_initial_record) 대비 향상 여부.
        // 스냅샷이 없으면(진입 직후 첫 이벤트 등) 보수적으로 false.
        let is_pb = self
            .session_initial_record
            .is_some_and(|(initial_rate, _)| event.rate > initial_rate);
        let updated = self.record_manager.handle_verified_play(event);
        // play_verified는 DB 커밋 성공 여부와 무관하게 확정 이벤트로 통지
        let meta = song_meta_for(varchive_db, event.song_id, event.mode, event.diff);
        self.ipc_publisher.publish(IpcEvent::PlayVerified {
            song_id: event.song_id,
            mode: event.mode.as_str().to_string(),
            diff: event.diff.as_str().to_string(),
            rate: event.rate,
            is_max_combo: event.is_max_combo,
            is_pb,
            meta,
        });
        updated.then_some(*event)
    }

    /// 감지 상태 → IPC 이벤트 변환 발행 (compare-and-publish 중복 억제 포함).
    /// `scene`/`context` 조합이 직전 발행과 동일하면 아무것도 보내지 않는다.
    fn publish_ipc_events(&mut self, state: &GameSessionState, varchive_db: &VArchiveDB) {
        let scene_key = format!("{:?}", state.scene);
        if self.last_ipc_scene_key.as_deref() != Some(scene_key.as_str()) {
            self.ipc_publisher.publish(IpcEvent::SceneDetected {
                scene: scene_key.clone(),
            });
            self.last_ipc_scene_key = Some(scene_key);
        }

        if !state.is_stable {
            return;
        }
        let Some(ctx) = &state.context else {
            return;
        };
        let ctx_key = (
            ctx.song_id,
            ctx.mode,
            ctx.diff,
            ctx.rate.to_bits(),
            ctx.is_max_combo,
        );
        if self.last_ipc_context_key.as_ref() != Some(&ctx_key) {
            let meta = song_meta_for(varchive_db, ctx.song_id, ctx.mode, ctx.diff);
            self.ipc_publisher.publish(IpcEvent::SongDetected {
                song_id: ctx.song_id,
                mode: ctx.mode.as_str().to_string(),
                diff: ctx.diff.as_str().to_string(),
                rate: ctx.rate,
                is_max_combo: ctx.is_max_combo,
                meta,
            });
            self.last_ipc_context_key = Some(ctx_key);
        }
    }
}

/// 곡 메타 정보 조회 (오버레이가 이미 사용하는 VArchiveDB 값 전달 — 새 결합 없음).
/// 조회 실패 시 빈 meta (필드 생략) — 클라이언트는 song_id로 자체 해석 가능.
fn song_meta_for(varchive_db: &VArchiveDB, song_id: i32, mode: Mode, diff: Difficulty) -> SongMeta {
    let Some(song) = varchive_db.search_by_id(song_id) else {
        return Default::default();
    };
    let floor_name = song
        .get_pattern(mode, diff)
        .and_then(|p| p.floor_name.as_deref().map(String::from));
    SongMeta {
        title: Some(song.name.to_string()),
        floor_name,
    }
}
