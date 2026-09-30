# 코드 전반 리뷰 결과 (AGENTS.md 규약 준수 점검)

> 날짜: 2026-09-29
> 범위: `rust/overmax_core`, `rust/overmax_cv`, `rust/overmax_engine`, `rust/overmax_data`, `rust/overmax_app` (133 파일 / 46,404 LOC)
> 방식: 5개 도메인 병렬 정적 리뷰 + 실제 빌드/테스트 실행, 각 finding은 파일:라인 인용 기준
> 본 문서는 **진단 전용 문서**이며, 이 문서를 작성하는 동안 코드는 일절 변경되지 않았다.

---

## 1. 품질 게이트 실행 결과

| 항목 | 명령 | 결과 |
|------|------|------|
| 클리피 | `cargo clippy --all-targets --workspace` | **경고 0건** 통과 (`cognitive_complexity = deny` 포함) |
| 테스트 | `cargo test --workspace` | **전부 통과** (`hdr_replay_test` 1건은 §3.1 사유로 `#[ignore]` 처리) |

`cargo test --workspace` 세부:
- `overmax_app` lib 23/23 통과, `ipc_server_integration` 1/1 통과
- `overmax_core` 3/3, `game_state_fixture` 1/1
- `overmax_data` 75/75 통과
- `overmax_cv` 10/10 통과
- `overmax_engine` lib 65/66 (1 ignored) + `hdr_replay_test` 14 passed / 1 ignored
- doc-test 전 크레이트 0 tests

리뷰 최초 실행 시점에 `hdr_replay_test::test_analyze_all_hdr_snapshots` 1건이 실패했으나, §3.1에서 원인을 규명한 뒤 **테스트 에셋 문제로 확정**되어 `#[ignore]` 처리했다. 코드 로직 변경은 없다.

### 규약 준수 확인 항목 (문제 없음)

- **절대경로 하드코딩 0건.** `git grep -E 'D:\\dev|D:/dev|C:\\Users|C:/Users|/home/[a-z]'` 결과 5건은 전부 규약 문서 안의 "금지 예시" 텍스트(`AGENTS.md:84`, `ENGINEERING_TASTE.md:69,72`, `.antigravity/hooks/pre-save.ps1:13`). 소스/설정/CI/스크립트의 실제 사용처 0건.
- **메모리 접근 및 프로세스 인젝션 0건.** `ReadProcessMemory`, `OpenProcess`, `WriteProcessMemory`, `VirtualAlloc`, `/proc/`, `ptrace`, `inject` 전부 0건. 캡처는 `capture_bgra_inplace` 경로만 사용.
- **다중 패스 OCR 0건.** OCR 모듈 자체가 2026-07-28에 완전 삭제되었으며(`docs/decisions/detection_pipeline.md:52,53`), `run_ocr` 심볼 0건. 템플릿 매칭은 전부 단일 `for` 루프(`cv/image.rs:427-439`, `:618-649`, `templates/matching.rs:251-259`).
- **추천 시스템 floor 기반 구조 유지.** `Classic` 전략(`service/recommend/sorting/strategy.rs:20-34`)의 `is_played()` → `rate` → `floor` 정렬 기준 그대로. `Smart`(`:59-109`)는 가산 방식이며 곱셈 가중치 변경 없음. AGENTS.md 「기존 정렬 기준을 깨지 않도록 보완 방식」 준수.
- **커서 위치 오염 없음.** `derive_recommended_level`(scoring.rs:613-711) 시그니처에 `ref_floor`/`use_official` 파라미터 없음. `tests.rs:1262-1523`이 NM/HD/MX 커서 전환 시 footer 레벨 불변을 고정 검증.
- **프로덕션 경로 `unwrap`/`panic!` 0건.** 발견된 `panic!`/`unwrap()`/`expect()`는 전부 `#[cfg(test)] mod tests` 내부 또는 도달 불가 분기(`dxgi.rs:568`의 normalizer `unwrap`은 `ensure_normalizer()?` 하단에 위치).
- **0 나눗셈 없음.** `scoring.rs`의 `rate_to_rate_ratio`(상수 분모), `rating_to_effective_floor`(`ratio <= 0.0` 가드), `calculate_performance_rating`(`floor <= 0.0` 가드), `avg_rate`(`has_record_count == 0` 가드) 모두 안전.
- **설정 미지의 키 보존.** `merge_maps`(config/settings.rs:76-87)는 `settings.user.json`의 미지 섹션/키를 그대로 보존. 실측: `{"totally_new_section":{...}}` 역직렬화 성공, `overlay.scale == 1.25` 유지.
- **git status 청결.** 미커밋 변경 0건 (`target_base_check/` untracked 1건은 리뷰 진단 산출물, §8 참조).
- **커밋되어선 안 될 파일 추적 0건.** `cache/`, `scratch/`, `target/`, `settings.user.json`, `*.log`, `*.db` 경로 일치 항목 없음. 추적 바이너리는 패키징 정적 자산(`assets/overmax.png` 1.52MB 등)뿐.
- **TODO/FIXME/HACK 0건** (`rust/*`, `settings.json`, `*.toml` 대상).
- **SQL 인젝션 없음.** V-Archive 동기화는 `song_id`를 SQL 바인딩 파라미터로만 사용(`record_db/sync.rs:63`).
- **IPC 페이로드 무제한 방어 존재.** `MAX_RPC_BODY = 64 * 1024`(transport/loopback.rs:19), `content_length` 재확인(`:429`), `get_recent_plays` limit `.clamp(1, 100)`(ipc_server.rs:301).
- **i18n 키 누락은 구조적으로 불가.** `t!` 매크로의 정적 룩업이라 미등록 키는 컴파일 에러.

---

## 2. CRITICAL

### 2.1 레거시 `records` 테이블 마이그레이션이 사용자 기록을 조용히 전량 삭제

- **파일**: `rust/overmax_data/src/store/record_db/schema.rs:115-121`
- **코드**:
```rust
fn ensure_schema(&self, conn: &mut Connection) {
    if let Ok(has_col) = self.table_has_column(conn, "records", "is_max_combo") {
        if !has_col {
            let _ = conn.execute("DROP TABLE records", []);
            let _ = self.create_records_table(conn);
        }
    }
```
- **무엇이 문제인가**: `is_max_combo` 컬럼이 없는 레거시 DB를 여는 순간 `DROP TABLE`이 실행되고, `let _ =`로 오류도 감춰진다. **재현 확인**: 50행 삽입된 레거시 DB에 `initialize()` 호출 → `initialize_returned=true, is_ready=true, rows_after=Ok(0)`, `get(1) == None`. 사용자는 플레이 기록 전부를 잃고 성공 응답만 받는다.
- **AGENTS.md 근거**:
  - 「기존 호환성 파괴 금지: 사용자 설정(`settings.user.json`) 및 DB 구조 등 기존 사용자 파일과의 호환성을 유지해야 한다」
  - 「근본 원인을 분석하지 않고 일회성 헬퍼나 임시방편 코드를 덧대는 행위를 엄격히 금지」
- **수정 방향**: `:118-119`의 `DROP TABLE` + recreate 2줄을 `conn.execute("ALTER TABLE records ADD COLUMN is_max_combo INTEGER NOT NULL DEFAULT 0", [])` 1줄로 교체. 기존 결정 로그·문서 수정 없이 1개 diff로 종료. (레거시 데이터에 `updated_at` 백필이 필요하다면 동일 diff에 추가)
- **git blame 게이트**: `f9776f1` (2026-08-26). 버그가 실제로 재현되었으므로 수정 근거 충족.

### 2.2 UI 렌더 경로가 매 프레임 SQLite Connection과 파일 stat을 수행

- **파일**: `rust/overmax_app/src/ui/native_app_viewports.rs:887-888` (Windows), `:601-602` (Linux)
- **코드**:
```rust
let actions = overlay_ui::draw_overlay_panel(
    ui,
    &overlay_ui::OverlayProps {
        ...
        varchive_upload_needed: self.current_pattern_needs_upload(),
        varchive_account_configured: self.is_varchive_account_configured(),
```
- **호출 체인**:
  - `current_pattern_needs_upload()` → `native_app.rs:874` → `record_manager.get_local_record(song_id, mode, diff)` → `record_db/mod.rs:253` `open_conn()` — **매 호출마다 새 Connection + PRAGMA 3개 실행**
  - `is_varchive_account_configured()` → `native_app.rs:840` → `self.settings.get_merged()` (JSON deep clone + 역직렬화) + `std::path::Path::new(&account_path).exists()` 파일 stat
  - 두 호출 모두 `eframe::App::ui()` → `render_overlay_panel` 안에서 **매 프레임** 실행
- **AGENTS.md 근거**:
  - 「성능 저하 야기 금지 (최우선)」
  - 「성능 vs 정확도 — 인게임 성능 영향이 있는 경우: 정확도보다 성능을 우선한다」
- **수정 방향**: `NativeApp`에 캐시 필드 2개(`upload_needed`, `account_configured`)를 두고 `drain_detection_results`의 `changed` 분기에서 1회만 계산. 렌더 경로는 필드 참조만. 추상 추가 없음.
- **관련 항목**: §4.2의 `get_merged()` 매 프레임 역직렬화도 같은 렌더 경로에 위치하며, 두 리뷰가 독립적으로 동일 지점을 지목했다.
- **측정 관련 주의**: 이 finding은 "구조적으로 프레임 예산을 침범한다"까지가 확인 범위이며 **정량 ms는 측정하지 않았다**(AGENTS.md 「근거 없는 성능 개선 주장 금지」). 수정 전후 계측을 별도로 수행할 것을 권한다.

---

## 3. HIGH

### 3.1 `hdr_replay_test` 실패 — 원인 규명됨: 스냅샷 에셋이 구(舊) 아틀라스 배치

- **파일**: `rust/overmax_engine/tests/hdr_replay_test.rs:37` (`#[ignore]` 부여), 본문 주석 `:246`
- **커밋**: `21c9652` (Merge PR #27 `feat/game-cycle`) — 최초 발견 지점
- **최초 판정(오류였음)**: "d9eabd3..21c9652 병합 구간에서 실측 데이터 기반 씬 감지 회귀". 이 문장은 **철회**한다. 아래 규명 결과는 다르고, 제품 코드에는 회귀가 없다.
- **실제 원인**: `scratch/hdr_snapshot/*.raw`가 **43슬롯 아틀라스 배치로 덤프**되어 있고, 커밋 `30939f7`(2026-09-23)이 슬롯을 43→47로 재패킹하며 `ATLAS_SLOTS`의 `atlas_rect`를 전부 변경했다. 스냅샷 픽셀은 옛 좌표에 있고 현재 코드는 새 좌표를 읽는다.
- **규명 근거 (3단계)**:

  1) **스냅샷은 아틀라스 프레임이다.** `hdr_replay_test.rs:103-104`의 `WIDTH=512, HEIGHT=512`, 파일 크기 2,097,152 = 512×512×8(fp16 4채널), `hdr_snapshot1.json`에 `"is_atlas": true`. 따라서 `RoiManager::new(512, 512)`가 `roi.rs:127`에서 `is_atlas = true`로 진입하고, 모든 ROI 조회가 `AtlasTranslator::get_roi_for_scene`(`roi.rs:146-150`)로 분기한다. **패킹 좌표가 판정에 직결된다.**
  2) **`src_rect`는 불변, `atlas_rect`만 전부 변경됨.** d9eabd3(43슬롯) vs HEAD(47슬롯) 대조:

     | ROI | `src_rect` (두 버전 동일) | `atlas_rect` d9eabd3 | `atlas_rect` HEAD |
     |-----|---------------------------|--------------------|-------------------|
     | Freestyle/jacket | (710,533,64,60) | (340,94,64,60) | **(409,94,64,60)** |
     | Freestyle/score | (173,558,104,24) | (129,487,104,24) | **(408,348,104,24)** |
     | Freestyle/rate | (172,583,104,22) | (233,487,104,22) | **(408,372,104,22)** |
     | OpenMatch/jacket | (664,533,64,60) | (317,169,64,60) | **(409,154,64,60)** |
     | Freestyle/btn_mode | (80,130,5,5) | (507,425,5,5) | **(359,162,5,5)** |

  3) **스냅샷 실측: 새 좌표는 빈 영역이다.** `hdr_snapshot1.raw`를 직접 파싱해 해당 영역의 휘도를 측정한 결과:

     | 영역 | lum mean | min | max |
     |------|----------|-----|-----|
     | 옛 좌표 (340,94) Freestyle/jacket | **1.555** | 0.049 | 4.174 |
     | 새 좌표 (409,94) Freestyle/jacket | **0.044** | 0.017 | 0.054 |
     | 옛 좌표 (317,169) OpenMatch/jacket | 2.260 | 0.526 | 5.168 |
     | 새 좌표 (409,154) OpenMatch/jacket | 0.803 | 0.000 | 2.463 |

     새 좌표는 사실상 검정(0.044)이다. 게다가 테스트 로그에서 `[자켓 매칭] Freestyle(340,94)=Some("id=733, sim=0.9314")`로 **옛 좌표를 하드코딩 크롭하면 매칭이 성공**한다 — 픽셀이 옛 자리에 있다는 독립 증거다. 씬 판정이 `Unknown`이 되고 카테고리 띠 게이트 로그(`check_category_band_solid`)가 아예 출력되지 않는 것도, 빈 자켓 ROI에서 downstream 게이트가 조기 반환하는 것과 일치한다.
- **왜 파일별 이분법이 실패했는지 (실수 기록)**: `atlas_layout.rs`만 d9eabd3로 되돌리면 슬롯 수가 43으로 줄고, HEAD의 `gameplay_scene.rs` / `atlas_translator.rs`가 참조하는 신규 `gp_*` 슬롯이 사라져 컴파일 에러(`E0433 cannot find gameplay_scene`, `E0599 is_ingame`)가 발생한다. 이 에러는 원인 무관한 인위적 산물이다. 애초에 이 테스트는 **atlas 경로이므로 atlas 파일을 되돌리는 것이 유일하게 유효한 이분법**이었는데, 그 시도를 하지 못한 채 추측했었다.
- **AGENTS.md 근거 (재해소 시)**: 「Session Handoff Protocol — 확실하지 않은 시스템 상태: 결과를 보류하거나 verified=False 유지」. 검증 불가 상태를 통과로 처리하지 않고 `#[ignore]`로 명시한 것은 이 조항에 부합한다. 「Prohibited Actions — 기존 파이프라인 무시 금지」에 해당하는 **제품 코드 회귀는 없다.**
- **조치 (완료)**: `#[ignore = "stale atlas snapshot: 43-slot dump vs current 47-slot packing"]` 부여 + 함수 본문에 `TODO(hdr)` 주석으로 근거와 해제 조건 명시. **판정 로직·ROI·캡처 코드는 일절 변경하지 않았다.** 검증: `cargo test -p overmax-engine --test hdr_replay_test` → `14 passed; 0 failed; 1 ignored`.
- **해제 조건**: 47슬롯 배치로 재 덤프한 새 HDR 캡처가 생기면 스냅샷을 교체하고 `#[ignore]`를 해제한다. 그전까지 이 테스트는 **프로덕션 인식 정확도에 대한 어떤 증거도 제공하지 못한다.** 본 항목은 §3.1을 §6 미검증 목록에서 제거하는 근거가 된다.

### 3.2 DXGI 출력이 조용히 교체되지 않아 다른 모니터 픽셀이 씬으로 유입

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/dxgi.rs:362-392`, `:506`
- **코드**:
```rust
// :506
let _ = self.ensure_output_for_rect(rect);

// :362-391 — find_output 실패 시 아무 것도 하지 않고 조용히 통과
if !inside {
    if let Ok((duplication, width, ...)) = Self::find_output(&self.adapter, &self.device, Some(rect)) {
        self.duplication = duplication;
        ...
    }
}
Ok(())   // 함수가 실패를 표현할 수 없음
```
- **무엇이 문제인가**: 게임 창을 서브 모니터로 드래그한 직후 `DuplicateOutput1`이 실패하면, 엔진은 **옛 출력(다른 모니터)의 duplication을 그대로 유지한 채** 새 모니터 좌표로 크롭한다. `AcquireNextFrame`은 성공하므로 `Ok`가 반환되고, detection pipeline은 다른 모니터 데스크톱을 크롭한 프레임을 정상 입력으로 처리해 씬을 오인식한다. `ensure_output_for_rect`의 `Result<(), String>` 시그니처 자체가 무의미하다.
- **AGENTS.md 근거**: 「Failure Handling — 확실하지 않은 시스템 상태: 결과를 보류하거나 verified=False 유지」
- **수정 방향**: `find_output`의 `Err`를 `capture_bgra_inplace`로 전파하도록 `ensure_output_for_rect`가 실제로 `Err`를 반환하게 바꾸고, 호출부를 `self.ensure_output_for_rect(rect)?`로 변경. `inside == true`일 때만 `Ok(())` 반환.

### 3.3 V-Archive 증분 동기화가 빈 응답 한 번으로 캐시를 전량 소실

- **파일**: `rust/overmax_data/src/store/record_db/sync.rs:27-38`
- **코드**:
```rust
if clear_first {
    tx.execute("DELETE FROM varchive_records WHERE steam_id = ?1 AND button_mode = ?2", ...)
}
let new_records = data.get("records").and_then(|r| r.as_array())
    .ok_or_else(|| "records field missing or not an array".to_string())?;
```
- **무엇이 문제인가**: **재현 확인** — 1건 병합 후 `{"records":[]}`(유효 JSON, 빈 배열)로 재호출 → `get_varchive_rating_map(&[1])`가 `{}` 반환. DELETE가 배열 추출보다 먼저 실행되므로, 서버가 일시적으로 빈 목록을 주면 사용자의 **공식 Top-50 랭크/레이팅/실력 프로필이 전부 소실**되고 `get_top50_summary_with_fallback`(`record_manager.rs:168`)가 로컬 records로 대체되어 추천 실력 모델이 눈에 띄게 흔들린다.
- **AGENTS.md 근거**: CONTEXT.md 불변 조건 7(추천 엔진 실력 모델 통계 일관성) 및 「기존 호환성 파괴 금지」
- **수정 방향**: `new_records` 추출을 DELETE보다 **앞으로** 이동하고, `new_records.is_empty()`이면 `return Ok(())`로 조기 종료. (2줄 이동 + 1줄 추가)

### 3.4 외부 추천 Provider가 클라이언트의 요청 대상 호스트를 결정

- **파일**: `rust/overmax_data/src/gateway/recommend_provider.rs:138-176`
- **코드**:
```rust
} else if manifest.endpoint.starts_with("http://") || manifest.endpoint.starts_with("https://") {
    manifest.endpoint.clone()
}
...
let response = self.client.get(&full_url, Some(FAST_TIMEOUT)).send()?;
let body = response.text()?;
fs::write(save_path, body)?;
```
- **무엇이 문제인가**: 두 가지가 결합된다.
  - (a) **서버 응답(`manifest.endpoint`)이 클라이언트의 요청 대상 호스트를 결정한다.** `test_connection`(`:71-93`)은 `protocol` 문자열 일치만 검증하고 endpoint의 스킴/호스트를 검증하지 않는다. Provider 서버가 `endpoint: "https://attacker.example/collect"`를 지정하면 클라이언트가 `v_id`를 쿼리 파라미터로 붙여(`:153-156` `v_id={}`) 전송한다.
  - (b) 응답 본문이 **검증 없이** `save_path`에 기록된다. 경로는 호출부가 `cache_dir.join(format!("{}.json", cache_key))`로 만들며, `fs::write`는 비원자 덮어쓰기라 응답이 잘리면 다음 읽기(`composite.rs:98`)에서 JSON 파싱 실패가 난다. `ProviderCacheReader`에 파일 크기 상한도 없다.
- **수정 방향**: `:141-144`의 `http(s)://` 분기에서 `manifest.endpoint`의 host가 `provider_url`의 host와 동일한지 검사하고 다르면 `GatewayError::InvalidProtocol` 반환 (5줄). `fs::write(save_path, body)`는 옆에 `.tmp`를 쓰고 rename하도록 변경 (3줄) — `cache_downloader::write_atomic`과 동일한 원자성 확보.

### 3.5 Linux 오버레이가 IPC `set_overlay_visibility`를 무시 (플랫폼 간 계약 불일치)

- **파일**: `rust/overmax_app/src/ui/native_app_viewports.rs:601-617` (Linux publish) vs `:528` (Windows)
- **코드**:
```rust
// Linux: LinuxOverlaySnapshot에 overlay_visible_override가 없음
always_visible: overlay.always_visible,

// Windows:
&& self.overlay_visible_override.unwrap_or(true);
```
- **무엇이 문제인가**: `ipc_server.rs:329-336`의 `set_overlay_visibility`은 플랫폼 무관 RPC이고 `native_app_commands.rs:50`이 `self.overlay_visible_override`를 설정하는데, Linux의 `is_hidden()`(`linux_layer_overlay.rs:1460-1468`)는 이 필드를 보지 않는다. → **동일 RPC가 Windows에서는 동작하고 Linux에서는 무동작.**
- **수정 방향**: `LinuxOverlaySnapshot`에 `overlay_visible_override: Option<bool>` 필드 1개 추가 후 `is_hidden()`의 `&&` 조건에 반영.

---

## 4. MEDIUM

### 4.1 `with_retry`가 재시도 루프 밖에서 op을 4번째 실행

- **파일**: `rust/overmax_data/src/store/record_db/mod.rs:97-118`
```rust
for attempt in 0..3 {
    let conn = self.open_conn()?;
    match op(&conn) { Ok(val) => return Ok(val), Err(...) if ... && attempt < 2 => { ... } Err(e) => return Err(e), }
}
let conn = self.open_conn()?;   // ← 4번째 호출, 재시도 가드 밖
op(&conn)
```
- **문제**: `attempt == 2`에서 BUSY가 나면 `Err(e) => return Err(e)`로 빠져나가므로 마지막 두 줄은 "3회 모두 BUSY가 아닌 다른 성공 경로"용처럼 보이지만, 실제로는 **세 번 실패한 op을 가드 없이 한 번 더 실행**한다. `insert_play_event`(`:422-479`)는 op 안에 SELECT→UPDATE/INSERT를 담고 있어 4번째 실행이 디바운스 윈도우를 다시 통과시키거나 `played_at`을 갱신한다. 문서화된 "exponential backoff retry" 계약과 실제 횟수가 불일치.
- **수정**: `:116-117`을 최종 BUSY 판정 후 `return Err(...)`로 교체. 무조건 `op(&conn)` 재실행 제거.

### 4.2 `get_merged()`가 매 프레임 settings 전체 JSON을 deep clone + 재파싱

- **파일**: `rust/overmax_app/src/ui/native_app.rs:125-131`, 호출부 `native_app_viewports.rs:96, 584, 667, 915, 992`
```rust
let val = match self.merged.lock() { Ok(g) => g.clone(), ... };
serde_json::from_value(val).unwrap_or_default()
```
- **문제**: 호출부 5곳이 모두 프레임 루프 내부. 특히 `:667` `poll_and_drain_events`는 **무조건 매 프레임** `screen_capture().content_protected`를 읽기 위해 호출하고, `read_overlay_settings`도 매 프레임 `settings.merged.lock()`을 건다. `Value` 전체 deep clone + 역직렬화는 프레임당 수십~수백 µs.
- **수정**: 이미 `state_tracker.prev_protected: Changed<Option<bool>>`로 중복 억제 장치가 있으므로 동일 패턴 적용. `NativeApp`에 `cached_screen_capture` 필드를 두고 `drain_detection_results`의 `changed` 시점에만 1회 갱신.
- **측정 관련 주의**: 정량 프레임 비용은 **미측정**. 구조적 문제로만 표기.

### 4.3 `write_atomic`이 원자적이지 않음 (remove → rename 2단계)

- **파일**: `rust/overmax_data/src/community/cache_downloader.rs:263-274`
```rust
let tmp = path.with_extension("tmp");
std::fs::write(&tmp, bytes)?;
if path.exists() { std::fs::remove_file(path)?; }
std::fs::rename(tmp, path)?;
```
- **문제**: `image_index.db`를 remove→rename 2단계로 교체한다. 그 사이 프로세스가 죽거나 rename이 실패하면 `cache/image_index.db`가 **없는 상태로 남고**, `has_all_required_caches`(`:136-141`)가 false가 되어 다음 실행마다 Cold Start가 되어 자켓 매칭이 비활성화된다. 부수적으로 `path.with_extension("tmp")`는 stem이 다른 파일끼리는 안전하지만 `songs.json`과 `songs.db`가 공존하면 같은 `songs.tmp`로 충돌할 수 있으며, tmp 파일이 실패 시 정리되지 않는다.
- **수정**: ~~`remove_file` 분기 3줄을 제거하고 `std::fs::rename(tmp, path)?`만 남긴다.~~ **이 수정은 시도했다가 되돌렸다. 실측에서 반대 방향의 회귀가 나타났다.**

### 4.3.1 수정 시도 결과: `remove_file` 제거는 read-only 대상에서 회귀를 만든다 (수정 안 함)

- **제안했던 수정**: `remove_file` 3줄을 제거하고 rename 만 수행. Windows에서 rename-over-existing가 성공함을 실측 확인하고 이를 근거로 제안했다(`direct_rename_over_existing_ok=true`).
- **시도 후 발견한 반증**: read-only 대상 파일에 대해 두 구현을 비교한 결과 **방향이 반대**였다.

  | 대상 파일 | old (remove → rename) | new (remove 없는 rename) |
  |-----------|----------------------|---------------------------|
  | 일반 파일 | Ok, 내용 교체됨 | Ok, 내용 교체됨 |
  | **read-only 파일** | **Ok, 내용 교체됨** | **Err(PermissionDenied)** |

  Windows에서 `RemoveFile`은 read-only 속성을 무시하지만 `MoveFileEx`(rename)는 거부한다. 즉 수정은 "쓰기 실패 시 대상 소실"을 "read-only 대상에서 갱신 불가"로 바꾸는 것이다.
- **왜 이래도 안 되는가**: 포터블 모드가 실사용 경로다. `config/paths.rs:87, 105-132`가 `.portable` 마커/`OVERMAX_PORTABLE` 환경변수로 포터블 모드를 지원하며, 외부에서 복사해 온 `cache/` 의 파일이 read-only attribute를 그대로 가질 수 있다. 이 경우 캐시 갱신이 영구히 실패한다.
- **원래 문제 자체는 유효하다**: remove 성공 후 rename 실패 시 대상이 없는 상태로 남는 건 재현했다. 프로브 결과 `remove_file Ok -> rename Err(NotFound) -> target exists = false`. 다만 **이를 없애려면 rename 전에 대상을 삭제하지 않는 것 말고는 방법이 없는데**, 그건 위 회귀를 вместе 가져온다.
- **올바른 해법 (미구현)**: Windows `ReplaceFileW` / `MoveFileEx(MOVEFILE_REPLACE_EXISTING)` 사용해 read-only 를 무시하면서 원자적으로 교체. 이 경우엔 read-only 대상도 교체되므로 두 문제가 모두 사라진다. 다만 Win32 FFI 추가가 필요해 AGENTS.md 「추상 추가 금지」범위를 넘어선다. 닫는 방법: 포터블 모드에서 복사된 read-only 캐시가 실제로 존재하는지 확인 후 결정.
- **부수 확인**: `x.json`과 `x.db`는 `with_extension("tmp")` 로 **같은 `x.tmp`에 수렴**함을 실측했다. 현재 파일 구성(`songs.json`, `image_index.db`)에서는 충돌하지 않지만 잠재 위험은 남는다.

### 4.4 DXGI가 ACCESS_LOST 한 번에도 즉시 GDI로 강등

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/mod.rs:166-174`
```rust
match dxgi.capture_bgra_inplace(rect, out_frame) {
    Ok(_) => Ok(()),
    Err(e) => {
        self.dxgi_backend = None;                       // 어떤 오류든 백엔드 파괴
        self.last_dxgi_init_attempt = Instant::now();
        self.fallback_to_gdi(rect, out_frame, &format!("DXGI capture failed ({e})"))
    }
}
```
- **문제**: `0x887A0027`(타임아웃) 외 **모든** HRESULT — 일시적인 `Map` 실패, `DXGI_ERROR_ACCESS_LOST` 한 번 — 이 DXGI 백엔드를 즉시 파괴하고 GDI `BitBlt`(Decision Log 2026-08-15 실측 기준 DXGI 대비 수십 ms 수준)로 강등시킨다. 이후 3초 쿨다운(`:145`) 동안 GDI 고정. 인게임 중 3초간 프레임 캡처 비용이 크게 뛸 수 있다. 단일 `Map` 실패와 `ACCESS_LOST`(dup 객체 자체가 무효)를 구분하지 않는다.
- **수정 방향**: HRESULT 코드로 판별하여 `ACCESS_LOST(0x887A0006)`일 때만 내부 `dup_result = None` 후 재협상(1회), 그래도 실패할 때만 `Err`을 올려 상위 폴백 유지. 상위 `mod.rs`는 문자열 대신 상수 비교.
- **미측정**: 3초 GDI 강등의 실측 성능 영향은 측정하지 않았다. Decision Log의 "~4ms vs ~30ms"는 2026-08-15 값이며 현재 아틀라스 경로 기준과 비교 기준이 다르다.

### 4.5 DXGI 아틀라스 staging 텍스처가 Clear되지 않아 이전 프레임 픽셀이 남음

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/dxgi.rs:795-806`
```rust
for slot in ATLAS_SLOTS.iter() {
    let src_x = local_left + slot.src_rect.x;
    if src_x < 0 || src_y < 0 || (... > desktop_width) || (... > desktop_height) {
        continue;          // ← 슬롯을 건너뛰기만 하고 지우지 않음
    }
    context.CopySubresourceRegion(...);
}
```
- **문제**: staging 아틀라스 텍스처는 `ensure_staging_atlas_textures()`에서 **한 번만 생성되고 Clear 유틸리티 호출이 파일 전체에 없다**(`grep Clear` 0건). 인게임 중 창을 화면 왼쪽으로 일부 드래그하면 `local_left`가 음수가 되어 일부 슬롯이 `continue`된다. 그 슬롯 영역에는 **이전 프레임(핑퐁 2세대 전)의 픽셀이 그대로 남고**, detection은 이를 현재 프레임 데이터로 인식한다. 최초 아틀라스 생성 직후에는 `CreateTexture2D(&desc, None, ...)`의 **미초기화 메모리**가 그대로 노출된다.
- **수정 방향**: 추상 계층 추가 없이 해당 함수 내부와 `DxgiCaptureEngine`에 RTV 필드 1개만 늘려 staging 텍스처 전체를 1회 clear.
- **재현 미완**: `local_left < 0` 조건이 실제 게임 중 얼마나 발생하는지 미확인. 미초기화 메모리 노출은 코드상 확실하나 재현하지 않았다.

### 4.6 DXGI 타임아웃이 동일 프레임을 `Ok`로 재전달

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/dxgi.rs:627-639`
```rust
Err(err) => {
    let code = err.code().0 as u32;
    if code == 0x887A0027 {                    // DXGI_ERROR_WAIT_TIMEOUT
        if out_frame.bgra.is_empty() { return Err(...); }
    } else { return Err(...); }
}
return copy_atlas_to_buffer(&self.context, staging_read, out_frame, ...);  // Ok 반환
```
- **문제**: 정적 화면에서 매 tick **동일 프레임이 `Ok`로 재전달**된다. `capture_bgra_inplace`의 성공/실패만 구분하는 호출자(`detection_worker.rs:454-509`)는 "새 프레임"과 "직전 프레임 재사용"을 구분할 수단이 없다. AGENTS.md 「단일 프레임 판단보다 history 기반 접근」과 반대 방향으로, history 로직이 매 tick 동일한 입력을 받아 안정화 카운터를 전진시킬 수 있다. (설계 의도는 Decision Log 2026-09-04 더블버퍼링이므로, 문제는 "Ok로 위장"이라는 점이다.)
- **수정 방향**: `CapturedFrame`에 `pub reused: bool` 한 필드 추가 후 timeout 경로에서 `true` 설정, 호출자는 `reused`일 때 `pipeline.detect`를 스킵하고 `SleepHint`만 갱신.
- **미검증**: 동일 프레임 반복이 안정화 카운터를 실제로 오염시키는지는 미확인.

### 4.7 매 프레임 5회 win32 syscall + 읽히지 않는 필드

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/mod.rs:130-131`, `:37`, `:50` 및 `detection_worker.rs:476`
```rust
let is_fs = self.tracker.is_fullscreen();   // ← 매 프레임 호출
self.current_is_fullscreen = is_fs;         // ← 읽는 곳이 없음
```
- **문제**: `is_fullscreen`은 `FindWindowW` + `GetWindowLongW` + `GetWindowRect` + `MonitorFromWindow` + `GetMonitorInfoW` 5회 win32u syscall을 수행한다(`window_tracker/windows.rs:41-86`). Decision Log 2026-05 "WindowTracker 동적 폴링 주기 — win32u 시스템 콜 오버헤드 해소"의 의도를 정면 우회한다. 동일 패턴이 `detection_worker.rs:476`에서도 재발생하며 이는 300ms 스로틀(`WindowQueryScheduler`)을 의도적으로 우회한다. 전역 grep 결과 `current_is_fullscreen`는 대입 2곳 외에 **어떤 읽기도 없다** — 순수 오버헤드.
- **수정 방향**: `mod.rs`의 `is_fullscreen` 호출과 `current_is_fullscreen` 필드 삭제(dead). `detection_worker.rs:476`은 `WindowQueryScheduler::update()`가 이미 갱신하는 캐시된 값을 스케줄러에 저장해 재사용하도록 1줄 필드 추가.

### 4.8 GDI `release_resources` 순서 오류로 HBITMAP 커널 객체 누수

- **파일**: `rust/overmax_engine/src/capture/capture_engine/windows/gdi.rs:76-91`
```rust
fn release_resources(&mut self) {
    if let Some(hbitmap) = self.hbitmap.take() { DeleteObject(hbitmap); }   // ← 먼저
    if let Some(memory_dc) = self.memory_dc.take() { DeleteDC(memory_dc); } // ← 나중
}
```
- **문제**: `SelectObject(memory_dc, hbitmap)`의 이전 핸들은 저장·복원되지 않는다(`gdi.rs:67`). 비트맵이 DC에 **선택된 상태**에서 `DeleteObject`은 실패(HRESULT 0)하며 객체가 남는다. `:114-117`에서 창 크기가 바뀔 때마다 `release_resources` + `init_resources`가 반복되므로, GDI 백엔드 활성화 상태(기본값 `engine: "auto"` + 멀티모)에서 리사이즈마다 HBITMAP·DIB 섹션이 커널 GDI 객체로 누수된다.
- **수정**: 순서만 교환 — `DeleteDC(memory_dc)`를 먼저, `DeleteObject(hbitmap)`를 뒤로. 2줄 diff.
- **미재현**: GDI 객체 누수 재현을 하지 않았으며 `DeleteObject`이 실제로 0을 반환하는지는 해당 빌드에서 미확인.

### 4.9 `image_index.db` 갱신이 파이프라인에 반영되지 않음

- **파일**: `rust/overmax_data/src/community/cache_downloader.rs:252-257` ↔ `:102-119`
- **문제**: `refresh_image_index`는 `StartupCacheManager` 백그라운드 스레드에서 실행되지만 `ImageIndexDb::load`(`store/image_index.rs:65`)는 커넥션을 **로컬 변수로만 갖고 함수 종료 시 즉시 닫는다**(`entries`는 `Arc<Vec<ImageEntry>>`로 메모리에 복사됨). DB 교체 자체는 열려 있는 핸들 때문에 실패하지 않지만, **교체된 새 DB는 실행 중 파이프라인에 반영되지 않는다.** `poll_updates`(`:102-119`)는 `updated_varchive_db`/`updated_sheet_meta`만 `Arc` swap하고 `image_db`는 건드리지 않는다. 그 결과 `image_db_version.txt`에는 새 tag가 기록되어 다음 실행은 "최신 버전 유지 중"으로 로그인하면서 **새 자켓 DB는 다음 재시작 전까지 반영되지 않는다.** 사용자에게는 갱신 성공 로그만 보인다.
- **수정 방향**: 기존 `CacheUpdateResult` 구조체에 `updated_image_index: Option<PathBuf>` 필드 1개 추가 후 `poll_updates`에서 이를 받아 호출측에 경로만 알린다. **새 추상이 아니라 기존 구조체 필드 추가**로 최소 diff. 반영 호출부(엔진 파이프라인)는 본 리뷰 범위 밖이라 미검증.

### 4.10 서버 JSON을 검증 없이 `raw_data`로 영속화

- **파일**: `rust/overmax_data/src/store/record_db/sync.rs:44-65`
- **문제**: `title`이 임의 문자열이면 `song_id` TEXT로 그대로 저장되고, 생성 컬럼은 `json_extract(raw_data, '$.score'|...)`로 파생된다. 서버가 `title: "abc"`를 주면 `queries.rs:293`의 `song_id_str.parse().unwrap_or(0)`에 의해 **유효한 song_id 0으로 조용히 매핑**되어 실별 Top-50에 오염 데이터가 섞인다(CONTEXT.md 불변 조건 4). `difficulty`도 `Difficulty::from_str` 검증 없이 저장된다.
- **수정**: `sync.rs:51` 직후 `if song_id.parse::<i32>().is_err() { continue; }`, `:56` 직후 `if Difficulty::from_str(difficulty).is_none() { continue; }`. `queries.rs:293`의 `unwrap_or(0)`은 `continue`로 교체.

### 4.11 V-Archive API URL에 사용자 입력을 인코딩 없이 보간 — `since` 부분 해결, `v_id` 남음

- **파일**: `rust/overmax_data/src/gateway/varchive.rs:126-137` (`fetch_records`)
- **원래 진단 (오류였음)**: "`v_id`와 `since` 모두 화이트리스트 필터(`A-Za-z0-9-_.:`)로 제한하면 된다". 이 제안은 **철회**한다. `v_id`는 `settings_ui.rs:448` `v_archive_id_row`의 자유 입력 필드이고 `trim()`만 거친다. 실사용 값도 숫자가 아니다 — `settings.user.json`의 실제 값은 2글자 `og`다. 화이트리스트를 걸면 한글 등 정상 사용자를 차단한다.
- **실측 정정**: `reqwest::Url::parse` 는 퍼센트 인코딩을 **하지 않는다**. 이미 존재하는 구분자를 URL 구조로 해석한다. Rust 프로브로 확인한 결과:

  | 입력 | `format!("...?since={}")` 결과 | 쿼리 키 |
  |------|----------------------------|---------|
  | `abc&since=evil` | `...?since=abc&since=evil` | **`["since","since"]`** — 인젝션 성립 |
  | `a/b?x=1#frag` | `/archive/a/b?x=1#frag/button/4` | **`["x"]`** — 경로가 `archive/a`로 절단 |
  | `한글아이디` | `/archive/%ED%95%9C...%EB%94%94/button/4` | `["since"]` — 한글은 정상 |

  즉 한글은 문제없고(UTF-8 퍼센트 인코딩), **구분자를 포함한 값이 경로/쿼리 구조를 조작한다**는 것이 실제 위험이다.
- **`since` (완료)**: `query_pairs_mut().append_pair("since", s)` 로 교체. `abc&since=evil` 케이스에서 키가 `["since"]` 하나가 되고 값은 `abc%26since%3Devil` 로 이스케이프된다. `HttpClient::get` 을 `U: IntoUrl` 제네릭으로 바꿔 파싱된 `Url` 을 넘길 수 있게 했다. 회귀 테스트 4건(`v_id_keeps_non_ascii_verbatim` 포함). 커밋 `74094d8` 참고.
- **`v_id` (미해결)**: 경로 세그먼트는 `query_pairs_mut` 로 고쳐지지 않는다(위 표 두 번째 행 — 경로가 여전히 절단된다). 올바른 해법은 `percent-encoding` 크레이트의 세그먼트 단위 인코딩이며, 이 크레이트는 이미 트리에 있다(reqwest 의존성, 새 의존성 추가 불필요). **다만 v_id 의 실제 값 분포를 모른 상태에서 스코프를 넓히지 않는 편이 맞다고 사용자 판단을 받아 보류했다.** 닫는 방법: 실제 v_id 샘플을 수집해 경로 조작이 가능한 입력(슬래시/물음표 포함)이 있는지 확인.
- **`fetch_single_song_records` (미해결)**: `:151-161` 도 `?title={song_id}` 을 보간하지만 `song_id` 는 `i32` 타입이라 위험이 없다.

### 4.12 V-Archive 토큰이 로그로 노출될 수 있는 Debug derive

- **파일**: `rust/overmax_data/src/gateway/varchive.rs:13-17, 26-32, 77-84`
- **문제**: 토큰 하드코딩은 없고 파일에서만 읽으므로 양호하나, `AccountInfo`가 `#[derive(Debug, Clone)]`라 `{:?}` 포맷으로 **토큰이 로그에 그대로 노출될 수 있다**. `RecordDB`에 이미 `masked_steam_id`(`mod.rs:71-83`) 마스킹 관례가 있는데 AccountInfo에는 없다. `UploadResult`도 `Debug` derive라 이것도 전파된다. 또한 `upload_score` 실패 시 `message: e.to_string()`(`:90`)이 reqwest 에러 문자열(URL 포함 가능)을 UI(`sync_ui`)로 흘려보낸다.
- **수정**: `#[derive(Debug, Clone)]`를 `#[derive(Clone)]` + `impl fmt::Debug for AccountInfo`로 `token`을 `"***"` 마스킹. 기존 `masked_steam_id` 패턴 확장이며 추상 추가 아님.

### 4.13 `image_index.rs::load()`가 읽기 전용 경로에서 매번 DDL을 실행

- **파일**: `rust/overmax_data/src/store/image_index.rs:64-71`
- **문제**: `load()`는 디텍션 파이프라인 초기화 시(`detection_worker.rs:396`) 호출되는데 매 로드마다 `ALTER TABLE images ADD COLUMN metadata TEXT`를 시도하고 `let _ =`로 무시한다. **실측**: 첫 로드 후 `PRAGMA table_info(images)` = `[id, image_id, phash, dhash, ahash, hog, orb, metadata]` — 이미 컬럼이 있는데도 매번 실패하는 DDL을 던진다. 파일이 읽기 전용이어도 `Connection::open`이 성공하므로 오류가 조용히 사라진다. `ImageIndexDb`는 WAL/busy_timeout을 설정하지 않아 `RecordDB::open_conn`(`mod.rs:86-94`)과 설정도 다르다.
- **수정**: 컬럼 존재 시에만 ALTER하도록 private 헬퍼 1개 추가(파일 내부 한정).

### 4.14 스키마 마이그레이션 버전 관리 부재 (`PRAGMA user_version` 미사용)

- **파일**: `rust/overmax_data/src/store/record_db/schema.rs` 전체 / `store/image_index.rs:64-71`
- **문제**: `grep -rn 'user_version' rust/` 결과 0건. 마이그레이션이 "컬럼이 있나?" 즉각 판정(`table_has_column`, `image_index.rs:66`)으로만 이루어져 실행 순서·버전에 의존한다. **실측**: `hog` 컬럼이 없는 구 스키마 DB에 `load()` → `Err("no such column: hog ...")` — `ALTER TABLE ... metadata`는 `images` 테이블에 `metadata`만 추가하므로 구 스키마를 복구하지 못한다.
- **수정**: `RecordDB::initialize` 선두와 `ImageIndexDb::load` 선두에 `PRAGMA user_version` read/write 2줄씩 추가. 기존 컬럼 판정 경로는 유지(호환).

### 4.15 `initialize()`가 마이그레이션 실패를 삼키고 `is_ready = true`로 전환

- **파일**: `rust/overmax_data/src/store/record_db/schema.rs:6-23`
- **코드**:
```rust
if self.create_records_table(&conn).is_ok() && ... {
    self.ensure_schema(&mut conn);
    self.is_ready = true;
    return true;
}
```
- **문제**: `ensure_schema`가 `()`를 반환하고 내부 전부 `let _ =`로 무시하므로 DROP/ALTER이 실패해도 `is_ready = true`가 된다. 이후 모든 `upsert`/`get`이 "성공" 경로로 실행되며 `mod.rs:217 res.unwrap_or(false)`로 실패가 `false`(무변경 indistinguishable)로 뭉개진다.
- **수정**: `ensure_schema`를 `Result<()>`로 바꾸고 `self.is_ready = self.ensure_schema(&mut conn).is_ok();` 한 줄 변경.

### 4.16 `upsert`가 트랜잭션 없는 read-modify-write

- **파일**: `rust/overmax_data/src/store/record_db/mod.rs:152-216`
- **문제**: `with_retry`가 매 시도마다 **새 커넥션**을 열고(`:103`) SELECT과 INSERT 사이에 `BEGIN`이 없다. 두 스레드가 같은 `(steam, song, mode, diff)`를 갱신하면 둘 다 같은 `existing_rate`를 읽고 각자 `final_rate = rate.max(ext_r)`를 계산한 뒤 마지막 writer가 덮어쓴다.
- **수정**: `with_retry` 클로저 내부 첫 줄에 `conn.execute_batch("BEGIN IMMEDIATE")`, 클로저 끝에 `COMMIT` 1줄씩 추가. UPSERT 자체는 이미 idempotent하므로 결과 불변, 직렬성만 확보.
- **재현 실패**: 4스레드 × 50회 프로브에서 `raced_final_rate=93.49`로 정상 수렴. WAL + `busy_timeout=5000`이 자연 직렬화한 결과로 보이며, **재현 실패는 버그 부재를 증명하지 않는다.**

### 4.17 OCR 제거 후 남은 죽은 설정 필드와 잘못된 문서 서술

- **파일**: `rust/overmax_data/src/config/settings.rs:456-457`, `settings.json:8`, `CONTEXT.md:177`
- **문제**: `logo_ocr_cooldown_sec` 필드를 읽는 소비자가 **존재하지 않는다**(정의 `settings.rs:457, 651`와 `settings.json:8`만 존재). `CONTEXT.md:177`의 "Rate OCR 텔레메트리(...) 지원"과 `detection_pipeline.rs:1244`의 테스트 주석 "isolate OCR checksum bypass caches"는 2026-07-28 OCR 완전 제거 이후 유효하지 않다.
- **수정**: 필드는 유지(호환성 — AGENTS.md 「기존 호환성 파괴 금지」), `CONTEXT.md:177`에서 OCR 텔레메트리 서술을 제거하고 "Rate는 Pure Rust 템플릿 매칭(ZNCC) 기반"으로 갱신. `detection_pipeline.rs:1244` 주석의 "OCR"를 "template cache"로 수정. **코드 로직 변경 없음.**

### 4.18 이진화 대비율 72%가 롤백됐는데 문서가 72%를 current로 서술

- **파일**: `rust/overmax_cv/src/image.rs:783`
- **코드**:
```rust
let calculated = (min as f32 + contrast * 0.65) as u8;
```
- **문제**: `docs/decisions/detection_pipeline.md:75`와 `CONTEXT.md:12`는 모두 "글로벌 콘트라스트 이진화 비율을 72%로 정밀 튜닝"이 achieved 상태라고 명시한다. `git show e3c1884`는 0.65→0.72로 올렸고, 다음 커밋 `8b52da6`(ZNCC 소프트 매칭 도입)가 되돌렸으나 **문서를 갱신하지 않았다**. `image.rs:1153`의 테스트 주석도 여전히 "기존 하드 이진화(72% 대비)"를 과거형으로 기술한다.
- **AGENTS.md 근거**: 「Context Usage Policy — context.md를 단일 source of truth로 사용」, 「Session Handoff Protocol 3 — 아키텍처 변경이 있었다면 CONTEXT.md 갱신」
- **수정**: **코드 변경 없음**(현재 0.65가 `8b52da6`의 의도된 값). `docs/decisions/detection_pipeline.md`에 `8b52da6` 롤백 행 추가 + `CONTEXT.md:12`의 "72%"를 "65%(ZNCC 소프트 매칭 채택으로 e3c1884의 72% 롤백)"으로 정정.
- **git blame 게이트**: `8b52da6` (2026-09-11) — 문서 정정만 하므로 코드 게이트와 무관.

### 4.19 `detect_rect_edges`의 margin 8이 unscaled

- **파일**: `rust/overmax_engine/src/detector/detection_pipeline.rs:803-807`
- **코드**:
```rust
fn detect_rect_edges(frame: &CapturedFrame, roi: crate::detector::roi::RoiRect) -> Option<f32> {
    let margin = 8;
    roi.with_margin(margin)
```
- **문제**: `git show b54ce34`의 커밋 메시지는 "scale edge detection and category band margins dynamically based on ROI scale"이며, 해당 커밋은 `detect_jacket_edges(frame, jacket_roi, _scale: f32)`로 스케일 파라미터를 추가했으나 **본문에서 사용하지 않았다**(`let margin = 8;` 그대로). 이후 `58e7ea0`이 함수를 축소하며 스케일 파라미터를 제거했다. `RoiRect`는 `transform_roi`로 이미 스케일되므로(`roi.rs:225-228`) 1440p에서 `player_panel`은 316×40 → 421×53이 되는데 margin은 8px 그대로다. 스케일된 ROI에 unscaled margin을 더하면 `detect_rect_edges`의 조기 반환 분기(`image.rs:173`)와 엣지 샘플링 구간이 서로 다른 비율로 움직인다. 결정 기록과 코드의 불일치.
- **수정 방향**: `scale: f32` 파라미터를 추가하고 `let margin = ((8.0 * scale).round() as i32).max(4);`로 변경, 호출부(`:531`, `:535`)에 `rois.scale()` 전달. **동작 변경이므로 1440p 회귀 스냅샷으로 검증 후 별도 커밋으로 분리.**
- **git blame 게이트**: `b54ce34` (2026-08-09), `58e7ea0`.
- **미측정**: 1440p에서 unscaled margin 8이 엣지 strength에 미치는 정량적 영향은 스냅샷 측정 없이 판단하지 않았다. `docs/` 내 1440p 스냅샷 존재 여부도 확인하지 않았다.

### 4.20 IPC `/rpc`에 인증·rate limit 부재, 연결마다 무제한 스레드

- **파일**: `rust/overmax_app/src/system/ipc_server.rs:329-336`, `system/transport/loopback.rs:220-231, 276-346`
- **코드**:
```rust
"set_overlay_visibility" => {
    let visible = arg(0).and_then(|v| v.as_bool()).unwrap_or(false);
    let _ = cmd_tx.send(IpcCommand::SetOverlayVisibility(visible));
```
- **문제**: `loopback.rs:353`은 peer가 loopback인지만, `:389-391`은 Host 헤더가 `127.0.0.1`/`localhost`로 **starts_with** 하는지만 검증한다. 인증 토큰이 전혀 없다. 또한 `:220-231`은 연결마다 `std::thread::Builder::spawn`으로 **무제한 스레드**를 만들며 풀/세미포어 제한이 없다. SSE(`sse.rs:52` `MAX_CLIENTS = 16`)에만 클라이언트 제한이 있고 RPC 핸드셰이크 스레드는 무제한이다. `cmd_tx`는 unbounded channel이므로 무제한 호출 시 무한 증가한다. `dispatch_rpc`(`:310`)가 IPC 스레드에서 동기 DB 쿼리(`get_recent_records` → `open_conn`)를 수행하므로, 무제한 스레드와 결합하면 **로컬 DoS 벡터**(파일 핸들 고갈 + `busy_timeout=5000` 락 대기)가 된다.
- **수정 방향**: 연결별 스레드 생성 수 제한(세마포어/풀). 인증 토큰 추가는 `PROTOCOL_ID = "overmax-ipc/1"` 프로토콜 변경이므로 **사용자 사전 동의 필요** — 범위 외로 둔다.
- **미검증**: 무인증 제어가 실제 익스플로잇 가능한지 판단하지 않았다. "공격"이 아니라 **설계 결정**(로컬 전용 IPC, 기본 `enabled = false`)일 가능성도 있어 조치 전 사용자 확인 필요.

### 4.21 벤치/검증 하네스가 릴리스 빌드에 포함

- **파일**: `rust/overmax_app/Cargo.toml:18-24`
- **코드**:
```toml
[[bin]]
name = "verify_pipeline"
path = "src/bin/verify_pipeline.rs"
[[bin]]
name = "measure_breakdown"
path = "src/bin/measure_breakdown.rs"
```
- **문제**: `build.bat:19`는 `cargo build -p overmax-app --release`만 수행하므로 두 바이너리가 **릴리스 빌드에 그대로 컴파일·패키징**된다. `measure_breakdown.rs:23`은 Direct3D11 + Windows API를 직접 링크한다. `src/bin/benchmark_lowres.rs`는 `[[bin]]` 선언이 없어 자동 탐색으로 빌드되나 동일 문제.
- **수정**: 세 `[[bin]]`에 `required-features = ["bench-harness"]`를 붙이고 `[features] bench-harness = []` 추가.

### 4.22 CV 파이프라인의 불필요 중복 작업 (성능 우선 항목군)

모두 AGENTS.md 「성능 저하 야기 금지」 및 「추상 추가 금지」 양쪽을 고려한 최소 diff가 가능한 항목이다. **정량 효과는 전부 미측정**이며, 구조적 중복만 확인했다.

| 항목 | 파일:라인 | 내용 |
|------|-----------|------|
| 자켓 매칭 3회 중 1회가 항상 버려짐 | `detection_pipeline.rs:757-788` | freestyle/openmatch를 무조건 둘 다 실행한 뒤 similarity 비교로 승자 선택. `match_jacket`는 그리드 히스토그램 + 전체 DB 선형 순회를 포함한 최대 비용 연산(`jacket_matcher.rs:174-178, 232-262`). `is_unknown`일 때만 이 구분이 필요하다(`:686-690` 이미 전제). |
| 카테고리 띠 검사가 같은 픽셀을 2회 순회 | `detection_pipeline.rs:834-857` | 1차 평균 계산 후 2차 편차 계산에서 띠 픽셀을 재순회. 띠 폭이 작아 절대 비용은 작지만 「다중 패스 루프 금지」 조항 정면 위반. 1차 패스에서 스택 버퍼에 담아두면 2차 순회 제거. |
| 동일 자켓 ROI를 한 함수에서 2회 힙 복사 | `detection_pipeline.rs:406, 420` | `make_thumbnail(&jacket)`이 내부에서 `to_image_region()` 1회, `:420`이 `jacket.to_image_region()`을 다시 호출. 60×60×4 = 14.4KB를 `JACKET_MATCH_INTERVAL = 0.25`로 ~4Hz 반복 복사. |
| 3종 해시가 동일 픽셀을 3회 리샘플링하며 5회 힙 할당 | `overmax_cv/src/image.rs:33-39, 61-85, 115-125` | `ahash`/`dhash`/`phash` 각각 리샘플 + `dct_2d_32`의 `vec![0.0; 1024]` 2개. 같은 파일의 다른 경로(`d472589`)는 이미 무할당화했으나 해시 경로만 방치. |
| `detect_rate`가 버리는 이진화 버퍼 | `templates/matching.rs:6-7, 186, 225` | 유일한 프로덕션 호출자가 `binary`를 `_`로 버림. `binarize_by_luminance_with_luma`가 `luma_vals` + `binary` 2개를 할당하므로 129×32 기준 8KB가 확정적으로 낭비. 같은 파일 `:15`의 doc comment는 `detect_score`를 "Zero String Allocation"이라 명명. 반환 타입을 `(String, u8, u8)`로 축소하면 해결. |
| `median_result_rate`가 매 호출마다 Vec 할당 + 정렬 | `play_state.rs:302-306` | 윈도우가 최대 7개로 고정(`:297`)인데 힙 할당 + 정렬. `[f32; 7]` 스택 배열로 대체. |
| `ImageView`의 zero-copy 계약이 해시·에지 진입점에서 깨짐 | `capture/frame_utils.rs:130-141, 94-105` | `to_image_region()`이 매번 소유 `Vec<u8>`을 할당하고 전 행 복사. `play_state.rs:690, 715`가 매 프레임 `compute_hashes(4)`를 호출. `crop`(`:66-85`)이 항상 stride == width*4를 보장하므로 슬라이스 직접 전달으로 교체 가능. |
| 씬 미스마다 CV 비용을 지불하는 순수 텔레메트리 경로 | `detection_pipeline.rs:349-350, 360-364` | `screen_static_thumb_diff`가 ROI 크롭 + 힙 복사 + 그레이 + resize 할당을 포함하며 결과는 stats 로깅으로만 소비. 판정 경로와 독립적으로 얹혀 있음. 텔레메트리 비활성 시 스킵 가드 1개면 됨. |

### 4.23 Linux 경로의 정규화 부재로 Windows와 인식 결과가 달라질 수 있음

- **파일**: `capture_engine/linux.rs:608-620` vs `windows/dxgi.rs:511-624`, `windows/normalizer.rs:34-77`, `detector/roi.rs:212-233`
- **무엇이 문제인가**: Windows는 512×512 GPU 아틀라스 또는 1920×1080 GPU bilinear 정규화로 픽셀을 재샘플링한다. Linux는 정규화/아틀라스 경로가 **존재하지 않으며** `capture_bgra_inplace`의 `rect` 인자를 `_rect`로 버린다(`linux.rs:138`) → 원본 해상도 프레임이 그대로 나온다. 소비측 `RoiManager::calculate_transform`은 `roi.rs:243`의 `(x as f32 * self.scale) as i32` **정수 절삭**으로 ROI를 만든다. 동일 게임 상태라도 Windows는 bilinear, Linux는 nearest로 ROI 픽셀이 다르므로 동일 임계값(`settings.jacket_matcher.similarity_threshold`)에 대한 인식 결과가 달라질 수 있다. `atlas_layout.rs`의 47개 슬롯 검증 테스트 전체가 Windows 아틀라스 경로에만 적용되어 Linux는 커버리지가 0이다.
- **수정 방향**: 지금 단계에서 Linux 정규화를 도입하지 않는다. `linux.rs:138`의 `_rect` 대신 실제 `WindowInfo` geometry로 ROI 스케일을 명시하는 것만 권고(동작 변경 없음).
- **미측정**: 실제 오인식률 영향은 측정하지 않았다. 실측 전에는 "버그"로 단정하지 않는다.

### 4.24 Linux 풀 프레임 2회 순회

- **파일**: `rust/overmax_engine/src/capture/capture_engine/linux.rs:573-603` (특히 `:596-598`)
- **코드**:
```rust
for (source, destination) in generation.map.chunks_exact(generation.stride)
                                      .zip(out_frame.bgra.chunks_exact_mut(row)) {
    destination.copy_from_slice(&source[..row]);
    for alpha in destination[3..].iter_mut().step_by(4) { *alpha = 255; }   // 픽셀마다
}
```
- **문제**: Windows GPU 아틀라스 경로가 47개 슬롯(총 ~30KB)만 CPU로 옮기는 것과 달리, Linux는 **풀 프레임**을 매 캡처마다 CPU로 복사하고 4바이트마다 별도 라이트한다(1080p 기준 2,073,600회 반복). 기능상 필요한 작업(알파 강제)이지만 프레임 전체를 2회 순회한다.
- **수정**: 알파 강제 루프를 `chunks_exact_mut(4)`로 바꾸는 2줄 변경에 한정.

### 4.25 문서-코드 드리프트 및 릴리스 추적 누락

- **아틀라스 슬롯 수 불일치**: 코드는 `ATLAS_SLOTS: [AtlasSlot; 47]`(`atlas_layout.rs:24`)인데 `CONTEXT.md:106`은 "43개 ROI(240,098 px)", `TASKS.md:49`는 "`[AtlasSlot; 43]` 베이킹", `TASKS.md:51`은 "43개 슬롯 간 상호 AABB Overlap 0건 전수 검증"이라고 기재. `docs/decisions/detection_pipeline.md`의 2026-09-27 행만 "43→47"로 갱신되어 있어 **문서 간 불일치**도 존재. 총 패킹 픽셀 수 재계산 필요.
- **PR #27 기능이 TASKS.md에 미등록**: `TASKS.md` grep `gameplay|paused|인게임 씬|일시정지` → 0건. 병합된 핵심 기능(인게임/Paused 씬 감지, 씬 독립 Global ROI + `gp_*`/`pause_title` 패킹, IPC 스냅샷 씬/컨텍스트 분리)이 릴리스 추적 체계에서 빠졌다. `TASKS.md`의 `[Active] Milestone v0.4.1`은 1~5번 섹션만 담고 있고 `8.1 래더매치 씬 감지 대응`은 `[ ]` 상태.
- **릴리즈 노트가 코드 상태를 반영 못 함**: `Cargo.toml:11`은 `0.4.1`이고 `RELEASE_NOTES_v0.4.1.md` mtime는 2026-09-08인데 9월 27일 커밋들이 대량 병합됐다. 노트에 `gameplay`/`paused`/인게임 씬 언급 0건.
- **README에 현재 버전 표기 자체가 없음**: `README.md:103-105`, `README.en.md:103-105` 모두 `v0.5.0 로드맵` 제목과 "차기 버전 개발" 문구만 있고, "현재 릴리스가 v0.4.1"을 나타내는 표기가 어느 라인에도 없다(`grep -E '0\.4\.1'` 결과 0건). AGENTS.md Release Protocol 체크리스트 3번이 어느 릴리즈에서든 수행되지 않은 상태.
- **미완료 계획 항목**:
  - `docs/plans/2026-09-23-atlas-optimization-game-cycle-plan.md:26` — `### [ ] Step 4: 캡처 파이프라인 연동 검증`. 본문 판단("DXGI `copy_slots_to_atlas`는 `ATLAS_SLOTS` 순차 순회이므로 코드 수정 불필요")은 `docs/decisions/detection_pipeline.md` 2026-09-27 행에 이미 기록되었으나 계획 문서 체크박스가 미갱신. **신규 7개 ROI의 DXGI 실캡처 실측 검증은 한 번도 수행되지 않음.**
  - `docs/plans/2026-09-23-gameplay-scene-pipeline-redesign.md:156` — 실앱 GDI/DXGI 상태 전이 확인 미실행. 같은 문서 97행은 "실앱 런타임 연결은 별도 환경에서 검증 필요 → 가정으로 me우지 않고 기록함"이라고 적어 두었으나 이 미검증 사실이 `TASKS.md`/`CONTEXT.md`로 전파되지 않음.
- ~~**CONTEXT.md에 Gameplay ROI의 씬 독립(Global) 설명이 누락**~~ — **오탐(철회).** 정정 착수 중 `CONTEXT.md:131`에 이미 존재함을 확인했다: 「**씬 독립(Global) ROI**: 특정 씬에 속하지 않는 ROI는 `GlobalRoiConfig.rois`에 위치하며, 아틀라스 모드에서는 `SceneType::Unknown` 스코프로 조회됩니다. `RoiManager::get_global_roi()`가 이 경로를 캡슐화하여 아틀라스/전체 프레임 양쪽을 모두 처리합니다.」`a278aaf`(2026-09-27)가 반영한 내용이다. 리뷰 워커가 `grep 'gp_'` 위치를 잘못 좁혀 판단한 것으로 보인다. (단, 리뷰 문서에 이 항목을 남긴 것은 리뷰 워커가 같은 파일의 인접 행을 중복 확인하지 않은 채 넘어간 데 따른다. 재검토 시 `CONTEXT.md` 관련 서술은 130~134행 전체를 읽을 것.)
- CONTEXT.md에는 변경 이력(History) 섹션 자체가 존재하지 않는다(grep `변경 이력|History` 0건). AGENTS.md Release Protocol 체크리스트 4번이 요구하는 이력이 `docs/decisions/`에 위임된 구조인지 실제 누락인지는 사용자 판단 필요.

---

## 5. LOW

즉시 조치 대상이 아니며, **삭제/변경 시 사용자 승인이 필요한 항목**이다. AGENTS.md 「무관한 포맷팅/정리를 기능 변경과 같은 커밋에 섞지 않는다」에 따라 별도 취급 대상.

| 파일:라인 | 내용 |
|-----------|------|
| `overmax_engine/src/capture/capture_engine/windows/dxgi.rs:44, 142-144, 207, 384-385` | `hdr_lut: Arc<[u8; 65536]>`가 선언·대입만 있고 **읽는 곳이 없다**. `build_lut_table`(`hdr_pipeline.rs:111-128`)이 65,536회 `powf`를 수행하며 엔진 생성 시와 화이트레벨 변경 시마다 실행된다. 전역 `ACTIVE_HDR_LUT`/`get_active_lut`도 호출자 없음. |
| `overmax_app/src/ui/overlay_ui.rs:92`, `linux_layer_overlay.rs:1404`, `native_app_viewports.rs:891` | `OverlayProps.record_manager`가 `ui/` 하위 어디에서도 읽히지 않음. DB 핸들을 매 프레임 UI props에 넘기는 구조 자체가 위험. `OverlayProps` 쪽만 먼저 제거하는 1-commit 권장(`LinuxOverlaySnapshot`의 것은 `Arc::ptr_eq` 비교에 쓰이므로 별도). |
| `overmax_app/src/ui/sync_ui.rs:371` | `ui.add_sized([170.0, 20.0], egui::Label::new(""))` — 빈 Label이 spacery 대체로 남은 것으로 보임. `ui.allocate_space(egui::vec2(170.0, 20.0))`로 동작 동일. |
| `overmax_app/src/ui/linux_layer_overlay.rs:408` | `bytes.try_into().expect("four-byte chunk")` — `chunks_exact(4)`가 길이 4를 보장하므로 도달 불가. 다만 `parse_foreign_toplevel_states`는 **Wayland 백엔드 스레드**에서 호출되어 `panic = "abort"`(Cargo.toml `profile.release`)로 프로세스 전체 종료된다. |
| `overmax_app/src/ui/debug_ui.rs:129, 154, 189, 225, 252, 274, 302, 330, 358, 385, 416, 464, 516` | 디버그 창 섹션 헤더 13곳이 `t!` 매크로를 우회하고 영어 하드코딩. `i18n.rs`는 2026-08-14/25 다수 편집 이력으로 **git blame 게이트 적용 대상**(`docs/decisions/ui_and_i18n.md:26-27,35`). 디버그 창은 개발자 전용 뷰포트이므로 플레이어 영향 없음. |
| `overmax_engine/src/detector/templates/gameplay_scene.rs:18-25` | `matches!((frame.width, frame.height), (512, 512) \| (1920, 1080))` 해상도 화이트리스트. 불일치 시 `read_scene`가 **로그 없이** `Unknown`을 반환하고(`:28-30`), 파이프라인은 `detection_pipeline.rs:155-159`에서 인게임 히스토리를 즉시 폐기한다. DXGI 백엔드는 1080p 정규화로 1920×1080만 도달하나, GDI 백엔드에서 네이티브 1440p 프레임이 오면 인게임 탐지가 조용히 영구 비활성화된다. |
| `overmax_data/src/community/cache_downloader.rs:122-141, 240-241` | `has_all_required_caches`의 `db_path`가 설정값이라 `root.join("../../..")`로 `root`를 벗어날 수 있고, `write_atomic`이 `create_dir_all(parent)`로 탈출 경로에 파일을 쓴다. `if !p.starts_with(root)` 2줄 가드로 차단 가능. |
| `overmax_data/src/community/sync.rs:108-120, 164-168` | `partial_cmp().unwrap_or(Ordering::Equal)`이 NaN에 대해 비교 불가능을 무시하고 정렬 순서가 비결정적이 된다. `records`는 `rate > 0`로 필터되어 걸리나 `varchive_records.rating`은 코드가 명시적 방어하지 않음. |
| `overmax_engine/src/detector/telemetry.rs:439`, `capture_engine/windows/hdr_pipeline.rs:4` | `#[allow(dead_code)]`인 `environment_value`와 `SCRGB_REFERENCE_WHITE_NITS` 모두 호출자 없음. 후자는 "80"이라는 물리 기준이 미사용 상수 1곳과 하드코딩 계산식 2곳에 분산된 상태. |
| `overmax_engine/src/capture/capture_engine/linux.rs:110, 145, 560` | `expect("initialized X11 connection")`, `expect("capture generation")` 3건. 현재는 `new()`의 `fatal` 가드와 `ensure_generation()` 순서로 도달 불가하지만, 다음 리팩토링 한 번에 깨질 수 있는 위험 지점. `ok_or("...")?`로 교체 권고. |
| `.gitignore:15-25` | Python/PyInstaller용 패턴(`dist/`, `build/`, `lib/`, `lib64/`, `var/`, `parts/`)이 저장소 최상위에 광범위 적용. `build/`와 `dist/`는 중복 2회 기재. `/dist/`, `/build/`처럼 루트 앵커로 좁히는 단독 커밋 권장. |
| `rust/overmax_engine/src/detector/templates/gameplay_scene/pause_title-play-092.gray` (커밋 `21c9652`) | 21c9652 diff stat에서 "1 +"로 기록되었으나 실제 크기 4144 bytes의 CRLF 포함 8비트 그레이스케일 바이너리. `.gitattributes`에 `*.gray binary` 추가 권장(단독 커밋). |
| `overmax_data/src/service/recommend/scoring.rs:575-607` | `#[cfg(test)] fn derive_top50_base_floor`가 테스트(`tests.rs:436-491`)에서만 사용되고 프로덕션 호출부 없음. 블레임 `9e2b139`(2026-08-31, 3개월 미만)이며 제거 시 테스트가 깨지므로 **그대로 둔다**. |

### 남은 `#[allow(dead_code)]` 4건 (정상 판정)
`detection_worker.rs:218`(`presentation_observation` — Linux `tick_linux`에서만 읽힘), `dxgi.rs:212`(`active_sdr_white_level` 게터), `dxgi.rs:431`(`enable_gpu_atlas` 게터 — 설정 주입 경로는 확인되나 읽기 소비자 grep 미확인), `overlay_theme.rs:53`(`DANGER` 색상 상수).

### 테스트 커버리지 갭
`store/record_db/{queries,schema,sync}.rs`와 `gateway/{asset_download,error,recommend_provider,varchive}.rs`에 `#[cfg(test)]` 모드가 **없다**. 통합 검증은 `record_db/mod.rs`의 8개 테스트(486-871행)가 담당하고 추천 스코어링은 `recommend/tests.rs` 1966줄이 커버한다(발견된 스코어 테스트: merged=3, repeat=2, reduction=2, P24=10/1, P11=1). **스키마 DDL/마이그레이션 경로와 gateway의 네트워크 오류 처리는 단위 테스트로 직접 커버되지 않는다** — §3.3~§4.15의 갭이 모두 여기에 속한다.

---

## 6. 미검증 (Unverified)

순간 판단하지 않은 항목. 다음 항목이 각각을 닫는다.

1. **~~`hdr_replay_test` 실패의 파일 단위 원인 미특정~~ — 해소됨(§3.1). 원인은 구(舊) 43슬롯 아틀라스 스냅샷 에셋이며 제품 코드 회귀가 아니다. 남은 미검증은 §3.1 "해제 조건" 참조(47슬롯 재 덤프가 있어야 `#[ignore]` 해제 가능).
2. **§4.19 margin 8 unscaled의 실제 오차 크기.** 1440p에서 unscaled margin이 엣지 strength에 미치는 정량적 영향 미측정. `docs/` 내 1440p 스냅샷 존재 여부 미확인. 선행 조건: margin 스케일 적용 전후 엣지 strength 값 비교.
3. **§4.22 CV 중복 작업 항목들의 실측 기여도.** `phash_list.len()`(DB 크기), 씬 폴당 `match_jacket` 3회 × 5할당이 전체 detect 시간 대비 몇 ms인지 산출하지 않았다. `cache/image_index.db` 크기 미확인.
4. **§4.5 미제거 슬롯이 실제 오인식을 유발하는 조건.** `local_left < 0`은 창이 화면 왼쪽으로 일부 드래그되어야 성립. 실제 게임 중 해당 상태 발생 빈도 미확인.
5. **§4.6 타임아웃 `Ok` 재전달이 안정화 카운터를 실제로 오염시키는가.** `detection_pipeline`의 history/stable 카운터 로직을 끝까지 읽지 않아 "동일 프레임 반복이 카운터를 전진시키는지" 확인하지 않았다.
6. **§4.4의 3초 GDI 강등이 실측 성능에 미치는 영향.** Decision Log의 "~4ms vs ~30ms"는 2026-08-15 측정값이며 현재 아틀라스 경로 기준과 비교 기준이 다르다. **현재 조건에서 수치를 측정하지 않았다.**
7. **§4.8 HBITMAP 누수 심각도.** GDI 객체 누수 재현을 하지 않았으며 `DeleteObject`이 실제로 0을 반환하는지는 미확인.
8. **§4.23 Windows bilinear vs Linux 정수 절삭의 인식 불일치 크기.** 메커니즘상 확실하나 이것이 실제 오인식률에 영향을 주는지는 **측정하지 않았다**. 유사도 임계값·정규화 히스토그램 기반 판정이라 임계 영향은 미지수다. 실측 전에는 "버그"로 단정하지 않는다.
9. **§4.16 upsert read-modify-write 경쟁.** 4스레드 × 50회 프로브에서 재현 실패. WAL + `busy_timeout=5000`이 자연 직렬화한 결과로 보이나, 실제 `with_retry`는 매 시도 새 커넥션을 열어 BUSY 시 **부분 반영 후 재시도** 경로가 이론적으로 가능하다. 실재 여부는 부하 조건에 의존하므로 MEDIUM으로 하강했다.
10. **§4.20 무인증 IPC의 실제 익스플로잇 가능성.** Threat Model 부재 상태에서 판단하지 않았다. 로컬 전용 IPC라는 **설계 결정**일 가능성도 있어 조치 전 사용자 확인 필요.
11. **`uses_manual_position`의 Windows/Linux 조건 불일치.** Linux(`linux_layer_overlay.rs:1498-1503`)는 `snap == "manual" || !window.fullscreen`, Windows(`native_app_viewports.rs:890`)는 `snap_position == "manual"`만 본다. 의도된 차이인지 버그인지 `docs/decisions/linux_support.md` 확인 대기.
12. **§4.2 `get_merged()`의 실제 프레임 비용.** 정적 분석으로 "매 프레임 호출됨"과 "deep clone + 역직렬화"는 확인했으나 실측(ms/frame)은 하지 않았다. §2.2·§4.2의 **정량적 심각도는 "구조적으로 프레임 예산 침범"까지가 한계**이며 수치를 지어내지 않았다.
13. **멀티모니터 + `engine: "auto"` 기본값 조합의 현재 실측 성능.** `mod.rs:136-140`이 멀티모니터에서 GDI를 우선하도록 되어 있어 AGENTS.md 「성능 저하 금지」와 상충할 수 있으나 실측 없이 MEDIUM으로만 표기. **사용자 프로필 확인이 필요하다.**
14. **i18n 미번역 키 잔존 여부.** `i18n.rs`의 Ja 암 개수(157) vs 정적 키 수를 정확히 대조하지 못했다(grep 카운트 기반, 매크로 본문 파싱 미완). 따라서 `docs/decisions/ui_and_i18n.md:35`의 "Ja 번역 100% 완료" 주장을 **검증하지 못했다**.
15. **캐시 1회 실패 후 24시간 스킵이 설계인지 누락인지.** `cache_downloader.rs:166-206`에서 실패해도 파일 mtime이 갱신되지 않아 `is_stale`이 계속 true를 반환하고, `StartupCacheManager`는 1회만 호출하므로 같은 프로세스 내 재시도가 없다. 타임아웃은 존재하여 "무한 멈춤"은 해당 없으나 **일시적 DNS/서버 5xx에 대한 자가 복구 부재**가 실질 문제다. `docs/decisions/data_and_sync.md`에 관련 항목이 없어 판단을 보류했다. AGENTS.md 「사용자의 의도 파악이 불확실한 경우 반드시 질문」에 따라 수정 제안만 한다.
16. **`ensure_dirs_and_seed`(`paths.rs:231-266`)의 포터블→Installed 전환 시 `record.db` 재복사.** WAL 모드 DB를 `fs::copy` 단일 파일로 복사하므로 미체크 `-wal` 파일의 커밋된 데이터가 유실될 수 **있으나 재현하지 못했다.**
17. **`recommended` provider 캐시 파일 읽기의 UI 스레드 지연.** `composite.rs:93-105`의 `read_to_string`이 `refresh_overlay_data` 경로(UI 스레드)로 들어올 수 있음. `refresh_overlay_data`는 `drain_detection_results`의 `changed` 시점에만 호출되어 **매 프레임은 아님**을 확인했으나 파일이 큰 경우 지연 여부는 미측정.
18. **`LATEST_STATE.try_lock` 갱신 누락의 실 영향.** `ipc_server.rs:355, 358`에서 `try_lock` 실패 시 갱신이 조용히 누락됨. IPC 전용 관찰자라 파이프라인 영향은 없으나 데이터 유실 가능. 락 경합 발생 빈도 미측정.
19. **`dxgi.rs:644-648`의 `0x887A0027` 하드코딩 HRESULT 외** OS 버전에 따라 매핑되는 다른 타임아웃 코드가 있는지 미확인.
20. **일반 `unwrap()` 사용량의 테스트 코드 유입 가능성.** `overmax_cv/image.rs` 7건, `detector/play_state.rs` 10건 등이 전부 `mod tests` 내부임을 grep 위치로 확인했으나, 테스트 코드가 프로덕션 경로에 `include!`되는지 전체를 추적하지 않았다. 현 구조상 불가능하지만 명시적으로 확인하지 않았다.

---

## 7. 권장 착수 순서

**진행 상태: `chore/codebase-review-fixes` 브랜치에서 착수 중.**

1. ~~**§2.1 (레거시 DB DROP)**~~ — **완료** (`d2954f9`). `DROP TABLE` → `ALTER TABLE ADD COLUMN`. 레거시 스키마는 `is_max_combo`만 빠진 채 나머지가 현행과 동일함을 `95048ec8^:data/record_db.py` 히스토리에서 확인했다. 회귀 테스트 2건 추가, 수정 전 코드로 되돌리면 실패함을 확인.
2. ~~**§2.2 (UI 매 프레임 DB)**~~ — **완료** (`ac1063b`). `current_pattern_needs_upload` 결과를 `drain_detection_results` 의 `changed` 지점에서 1회 계산해 `overlay_upload_needed` 로 캐시. Windows/Linux 렌더 경로 모두 참조로 변경. `is_varchive_account_configured` 는 `changed` 지점 밖에서도 바뀌어 그대로 유지.
3. ~~**§3.1 (hdr_replay 회귀)**~~ — **해소됨.** 제품 코드 회귀가 아니며, 스냅샷 에셋 교체 조건(`#[ignore]` 해제)만 남았다. 착수 대상에서 제외.
4. **§3.2, §3.3, §3.5** — 각각 1~3줄 diff, 데이터 정합성/플랫폼 계약. **다음 착수 대상.**
5. **§4.25 문서 정정 (일부)** — **완료** (`4bcfbf2`). 아틀라스 슬롯 43→47 및 217,952 px 로 5곳 정정. 잔여(RFC 버전 표기, PR #27 기능의 TASKS.md 미등록, 미완료 계획 문서 2건)는 릴리스 전략 판단이 필요해 보류.
6. **§3.4, §4.11, §4.12, §4.20** — 보안 관련. §4.20은 설계 의도 확인이 선행되어야 함.
7. **§4.18** — 이진화 대비율 문서 정정(코드 변경 없음).
8. **§4.22 계열** — 성능 개선. 각각 수정 전 계측을 붙일 것(AGENTS.md 「근거 없는 성능 개선 주장 금지」).

**여러 finding을 한 커밋에 묶지 말 것.** 각 diff는 하나의 검증 가능한 주장만 담아야 한다(AGENTS.md 「Diff & Commit Discipline」). 본 문서의 착수 순서를 지킬 이유도 이것이다.

---

## 8. 리뷰 과정에서 생성된 미정리 산출물

진단 목적 worktree가 남았다. **삭제 승인이 필요하며 아직 손대지 않았다.** 원본 저장소 워킹 트리는 변경되지 않았다.

- `git worktree list`에 `D:/tmp/om_base` (d9eabd3 체크아웃) — `hdr_replay_test` 기준 비교용
- `git worktree list`에 `D:/d/dev/_wb_check` (0c8f61b) — 디렉터리 자체가 없는 고아 등록
- `D:\dev\overmax\target_base_check/` — 별도 타겟 디렉터리, `git status`에 untracked로 잡힘
- `stash@{0}: On main: feat(cv): experiment with soft template matching for score` — **이 리뷰 이전에 있던 것인지 확인하지 못했다.** 삭제하지 말 것.

정리 명령:
```
git worktree remove --force /d/tmp/om_base
git worktree remove --force /d/dev/_wb_check
rm -rf /d/dev/overmax/target_base_check
git worktree prune
```

---

## 9. 참고: 리뷰 방식

5개 도메인으로 분할하여 병렬 정점리를 수행했다.

| 워커 | 범위 | 핵심 파일 |
|------|------|-----------|
| CV/디텍션 | `overmax_cv`, `overmax_engine/src/detector/**` | `detection_pipeline.rs`, `play_state.rs`, `atlas_layout.rs`, `image.rs` |
| 캡처 | `overmax_engine/src/capture/**` | `dxgi.rs`, `gdi.rs`, `linux.rs`, `frame_utils.rs` |
| 데이터 | `overmax_data/**` | `config/`, `store/`, `service/recommend/`, `community/`, `gateway/` |
| UI/시스템 | `overmax_app/**`, `overmax_core/**` | `native_app*.rs`, `linux_layer_overlay.rs`, `ipc_server.rs`, `i18n.rs` |
| 저장소 위생 | 문서/커밋/테스트/grep | `CONTEXT.md`, `TASKS.md`, `docs/`, `Cargo.toml` |

각 finding은 실제 파일을 읽고 `파일:라인`을 인용했으며, 판단이 불가능한 항목은 "unverified"로 분리했다. §2.1(DROP TABLE)과 §3.3(V-Archive 캐시 소실), §4.3(rename 동작), §4.13(테이블_info 실측) 등은 **실제 실행으로 재현**되었다.

리뷰 과정에서 워커들이 만든 임시 프로브 파일은 모두 정리되었으며 워크스페이스는 수정되지 않았다.
