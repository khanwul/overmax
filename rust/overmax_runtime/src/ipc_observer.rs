//! IPC 관찰자: 런타임 갱신을 SSE 이벤트와 IPC 상태 캐시로 내보낸다.

use crate::hub::{RuntimeObserver, RuntimeUpdate};
use crate::ipc_server::{update_latest_state, IpcEvent, IpcPublisher, SongMeta};
use overmax_core::{Difficulty, GameSessionState, Mode};
use overmax_data::VArchiveDB;

pub struct IpcObserver {
    publisher: IpcPublisher,
    last_scene_key: Option<String>,
    last_context_key: Option<(i32, Mode, Difficulty, u32, bool)>,
}

impl IpcObserver {
    pub fn new(publisher: IpcPublisher) -> Self {
        Self {
            publisher,
            last_scene_key: None,
            last_context_key: None,
        }
    }

    /// 감지 상태 → IPC 이벤트 변환 발행 (compare-and-publish 중복 억제 포함).
    /// `scene`/`context` 조합이 직전 발행과 동일하면 아무것도 보내지 않는다.
    fn publish_state_events(&mut self, state: &GameSessionState, varchive_db: &VArchiveDB) {
        let scene_key = format!("{:?}", state.scene);
        if self.last_scene_key.as_deref() != Some(scene_key.as_str()) {
            self.publisher.publish(IpcEvent::SceneDetected {
                scene: scene_key.clone(),
            });
            self.last_scene_key = Some(scene_key);
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
        if self.last_context_key.as_ref() != Some(&ctx_key) {
            let meta = song_meta_for(varchive_db, ctx.song_id, ctx.mode, ctx.diff);
            self.publisher.publish(IpcEvent::SongDetected {
                song_id: ctx.song_id,
                mode: ctx.mode.as_str().to_string(),
                diff: ctx.diff.as_str().to_string(),
                rate: ctx.rate,
                is_max_combo: ctx.is_max_combo,
                meta,
            });
            self.last_context_key = Some(ctx_key);
        }
    }
}

impl RuntimeObserver for IpcObserver {
    fn on_update(&mut self, update: &RuntimeUpdate<'_>) {
        // Publish current scene status; the IPC cache retains only stable song context.
        update_latest_state(update.state.clone());
        self.publish_state_events(update.state, update.varchive_db);

        // play_verified는 DB 커밋 성공 여부와 무관하게 확정 이벤트로 통지
        let Some(event) = update.event else {
            return;
        };
        let meta = song_meta_for(update.varchive_db, event.song_id, event.mode, event.diff);
        self.publisher.publish(IpcEvent::PlayVerified {
            song_id: event.song_id,
            mode: event.mode.as_str().to_string(),
            diff: event.diff.as_str().to_string(),
            rate: event.rate,
            is_max_combo: event.is_max_combo,
            is_pb: update.is_pb,
            meta,
        });
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
