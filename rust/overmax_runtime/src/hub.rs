//! Runtime hub: distributes each detection output to record storage and observers.
//!
//! Ordering invariants (docs/decisions/data_and_sync.md, 2026-10-28):
//! - the result-screen record snapshot is taken before the verified play is stored,
//!   so PB is judged against the record from before this play;
//! - `PlayVerified` is published regardless of whether the DB commit succeeded.
//!
//! The commit chain runs first in a fixed order; observers then read the result and
//! must not depend on each other's order.

use overmax_core::{GameSessionState, RecordValue, VerifiedPlayEvent};
use overmax_data::{RecordManager, RecordSource, VArchiveDB};
use std::sync::Arc;

/// Result of applying one detection output, handed to every observer.
pub struct RuntimeUpdate<'a> {
    pub state: &'a GameSessionState,
    pub event: Option<VerifiedPlayEvent>,
    /// The event improved on the record captured at result-screen entry.
    pub is_pb: bool,
    /// The event changed the stored record.
    pub record_updated: bool,
    /// Passed per update because the app swaps the song DB on cache updates.
    pub varchive_db: &'a VArchiveDB,
}

/// Read-only consumer of runtime updates (IPC, external tools, ...).
pub trait RuntimeObserver: Send {
    fn on_update(&mut self, update: &RuntimeUpdate<'_>);
}

/// Commit chain step 1: tracks the result-screen session and judges PB.
#[derive(Default)]
struct SessionTracker {
    initial_record: Option<RecordValue>,
}

impl SessionTracker {
    /// 결과창 진입 시 현재 패턴의 기존 기록을 한 번 저장하고, 결과창을 벗어나면 비운다.
    fn observe(&mut self, state: &GameSessionState, record_manager: &RecordManager) {
        if state.scene.is_result() {
            if let Some(ctx_val) = &state.context {
                if self.initial_record.is_none() {
                    let song_id = ctx_val.song_id;
                    let rate_map = record_manager.get_rate_map(&[song_id]);
                    if let Some(&(r, mc)) = rate_map.get(&(song_id, ctx_val.mode, ctx_val.diff)) {
                        self.initial_record = Some((r, mc));
                    } else {
                        self.initial_record = Some((0.0, false));
                    }
                }
            }
        } else {
            self.initial_record = None;
        }
    }

    /// PB 판정: 결과창 진입 시점의 기존 기록(initial_record) 대비 향상 여부.
    /// 스냅샷이 없으면(진입 직후 첫 이벤트 등) 보수적으로 false.
    fn is_pb(&self, event: &VerifiedPlayEvent) -> bool {
        self.initial_record
            .is_some_and(|(initial_rate, _)| event.rate > initial_rate)
    }
}

/// Commit chain step 2: stores the verified play. Returns whether the record changed.
fn commit_record(record_manager: &RecordManager, event: &VerifiedPlayEvent) -> bool {
    record_manager.handle_verified_play(event)
}

pub struct Runtime {
    record_manager: Arc<RecordManager>,
    session: SessionTracker,
    observers: Vec<Box<dyn RuntimeObserver>>,
}

impl Runtime {
    pub fn new(record_manager: Arc<RecordManager>) -> Self {
        Self {
            record_manager,
            session: SessionTracker::default(),
            observers: Vec::new(),
        }
    }

    pub fn with_observer(mut self, observer: Box<dyn RuntimeObserver>) -> Self {
        self.observers.push(observer);
        self
    }

    /// Record of the current pattern captured on result-screen entry.
    pub fn session_initial_record(&self) -> Option<RecordValue> {
        self.session.initial_record
    }

    /// Applies one detection output. Returns the event when it changed the stored record.
    pub fn on_detection(
        &mut self,
        state: &GameSessionState,
        event: Option<&VerifiedPlayEvent>,
        varchive_db: &VArchiveDB,
    ) -> Option<VerifiedPlayEvent> {
        self.session.observe(state, &self.record_manager);
        let is_pb = event.is_some_and(|event| self.session.is_pb(event));
        let record_updated = event.is_some_and(|event| commit_record(&self.record_manager, event));

        let update = RuntimeUpdate {
            state,
            event: event.copied(),
            is_pb,
            record_updated,
            varchive_db,
        };
        for observer in &mut self.observers {
            observer.on_update(&update);
        }
        update.event.filter(|_| record_updated)
    }
}

#[cfg(test)]
mod tests {
    use super::{Runtime, RuntimeObserver, RuntimeUpdate};
    use overmax_core::{
        Difficulty, GameSessionState, Mode, PlayContext, SceneType, VerifiedPlayEvent,
    };
    use overmax_data::{RecordDB, RecordManager, VArchiveDB};
    use std::sync::{Arc, Mutex};

    struct Recorder(Arc<Mutex<Vec<(bool, bool)>>>);

    impl RuntimeObserver for Recorder {
        fn on_update(&mut self, update: &RuntimeUpdate<'_>) {
            if update.event.is_some() {
                self.0
                    .lock()
                    .unwrap()
                    .push((update.is_pb, update.record_updated));
            }
        }
    }

    fn result_state(rate: f32) -> (GameSessionState, VerifiedPlayEvent) {
        let ctx = PlayContext {
            song_id: 77,
            mode: Mode::B4,
            diff: Difficulty::SC,
            rate,
            is_max_combo: false,
        };
        let state = GameSessionState {
            scene: SceneType::ResultFreestyle,
            context: Some(ctx),
            is_stable: true,
            is_fullscreen: true,
        };
        let event = VerifiedPlayEvent {
            song_id: 77,
            mode: Mode::B4,
            diff: Difficulty::SC,
            rate,
            is_max_combo: false,
            is_result_screen: true,
        };
        (state, event)
    }

    #[test]
    fn pb_is_judged_against_record_from_before_the_commit() {
        let dir = tempfile::tempdir().unwrap();
        let mut db = RecordDB::new(dir.path().join("record.db"), Some("765611"));
        assert!(db.initialize());
        let manager = Arc::new(RecordManager::new(Arc::new(db)));
        assert!(manager.upsert(77, Mode::B4, Difficulty::SC, 90.0, false, false));

        let seen = Arc::new(Mutex::new(Vec::new()));
        let mut runtime =
            Runtime::new(manager.clone()).with_observer(Box::new(Recorder(seen.clone())));
        let songs = VArchiveDB::new();

        let (state, event) = result_state(95.0);
        assert_eq!(
            runtime.on_detection(&state, Some(&event), &songs),
            Some(event)
        );
        assert_eq!(runtime.session_initial_record(), Some((90.0, false)));
        // 같은 결과창에서 다시 들어온 이벤트도 진입 시점 기록(90%) 기준으로 PB다.
        assert_eq!(runtime.on_detection(&state, Some(&event), &songs), None);

        assert_eq!(*seen.lock().unwrap(), vec![(true, true), (true, false)]);
    }
}
