//! V-Archive 백그라운드 작업 (동기화 후보 스캔, 기록 조회).
//!
//! UI 프레임워크에 의존하지 않도록 화면 갱신 요청(`Notify`)과 로그 출력(`LogSink`)은
//! 호출자가 주입한다.

use overmax_data::community::varchive_api;
use overmax_data::{build_candidates, RecordDB, SyncCandidate, VArchiveDB};
use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::Arc;

pub type Notify = Arc<dyn Fn() + Send + Sync>;
pub type LogSink = Arc<dyn Fn(String) + Send + Sync>;

/// 동기화 후보 목록을 백그라운드에서 만든다.
pub fn spawn_scan(
    songs_path: PathBuf,
    record_db: Arc<RecordDB>,
    steam: String,
    tx: Sender<Result<Vec<SyncCandidate>, String>>,
    notify: Notify,
) {
    std::thread::spawn(move || {
        let mut db = VArchiveDB::new();
        if let Err(e) = db.load_from_file(&songs_path) {
            let _ = tx.send(Err(format!("songs.json: {e}")));
            notify();
            return;
        }
        let list = build_candidates(&db, record_db.as_ref(), &steam);
        let _ = tx.send(Ok(list));
        notify();
    });
}

/// V-Archive 기록을 버튼별로 조회해 로컬 캐시에 병합한다. `button == 0`이면 전체 버튼.
pub fn spawn_fetch(
    steam_id: String,
    v_id: String,
    button: i32,
    record_db: Arc<RecordDB>,
    tx: Sender<(String, i32, Result<usize, String>)>,
    log: LogSink,
    notify: Notify,
) {
    std::thread::spawn(move || {
        let buttons = if button == 0 {
            vec![4, 5, 6, 8]
        } else {
            vec![button]
        };
        for b in buttons {
            log(format!("[VArchiveClient] 기록 요청 중: {} ({}B)", v_id, b));

            let since = record_db.get_latest_updated_at_from_db(&steam_id, b);
            if let Some(ref s) = since {
                log(format!("[VArchiveClient] 증분 조회 적용 (since={})", s));
            }

            match varchive_api::fetch_records_blocking(&v_id, b, since.as_deref()) {
                Ok(data) => {
                    let clear_first = since.is_none();
                    let save_res =
                        record_db.merge_varchive_fetched_records(&steam_id, b, &data, clear_first);

                    if let Err(e) = save_res {
                        log(format!("[VArchiveClient] 캐시 저장 실패: {}", e));
                        let _ = tx.send((v_id.clone(), b, Err(e)));
                    } else {
                        log(format!("[VArchiveClient] 캐시 저장 완료 ({}B)", b));
                        let _ = tx.send((v_id.clone(), b, Ok(1)));
                    }
                }
                Err(e) => {
                    let _ = tx.send((v_id.clone(), b, Err(e)));
                }
            }
        }
        notify();
    });
}
