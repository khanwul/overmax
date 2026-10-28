//! V-Archive 업로드 판단: 업로드가 필요한지, 무엇을 올릴지.

use overmax_core::PlayContext;
use overmax_data::{RecordManager, SyncCandidate, VArchiveDB};

/// `ctx` 기록이 V-Archive 기록보다 나은지(Rate +0.01 이상 또는 MAX COMBO 신규 달성) 판단한다.
pub fn pattern_needs_upload(record_manager: &RecordManager, ctx: &PlayContext) -> bool {
    let song_id = ctx.song_id;
    let mode = ctx.mode;
    let diff = ctx.diff;

    let local = record_manager.get_local_record(song_id, mode, diff);
    let varchive = record_manager.get_varchive_cache_record(song_id, mode, diff);

    let (mut local_rate, mut local_mc) = local.unwrap_or((0.0, false));
    if ctx.rate > local_rate {
        local_rate = ctx.rate;
    }
    if ctx.is_max_combo {
        local_mc = true;
    }

    match (Some((local_rate, local_mc)), varchive) {
        (Some((l_rate, l_mc)), Some((v_rate, v_mc))) => {
            (l_rate - v_rate) >= 0.01 || (l_mc && !v_mc)
        }
        (Some((l_rate, _)), None) => l_rate > 0.0,
        _ => false,
    }
}

/// `session_ctx` 기록을 로컬 최고 기록과 합쳐 단일 패턴 업로드 후보를 만든다.
/// 합친 기록은 로컬 DB에도 반영한다. 곡 DB에 없는 곡이면 `None`.
pub fn prepare_pattern_upload(
    record_manager: &RecordManager,
    varchive_db: &VArchiveDB,
    session_ctx: &PlayContext,
) -> Option<SyncCandidate> {
    let song_id = session_ctx.song_id;
    let mode = session_ctx.mode;
    let diff = session_ctx.diff;

    let song = varchive_db.search_by_id(song_id)?;
    let local = record_manager.get_local_record(song_id, mode, diff);
    let varchive = record_manager.get_varchive_cache_record(song_id, mode, diff);

    let (mut overmax_rate, mut overmax_mc) = local.unwrap_or((0.0, false));
    if session_ctx.rate > overmax_rate {
        overmax_rate = session_ctx.rate;
    }
    if session_ctx.is_max_combo {
        overmax_mc = true;
    }

    if overmax_rate > 0.0 || overmax_mc {
        record_manager.upsert(song_id, mode, diff, overmax_rate, overmax_mc, true);
    }

    let (v_rate, v_mc) = match varchive {
        Some((r, mc)) => (Some(r), Some(mc)),
        None => (None, None),
    };

    let pattern_level = song
        .get_pattern(session_ctx.mode, session_ctx.diff)
        .and_then(|p| p.level);

    Some(SyncCandidate {
        song_id,
        song_name: song.name.to_string(),
        composer: song.composer.to_string(),
        dlc: song.dlc_code.to_string(),
        button_mode: session_ctx.mode,
        difficulty: session_ctx.diff,
        pattern_level,
        overmax_rate: overmax_rate as f64,
        overmax_mc,
        varchive_rate: v_rate.map(|r| r as f64),
        varchive_mc: v_mc,
        upload_status: String::new(),
        upload_message: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::pattern_needs_upload;
    use overmax_core::{Difficulty, Mode, PlayContext};
    use overmax_data::{RecordDB, RecordManager};
    use std::sync::Arc;

    fn ctx(rate: f32, is_max_combo: bool) -> PlayContext {
        PlayContext {
            song_id: 77,
            mode: Mode::B4,
            diff: Difficulty::SC,
            rate,
            is_max_combo,
        }
    }

    #[test]
    fn needs_upload_only_with_a_record_when_varchive_has_none() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = RecordDB::new(dir.path().join("record.db"), Some("765611"));
        assert!(db.initialize());
        let manager = RecordManager::new(Arc::new(db));

        assert!(!pattern_needs_upload(&manager, &ctx(0.0, false)));
        assert!(pattern_needs_upload(&manager, &ctx(95.5, false)));
    }
}
