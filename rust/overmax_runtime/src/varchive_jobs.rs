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

/// 업로드 후 V-Archive 캐시 갱신 실패 사유. 업로드 자체는 성공한 상태다.
#[derive(Clone, Debug, PartialEq)]
pub enum UploadCacheError {
    /// 단일 곡 API 조회는 됐으나 캐시 병합 실패.
    ApiOkMergeFailed(String),
    /// 단일 곡 API 조회 실패 후 폴백 기록 병합도 실패.
    ApiAndFallbackFailed { api: String, fallback: String },
    /// V-ID가 없어 폴백 기록을 병합했으나 실패.
    FallbackFailed(String),
}

/// 단일 패턴 업로드 결과. 사용자에게 보일 문구는 호출자가 만든다.
#[derive(Clone, Debug, PartialEq)]
pub enum UploadOutcome {
    AccountPathMissing,
    AccountParseFailed,
    /// V-Archive가 업로드를 거부함 (서버 메시지).
    Rejected(String),
    /// 업로드 성공. `cache`는 캐시 갱신 결과와 성공 시 Top-50 순위.
    Uploaded {
        updated: bool,
        cache: Result<Option<usize>, UploadCacheError>,
    },
}

/// `candidate`를 V-Archive에 업로드하고 캐시를 갱신한 뒤 `on_done`으로 결과를 넘긴다.
pub fn spawn_upload(
    candidate: SyncCandidate,
    account_path: String,
    v_id: String,
    steam: String,
    record_db: Arc<RecordDB>,
    on_done: impl FnOnce(UploadOutcome) + Send + 'static,
) {
    std::thread::spawn(move || {
        let path = std::path::Path::new(&account_path);
        if account_path.is_empty() || !path.exists() {
            return on_done(UploadOutcome::AccountPathMissing);
        }
        let Some(account) = varchive_api::parse_account_file(path) else {
            return on_done(UploadOutcome::AccountParseFailed);
        };
        let res = varchive_api::upload_score_blocking(
            &account,
            &candidate.song_name,
            candidate.button_mode,
            candidate.difficulty,
            candidate.overmax_rate,
            candidate.overmax_mc,
            &candidate.composer,
        );
        if !res.success {
            return on_done(UploadOutcome::Rejected(res.message));
        }
        let cache = sync_varchive_cache_after_upload(&record_db, &v_id, &steam, &candidate);
        on_done(UploadOutcome::Uploaded {
            updated: res.updated,
            cache,
        });
    });
}

/// 업로드 성공 후 V-Archive 캐시를 병합하고 Top-50 순위를 조회한다.
fn sync_varchive_cache_after_upload(
    record_db: &RecordDB,
    v_id: &str,
    steam: &str,
    candidate: &SyncCandidate,
) -> Result<Option<usize>, UploadCacheError> {
    let btn = overmax_data::config::varchive_account::button_num(candidate.button_mode.as_str());

    if !v_id.is_empty() {
        match varchive_api::fetch_single_song_records_blocking(v_id, btn, candidate.song_id) {
            Ok(data) => record_db
                .merge_varchive_fetched_records(steam, btn, &data, false)
                .map_err(UploadCacheError::ApiOkMergeFailed)?,
            Err(e) => {
                let payload = upload_fallback_payload(candidate);
                record_db
                    .merge_varchive_fetched_records(steam, btn, &payload, false)
                    .map_err(|ue| UploadCacheError::ApiAndFallbackFailed {
                        api: e,
                        fallback: ue,
                    })?
            }
        }
    } else {
        let payload = upload_fallback_payload(candidate);
        record_db
            .merge_varchive_fetched_records(steam, btn, &payload, false)
            .map_err(UploadCacheError::FallbackFailed)?
    };

    Ok(record_db
        .get_varchive_top50_rank(
            steam,
            candidate.button_mode,
            &candidate.song_id.to_string(),
            candidate.difficulty,
        )
        .ok()
        .flatten())
}

/// 오프라인/실패 폴백용 단일 곡 기록 페이로드.
fn upload_fallback_payload(candidate: &SyncCandidate) -> serde_json::Value {
    serde_json::json!({
        "records": [
            {
                "title": candidate.song_id,
                "pattern": candidate.difficulty,
                "score": candidate.overmax_rate,
                "maxCombo": candidate.overmax_mc,
                "updatedAt": ""
            }
        ]
    })
}
