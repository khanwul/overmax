# app 경계 정리 — 비-UI 책임 목록과 목적지 (TASKS 11.1)

> 상태: 11.1 조사 결과. 이동 작업(11.2·11.3)의 입력이며, 이동 커밋은 이 문서의 분류를 따른다.
> 관련 결정: [data_and_sync.md](../decisions/data_and_sync.md) (런타임 허브), [linux_support.md](../decisions/linux_support.md) (창 geometry)

---

## 1. 목적지 규칙

| 목적지 | 담는 것 | 판단 기준 |
|:---|:---|:---|
| `overmax_engine` | 감지·캡처·창 추적 | 게임 화면/창을 관찰하는 코드 |
| `overmax_data` | 저장·계산·외부 API·설정/계정 파일 | 입력이 같으면 결과가 같은 데이터 처리 |
| `overmax_runtime` (신규) | 런타임 허브, 관찰자(IPC·자동 업로드), 동기화/업로드 작업 조율 | engine 출력과 data를 엮는 흐름. engine·data 어느 쪽에 넣어도 역방향/과잉 의존이 생김 |
| `overmax_app` 유지 | View/Settings/트레이/다이얼로그 + 앱 프로세스 관리(업데이터, 단일 인스턴스, 데스크톱 엔트리) | 사용자가 직접 닿거나, 이 실행 파일의 배포·수명에만 의미가 있는 것 |

- 신규 crate는 `overmax_runtime` 하나만 만든다. 의존 방향: `app → runtime → engine/data → core`. runtime은 egui/eframe에 의존하지 않는다.
- 런타임이 UI를 깨워야 하는 곳(현재 `egui::Context::request_repaint`)은 콜백(`Fn() + Send + Sync`)으로 주입한다.
- **목표 정정**: "app에는 View/Settings만"에서 앱 프로세스 관리(업데이터 등)는 예외로 app에 남긴다. 다른 프론트엔드가 재사용할 기능이 아니라 이 배포물 자체의 수명 관리이기 때문.

---

## 2. 현재 상태 요약

- 창 추적의 핵심은 **이미 engine에 있음**: `overmax_engine::capture::window_tracker` (`WindowTracker` 플랫폼별 구현, `WindowRect`, `PresentationObservation`). 단, trait 없이 `cfg`로 갈리는 구체 타입이다.
- app의 비-UI 코드 대부분은 egui를 쓰지 않는다 (`system/` 전체 egui 참조 0, `ui/auto_upload.rs`는 주석 1건뿐).
- 가장 큰 비-UI 덩어리는 `ui/native_app.rs`의 작업 조율 메서드들과 `ui/native_app_recommend.rs`의 허브 루프다. 이들은 egui를 `request_repaint`용으로만 쓴다.

---

## 3. 항목별 분류

### 3.1 창 추적 (→ 11.2)

| 위치 | 내용 | 목적지 | 비고 |
|:---|:---|:---|:---|
| `ui/linux_layer_overlay.rs` `ForeignToplevel*`, `parse_foreign_toplevel_states`, `unique_matching_toplevel` | wlr-foreign-toplevel로 게임 창 활성/전체화면/출력 관찰 → `PresentationObservation` 발행 | engine `window_tracker` (trait 구현) | **결합 주의**: 오버레이와 같은 Wayland 연결·`wl_output` 객체를 공유함. 분리하려면 별도 연결을 열거나 출력 식별을 이름 기반으로 바꿔야 함 → 11.2에서 설계 필요 |
| `ui/linux_layer_overlay.rs` `select_output_by_overlap`, `output_geometry` | 게임 rect와 출력 겹침으로 대상 출력 선택 | app 유지 | 오버레이 배치 로직. 좌표 입력만 받으면 됨 |
| `ui/native_app_viewports.rs` `determine_active_state` (Windows) | `GetForegroundWindow`/PID 비교로 게임 활성 판정 | engine `window_tracker` | engine `WindowTracker::is_foreground`와 중복 여부 확인 후 통합 |
| `ui/native_app_viewports.rs` `find_hwnd_by_title` 호출부 | 게임 hwnd 탐색 | engine (이미 있음) | 호출만 runtime/engine 쪽으로 |
| `ui/platform/windows.rs` `GetWindowRect`/`SetWindowPos` | **오버레이 자신의** 드래그 이동 | app 유지 | 창 추적 아님 |

### 3.2 런타임 허브와 작업 조율 (→ 11.3)

| 위치 | 내용 | 목적지 |
|:---|:---|:---|
| `ui/native_app_recommend.rs` 감지 출력 처리 루프 | 세션 추적·PB 판정·기록 저장·추천·IPC 발행 | runtime (`Runtime`, 이후 커밋 체인 + 관찰자로 분리) |
| `ui/native_app.rs` `spawn_scan`/`drain_sync_scan`/`poll_scan_requests` | V-Archive 동기화 후보 스캔 | runtime |
| `ui/native_app.rs` `spawn_upload`/`drain_upload_results`/`upload_pattern`/`sync_varchive_cache_after_upload` | 업로드 실행·결과 처리·업로드 기록 저장 | runtime (HTTP 자체는 이미 data) |
| `ui/native_app.rs` `tick_auto_upload`/`pattern_needs_upload` + `ui/auto_upload.rs` | 자동 업로드 스케줄링 | runtime (관찰자) |
| `ui/native_app.rs` `spawn_fetch`/`drain_fetch_results`/`poll_startup_cache`/`handle_auto_refresh` | V-Archive 캐시 갱신·시작 시 캐시 | runtime |
| `ui/native_app.rs` `poll_delete_requests` | 기록 삭제 요청 처리 | runtime |
| `ui/native_app.rs` `refresh_steam_session`/`drain_game_found_refresh_steam` | 게임 발견 시 Steam 계정 갱신 | runtime (판독 자체는 data) |

### 3.3 외부 연동·시스템

| 위치 | 내용 | 목적지 | 비고 |
|:---|:---|:---|:---|
| `system/ipc_server.rs` | IPC 도메인 디스패처 | runtime | 허브의 관찰자가 됨 |
| `system/transport/` | std-only HTTP/SSE 엔진 (도메인 로직 0) | runtime 내부 모듈 | 독립 crate로 뺄 만큼의 재사용 수요는 아직 없음 |
| `system/steam_session*` | `loginusers.vdf` 판독 | data | 계정 파일 판독 = 데이터 계층 |
| `system/native_helpers.rs` `first_steam_from_settings`, `account_path_for_steam`, 모드 변환 | 계정/설정 헬퍼 | data | `vp_*` ViewportId 헬퍼는 app 유지 (파일 분할 필요) |
| `system/settings_writer.rs` | 디바운스 원자적 설정 저장 | data | 저장 함수는 이미 data에 있음 |
| `system/updater/` | 자가 업데이트 | app 유지 | 앱 프로세스 관리 |
| `system/single_instance*` | 단일 인스턴스 | app 유지 | 앱 프로세스 관리 |
| `system/desktop_entry_linux.rs` | `.desktop` 설치 | app 유지 | 앱 프로세스 관리 |

### 3.4 app 유지 (표현 계층)

`ui/overlay_ui.rs`, `ui/overlay_recommend_ui.rs`, `ui/settings_ui.rs`, `ui/sync_ui.rs`(필터는 이미 data의 `matches_filter`), `ui/upload_history_ui.rs`, `ui/debug_ui.rs`, `ui/tray_icon.rs`, `ui/i18n.rs`, `ui/components/`, `ui/platform/`, `ui/native_app_viewports.rs`(창 추적부 제외), `ui/linux_layer_overlay.rs`(foreign-toplevel 관찰부 제외), 테마 파일들.

---

## 4. 이동 순서 (11.2 → 11.3)

1. **11.2 창 추적**: engine `window_tracker`에 trait 경계 도입 → Windows 활성 판정 통합 → Linux foreign-toplevel 관찰 분리 (Wayland 연결 공유 문제 설계 선행)
2. **11.3 하위 crate 이동** (각 1커밋, 동작 불변):
   1. `overmax_runtime` 빈 crate 생성
   2. data로: `steam_session` → `settings_writer` → `native_helpers` 계정 헬퍼
   3. runtime으로: `transport` → `ipc_server` → `auto_upload`
   4. 작업 조율 메서드(스캔·업로드·캐시·삭제)를 runtime 서비스로
   5. 런타임 허브 재설계 1~3단계 (TASKS 11.3 하위 항목)

## 5. 열린 질문

- Linux foreign-toplevel 관찰을 engine으로 옮길 때 Wayland 연결을 별도로 열지, 오버레이 연결을 공유하는 어댑터를 둘지 (11.2 설계 항목)
- `transport`를 runtime 내부 모듈로 둘지 독립 crate로 둘지 — 현재는 내부 모듈, 외부 재사용 수요가 생기면 재검토
