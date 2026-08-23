# 설정창 UX 개선 작업 스펙

## 배경 및 목표

현재 설정창(`rust/overmax_app/src/ui/settings_ui.rs`)은 탭 3개(UI / V-Archive / System) 안에
성격이 다른 설정들이 뒤섞여 있어 "무엇을 어떻게 써야 하는지" 직관적으로 알기 어렵다.
특히 `System` 탭은 진단·캡처엔진·언어·외부 Provider·업데이트가 한데 모여있는 잡동사니 상태이고,
V-Archive 연동 흐름은 "읽기(자동 fetch)"와 "쓰기(수동 업로드)"가 한 폼에 섞여 있어
사용자가 몇 단계를 거쳐야 하는지 파악하기 어렵다.

목표는 기능 추가가 아니라 **기존 동작을 최대한 보존하면서 정보 구조(IA), 설명력, 시각적 톤을
순서대로 개선**하는 것이다. Phase는 아래 순서로만 진행하고, 이전 Phase가 빌드/클리피를
통과하고 리뷰가 끝나기 전에는 다음 Phase로 넘어가지 않는다.

## 이 문서를 실행하는 에이전트를 위한 공통 규칙

- 작업 시작 전 `AGENTS.md`, `ENGINEERING_TASTE.md`, `CONTEXT.md`,
  `docs/decisions/ui_and_i18n.md`(존재 시)를 먼저 읽는다.
- 이 문서의 코드 스니펫은 **작성 시점 기준 참고용**이다. 실제 파일 내용이 다를 수 있으니
  편집 직전에 반드시 현재 파일을 다시 읽고 그 내용을 기준으로 diff를 만든다.
- 각 Phase는 독립적으로 컴파일/클리피를 통과해야 한다. 여러 Phase를 한 커밋/PR에 묶지 않는다.
- `settings.user.json` / `settings.json`에 저장되는 JSON 키 구조는 어떤 Phase에서도 변경하지
  않는다 (UI 재배치이지 데이터 마이그레이션이 아님). 구조를 바꿔야만 하는 상황이 생기면 작업을
  멈추고 사용자에게 확인한다.
- i18n 키는 `rust/overmax_app/src/ui/i18n.rs`의 `t!` 매크로에 추가하며, 항상 `Ko`/`En` 쌍으로
  넣는다 (하나만 넣지 않는다).
- `cognitive_complexity` clippy 임계값(55)을 넘기지 않도록 함수를 적절히 쪼갠다. 이미 그런
  패턴(`overlay_section`, `varchive_tab` 등 섹션별 함수 분리)이 있으니 그대로 따른다.
- 이 작업과 무관한 파일(예: `record_db.rs`, `varchive_api.rs`, `detection_pipeline.rs` 등
  데이터/디텍션 계층)은 건드리지 않는다.
- Phase를 마칠 때마다 `cargo build --workspace`, `cargo clippy --all-targets --locked`,
  `cargo test --workspace`를 실행하고 결과를 보고한다. 세션을 종료하기 전에는
  `AGENTS.md`의 Session Handoff Protocol(`cargo fmt`, `TASKS.md` 갱신 등)을 따른다.
- 각 Phase는 리스크가 낮은 국소 변경이어야 한다. 진행 중 "이 김에 다른 것도 리팩토링하고
  싶다"는 유혹이 들면 하지 않는다 (`ENGINEERING_TASTE.md` Scope Control).

---

## Phase 1 — V-Archive 탭 재구성 (최우선, 스코프 확정됨)

### 목표
V-Archive 탭의 "읽기(자동 fetch)"와 "쓰기(수동 업로드)" 흐름을 시각적으로 분리하고,
불필요하게 많은 버튼을 줄이고, ID 저장 시 자동으로 1회 조회가 되도록 한다.

### 해야 할 것
1. `varchive_tab()`을 두 개의 `section_frame`으로 나눈다:
   - **"계정 연결"**: 연동 상태 라벨 + V-Archive ID 입력 + 새로고침 버튼 1개
   - **"자동 업로드 연동"**: 설명 1줄 + account.txt 연결 + "동기화 후보 찾기" 버튼
2. 기존 `steam_account_rows()` 함수를 없애고 `v_archive_id_row()`,
   `account_path_row()`, `scan_candidates_row()` 세 개로 분리한다.
3. 기존 4B/5B/6B/8B + All 버튼 5개를 "새로고침" 버튼 1개로 통합한다.
   버튼 클릭 시 내부적으로 `button: 0`을 `fetch_tx`에 보내면 기존 `spawn_fetch()`가
   4개 모드를 전부 순회하므로 별도 로직을 새로 만들 필요는 없다.
4. `scan_candidates_row()`에서 account.txt가 비어있으면 "동기화 후보 찾기" 버튼을
   `add_enabled_ui(false, ...)`로 비활성화하고, 아래에 이유를 안내하는 문구를 출력한다.
5. `i18n.rs`에 다음 키를 추가한다 (Ko/En 둘 다):
   `settings-varchive-connect`, `settings-varchive-upload`,
   `settings-varchive-upload-desc`, `sync-refresh`, `settings-account-path-required`.
6. `native_app_viewports.rs`의 `show_settings_viewport()` 저장 버튼 핸들러에서,
   저장 직전 V-Archive ID를 읽어두고, 저장 후 새 ID와 비교해 값이 바뀌었으면
   `settings_ctx.fetch_tx`로 `(steam_id, v_id, 0)`을 1회 전송한다. 새 채널이나
   새 스레드 스폰 패턴을 만들지 말고 기존 `fetch_req_tx`/`fetch_res_rx` 파이프라인을
   그대로 재사용한다.

### 하지 말아야 할 것
- `RecordDB`, `varchive_api.rs`, `RecordManager` 등 데이터 계층 로직 변경 금지.
- 새로운 background 폴링/스레드 패턴 도입 금지 — 기존 `spawn_fetch()` 그대로 사용.
- V-Archive 탭 외 다른 탭(UI, System) 손대지 않기 — Phase 2에서 다룬다.
- `account_path`/`v_id`가 저장되는 JSON 구조(`varchive.user_map.<steam_id>.{v_id,account_path}`)
  변경 금지.

### 완료 기준
- `cargo build --workspace`, `cargo clippy --all-targets` 통과.
- 기존 `save_user_roundtrip_matches_python_delta_policy` 등 settings 관련 테스트 그대로 통과.
- V-Archive ID를 새로 입력하고 저장하면 재시작 없이 백그라운드에서 조회가 1회 트리거됨을
  로그(`[VArchiveClient] 기록 요청 중...`)로 확인.
- account.txt 미설정 상태에서 "동기화 후보 찾기" 버튼이 비활성화되는지 확인.

### 참고: 구체 스니펫
아래는 참고용 구현 스케치이다. 실제 파일과 diff가 다를 수 있으니 그대로 붙여넣지 말고
현재 파일 내용에 맞춰 적용한다.

```rust
// settings_ui.rs
fn varchive_tab(ui: &mut egui::Ui, draft: &mut Value, ctx: &SettingsUiContext) {
    section_frame(ui, crate::t!("settings-varchive-connect"), |ui| {
        form_row(ui, crate::t!("settings-link-status"), |ui| {
            ui.label(RichText::new(current_steam_label(ctx))
                .color(Theme::TEXT_MUTED).size(Theme::FONT_SMALL));
        });
        if ctx.current_steam_id.is_empty() {
            ui.add_space(8.0);
            ui.label(RichText::new(crate::t!("settings-no-steam-account"))
                .color(Theme::WARN).size(Theme::FONT_SMALL));
            return;
        }
        ui.add_space(Theme::ROW_SPACING);
        v_archive_id_row(ui, draft, ctx);
    });

    if ctx.current_steam_id.is_empty() { return; }
    ui.add_space(16.0);

    section_frame(ui, crate::t!("settings-varchive-upload"), |ui| {
        ui.label(RichText::new(crate::t!("settings-varchive-upload-desc"))
            .color(Theme::TEXT_MUTED).size(Theme::FONT_SMALL));
        ui.add_space(Theme::ROW_SPACING);
        account_path_row(ui, draft, ctx);
        ui.add_space(20.0);
        scan_candidates_row(ui, draft, ctx);
    });
}
```

```rust
// v_archive_id_row: "새로고침" 버튼 1개 + ID 입력. 버튼은 id가 비어있지 않을 때만 활성화.
// 클릭 시 ctx.fetch_tx.send((steam_id, v_id, 0)) — button=0은 4모드 전체 조회.

// account_path_row: 기존 account.txt Browse 로직 그대로 이동.

// scan_candidates_row: draft에서 account_path 존재 여부를 읽어
// add_enabled_ui(has_account, ...)로 스캔 버튼을 감싸고,
// !has_account일 때 settings-account-path-required 문구 출력.
```

```rust
// native_app_viewports.rs — 저장 버튼 핸들러
if ui.add(save_btn).clicked() {
    let base_g = overmax_core::lock_clone_or_default(&base);
    let mut merged_g = overmax_core::lock_clone_or_default(&merged);
    let prev_v_id = varchive_v_id(&merged_g, &settings_ctx.current_steam_id);
    let _ = settings_ui::save_settings_to_disk(
        root.as_ref(), defaults.as_ref(), &base_g, &mut local_draft, &mut merged_g,
    );
    crate::ui::i18n::set_locale_from_settings(&merged_g);

    let new_v_id = varchive_v_id(&merged_g, &settings_ctx.current_steam_id);
    if !new_v_id.is_empty() && new_v_id != prev_v_id {
        let _ = settings_ctx.fetch_tx.send((settings_ctx.current_steam_id.clone(), new_v_id, 0));
    }
    if let Ok(mut m) = merged.lock() { *m = merged_g; }
}

fn varchive_v_id(settings: &Value, steam_id: &str) -> String {
    settings.get("varchive").and_then(|v| v.get("user_map"))
        .and_then(|m| m.get(steam_id)).and_then(|e| e.get("v_id"))
        .and_then(Value::as_str).unwrap_or("").trim().to_string()
}
```

---

## Phase 2 — 탭/섹션 정보 구조(IA) 재편

### 목표
탭 3개(UI / V-Archive / System) → 탭 4개(**일반 / 추천 / V-Archive / 고급**)로 재편하여
"자주 쓰는 것"과 "가끔/거의 안 쓰는 것"을 분리한다.

| 새 탭 | 담을 섹션 | 현재 위치 |
|---|---|---|
| 일반 | 언어, 오버레이(크기/투명도/스냅위치), 라이트모드, 항상표시 | `ui_tab()`의 `overlay_section` + `system_tab()`의 `general_section` |
| 추천 | 스마트 추천 on/off, 외부 Provider 연동 | `ui_tab()`의 `recommend_section` + `system_tab()`의 `recommend_provider_section` |
| V-Archive | Phase 1 결과 그대로 | 변경 없음 |
| 고급 | 디버그창, 캡처 엔진/보호, 자동 업데이트, (Linux) 바로가기 | `system_tab()`의 나머지 |

### 해야 할 것
- `render_settings_form()`의 탭 라벨 배열을
  `["일반", "추천", "V-Archive", "고급"]` (i18n 키로) 로 바꾸고 `active` 분기를 4개로 늘린다.
- 각 섹션 함수(`overlay_section`, `recommend_section`, `recommend_provider_section`,
  `general_section`, `debug_section`, `capture_section`, `update_section`) **내부 로직은
  그대로 두고**, 이들을 호출하는 탭 함수만 새로 구성한다 (`general_tab`, `recommend_tab`,
  `advanced_tab`).
- 탭 이름 i18n 키 추가: `settings-tab-general`, `settings-tab-recommend`,
  `settings-tab-varchive`, `settings-tab-advanced`.

### 하지 말아야 할 것
- 섹션 함수 내부(값을 읽고 쓰는 로직)를 재작성하지 않는다. "어느 탭에서 어느 함수를
  호출하느냐"만 바뀌는 순수 재배치여야 한다.
- draft JSON 스키마 변경 금지.
- Phase 1에서 만든 V-Archive 탭 구조를 다시 건드리지 않는다.

### 완료 기준
- `cargo build`/`clippy` 통과.
- 탭을 전환해도 각 필드 값이 유지되는지(=같은 `draft` 참조를 계속 쓰는지) 확인.
- 기존 `render_pill_tabs` 컴포넌트를 그대로 재사용했는지 확인 (새 탭 위젯 만들지 않기).

---

## Phase 3 — 설명력 보강

### 목표
hover하지 않아도 각 섹션/컨트롤이 무엇을 하는지 알 수 있게 하고,
"즉시 적용"과 "저장 후 적용"을 구분해서 보여준다.

### 해야 할 것
- `form_row()` 옆에 짧은 설명을 붙일 수 있는 `form_row_with_hint(ui, label, hint, add_contents)`
  헬퍼를 추가한다 (`TEXT_MUTED`, `FONT_TINY` 톤으로 라벨 아래 1줄).
- 오버레이 크기/투명도처럼 저장 없이 실시간 반영되는 항목에는 짧은 "즉시 적용" 뱃지를,
  나머지(캡처 엔진, 언어 등 저장 버튼을 눌러야 반영되는 항목)에는 아무 표시도 하지 않거나
  "저장 후 적용" 뱃지를 일관되게 붙인다. 어떤 항목이 즉시 반영되는지는 현재 코드에서
  `ui.ctx().request_repaint_of(...)`를 즉시 호출하는지 여부로 판별한다.
- 이미 hover tooltip이 달려있는 항목(`settings-smart-recommend-desc` 등)은 tooltip을
  유지하면서, 그 요약을 인라인 1줄로도 추가한다.

### 하지 말아야 할 것
- 기존 `.on_hover_text(...)` 호출을 제거하지 않는다 (추가이지 대체가 아님).
- 새로운 rich-text/마크다운 렌더링 라이브러리를 도입하지 않는다. `RichText`/`Label`만 쓴다.
- 모든 항목에 장문 설명을 넣지 않는다 — 한 줄로 요약되지 않는 설명은 tooltip에만 남긴다.

### 완료 기준
- 모든 `section_frame` 아래 최소 1개 섹션 설명 또는 각 주요 컨트롤에 인라인 힌트 존재.
- 즉시 적용/저장 필요 표시가 실제 동작과 일치하는지 수동 확인.

---

## Phase 4 — 비주얼 톤 통일

### 목표
오버레이 스냅 위치 위젯(`overlay_section`의 미니맵 버튼)처럼 시각적으로 잘 만들어진
컨트롤 수준으로 나머지 컨트롤(특히 캡처 엔진 선택 콤보박스)의 완성도를 맞춘다.

### 해야 할 것
- 캡처 엔진 선택(`egui::ComboBox`)을 S/M/L/XL 크기 선택처럼 버튼형 토글 UI로 교체하는
  것을 검토한다 (기존 `overlay_section`의 크기 버튼 스타일 재사용).
- 카드(Frame) 여백, corner_radius, 강조색 사용이 섹션마다 일관되는지 점검하고
  어긋나는 곳을 `Theme::` 상수 기준으로 맞춘다.

### 하지 말아야 할 것
- `Theme` 구조체에 새 색상을 추가하는 것은 최소화한다. 기존 팔레트로 해결되지 않는
  경우에만 추가하고, 추가 시 왜 필요한지 남긴다.
- 애니메이션, 커스텀 렌더링, 새 egui 확장 크레이트 도입 금지 — 기존 egui 기본 위젯
  범위 내에서만 작업한다.

### 완료 기준
- 변경 전/후 스크린샷 비교로 카드/버튼 스타일 일관성 확인.
- `cargo build`/`clippy` 통과.

---

## 진행 순서 체크리스트

- [ ] Phase 1: V-Archive 탭 재구성 + 자동 조회
- [ ] Phase 2: 탭/섹션 IA 재편 (4탭)
- [ ] Phase 3: 설명력 보강
- [ ] Phase 4: 비주얼 톤 통일

각 Phase 완료 후 diff를 사용자에게 보여주고 확인받은 다음 다음 Phase로 진행한다.
