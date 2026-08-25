# Complexity Reduction Refactoring Plan

> 작성일자: 2026-08-25
> 브랜치: `refactor/complexity-reduction`
> 배경: 코드베이스 복잡도 견적(33,500줄 기준)에서 도출된 핫스팟의 단계적 정리

---

## 1. 개요 및 목표

거대 단일 함수와 복붙 성장 구조를 정리해, 향후 기능 추가(특히 래더매치 감지) 시
사본이 늘어나는 것을 원천 차단한다. **동작 변화는 없어야 한다** — 모든 단계는
컴파일·테스트·clippy 통과 및 온디바이스 UI 확인을 전제로 한다.

## 2. 대상 및 우선순위 (견적 요약)

| 순위 | 대상 | 공수 | 위험 |
|---|---|---|---|
| 1 | `sync_ui.rs::render_sync` (454줄 단일 함수) | 소 | 낮음 |
| 1 | `native_app.rs` 초기화(`new` 387줄) + `spawn_upload`(334줄) | 소 | 중 (채널 와이어링) |
| 2 | `detection_pipeline.rs` 씬 감지 3형제 (`detect_*_scene_via_edge`) | 중 | 높음 (verified pipeline) |
| 2 | `record_db.rs` (1,586줄 / fn 40개) 책임 분리 | 중 | 낮음~중 |

### 제외 (건드리지 않음)
- `recommend/tests.rs` — 테스트 자산
- `i18n.rs` — 0-cost 매크로 SSOT, 의도된 아키텍처
- `templates/digit.rs` — 데이터 파일
- `linux_layer_overlay.rs` — Wayland 특성상 단일 파일이 자연스러움

## 3. 단계별 실행 계획

- [x] **Step 1: `sync_ui.rs::render_sync` 분리**
  - `steam_account_card`, `filter_card`(+`render_filter_form`), `candidates_header`,
    `sort_candidates`(순수 함수)로 추출. `render_sync`는 ~60줄 오케스트레이터.
  - ⚠️ 사고 기록: 치환 스크립트가 ScrollArea 내부 닫는 괄호를 본문 끝으로 오인해
    리스트 이중 렌더링 발생 → `fe761a1`에서 복원. 교훈: 대형 블록 이동 시
    앵커 경계 검증 + 렌더 구조 사람 확인 단계 필수.
- [x] **Step 2: `native_app.rs` 초기화/업로드 플로우 정리**
  - mpsc 채널 번들을 값 객체(`SyncChannels` 등 이미 일부 존재하는지 재확인)로 묶고,
    `spawn_upload`의 후보 탐색·페이로드 조립·쓰레드 스폰을 헬퍼로 분리.
  - 채널 연결 관계는 기능 변경 없이 이동만 한다.
- [x] **Step 3: `detection_pipeline.rs` 씬 감지 게이트 체인 추상화**
  - `detect_result/freestyle/openmatch_scene_via_edge`의 공통 구조
    (ROI → centroid 게이트 → band 게이트 → 자켓 매칭)을 `SceneGate` 체인으로 추상화,
    씬별 차이만 설정으로 주입. `SceneMissDiag` 진단은 체인에서 자동 수집.
  - 래더매치 감지(6.1) 착수 전 완료가 목표 — 사본 3번째 탄생 방지.
  - 각 단계 커밋마다 기존 테스트 + 새 회귀 테스트 유지.
- [x] **Step 4: `record_db.rs` 책임 분리** (선택 — Step 1~3 완료 후 재평가)

  - 적용 구조: `store/record_db/` 디렉터리 모듈(mod: 구조체+CRUD 코어 / schema / queries / sync).
    자식 모듈은 부모 private 항목에 접근 가능하므로 가시성 변경 없이 public API 유지.
## 4. 검증 기준

- 매 단계: `cargo test --workspace` 전체 통과, `cargo clippy --all-targets` 경고 0
- Step 1~2: 동기화 창/설정창 온디바이스 UI 확인 (렌더 구조 변화 없음)
- Step 3: 씬 감지 로그(SleepHint/미스 진단 스냅샷)가 리팩터링 전과 동일 패턴

## 5. 원칙

- verified pipeline 보호: detection 계열은 계측 데이터로 회귀 여부를 먼저 확인한다.
- 땜질 금지: 구조 개선 목적의 이동 외에 로직 변경을 섞지 않는다.
- 문서화: 설계 결정 발생 시 `docs/decisions/<domain>.md`에 즉시 기록한다.
