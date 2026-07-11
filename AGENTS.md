# Agent Overview

이 에이전트는 DJMAX RESPECT V 오버레이 기반 추천 시스템의
정확도 개선, 성능 최적화, 안정성 향상을 목표로 한다.

---

# Primary Goals

- 인식 정확도 향상 (song / mode / difficulty / rate)
- 인게임 성능 영향 최소화
- 안정적인 상태 전이 (verified pipeline 유지)

---

# Context Usage Policy

- context.md를 현재 시스템 상태의 단일 source of truth로 사용한다
- context.md에 명시된 제약 조건을 절대 위반하지 않는다
- context.md에 없는 시스템은 존재한다고 가정하지 않는다

---

# Decision Policy

## 성능 vs 정확도

- 인게임 성능 영향이 있는 경우:
  → 정확도보다 성능을 우선한다

- 선곡 화면에서만 실행되는 로직:
  → 정확도 우선

---

## 인식 로직 수정

- 기존 파이프라인 (verified flow)을 깨지 않는 선에서 개선
- 단일 프레임 판단보다 history 기반 접근 우선
- OCR은 fallback 또는 검증 용도로만 사용

---

## 추천 시스템

- 현재 구조 (floor 기반)는 유지
- 새로운 기준 추가 시:
  → 기존 정렬 기준을 깨지 않도록 보완 방식으로 적용

---

# Key Constraints (핵심 제약 및 절대 금지 사항)

- **메모리 접근 및 프로세스 인젝션 금지**: 화면 캡처 및 Win32 API 추적 방식만 사용해야 한다.
- **성능 저하 야기 금지 (최우선)**: 특히 OCR 1-Pass를 강제하며, 다중 패스 루프 생성을 절대 금지한다.
- **대규모 재작성 및 임의 리팩토링 금지**: 강력한 이유 없이는 작동 중인 코드를 재작성하거나 관련 없는 주변 코드를 리팩토링하지 않는다.
- **절대 경로 사용 금지**: 문서와 코드에서 절대경로(예: `D:\dev\...`) 사용을 금지하며, 항상 프로젝트 루트 기준 상대경로를 사용한다.
- **기존 호환성 파괴 금지**: 사용자 설정(`settings.user.json`) 및 DB 구조 등 기존 사용자 파일과의 호환성을 유지해야 한다.

---

# Failure Handling

- 확실하지 않은 경우:
  → 결과를 보류하거나 verified=False 유지

- 복수 해석 가능:
  → 조건별로 분리해서 제시

- 정보 부족:
  → 최소 질문만 생성 (1~2개)

---

# Output Format

기술 제안 시 다음 구조를 따른다. 단, 이 5단계 구조는 복잡하거나 트레이드오프가 있는 결정에만 조건부로 적용하며, 단순 버그 fix류는 면제한다.

1. 문제 정의
2. 원인 분석
3. 해결 방법 (옵션별)
4. 트레이드오프
5. 추천안

---

# Prohibited Actions

- 근거 없는 성능 개선 주장 금지
- 전체 리팩토링 제안 금지 (요청 시 제외)
- 기존 파이프라인 무시 금지

---

# Reference Documents (필요시 참조)

- **상세 제약 조건**: 상세한 시스템 스펙 및 제약 조건은 단일 Source of Truth인 [CONTEXT.md](CONTEXT.md)를 필요할 때 참고한다.
- **엔지니어링 취향**: 설계 및 코드 변경 시 소유자의 엔지니어링 취향을 반영하기 위해 [ENGINEERING_TASTE.md](ENGINEERING_TASTE.md)를 필요할 때 참고한다.

---

# Session Handoff Protocol

의미 있는 변경(작업 완료, 제약 조건 변경, 설계 결정 등)이 있었을 때만 세션 종료 직전에 다음을 수행한다:

1. `cargo fmt` 및 `cargo clippy --fix`를 실행하여 코드를 정리하고 경고를 수정한다
2. `TASKS.md`의 완료 항목을 `[x]`로 갱신한다
3. 새로운 제약 조건이나 아키텍처 변경이 있었다면 `CONTEXT.md`를 갱신한다
4. 중요한 설계 결정이 있었다면 Decision Log 요약 행을 추가한다

---

# Quick Reference

## 빌드 & 검증
- 전체 빌드: `cargo build --workspace`
- 테스트: `cargo test --workspace`
- Clippy: `cargo clippy --all-targets`
- 릴리스 빌드: `build.bat`

## 주요 진입점
- 메인 앱: `rust/overmax_app/src/main.rs`
- 디텍션 파이프라인: `rust/overmax_engine/src/detector/detection_pipeline.rs`
- 디텍션 워커: `rust/overmax_engine/src/detector/detection_worker.rs`
- PlayState 감지: `rust/overmax_engine/src/detector/play_state.rs`
- OCR 엔진: `rust/overmax_engine/src/detector/ocr_engine.rs`
- CV 코어: `rust/overmax_cv/src/lib.rs`

## 설정 파일
- 기본 설정: `settings.json`
- 사용자 설정: `settings.user.json` (delta 형식, 기본값과 다른 항목만 저장)
- 곡 DB: `cache/songs.json`
- 기록 DB: `cache/record.db` (SQLite)
- 이미지 인덱스: `cache/image_index.db`