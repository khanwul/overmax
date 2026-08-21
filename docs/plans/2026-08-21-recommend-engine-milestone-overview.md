# 추천 엔진 고도화 — 마일스톤 개요

**작성일**: 2026-08-21
**목적**: TASKS.md의 "추천 기능 고도화" 백로그 항목을 실행 가능한 단계로 분해한 전체 조망.
개별 단계의 상세 스펙은 별도 문서로 분리하며, 이 문서는 단계 간 관계와 현재 상태만 추적한다.

---

## 배경

기존 `LocalFloorRecommender`는 floor(비공식 난이도) 근접 곡을 "낮은 rate 우선 → 미플레이 후순위"로만
추천했다. Overmax가 결과창을 실시간으로 인식해 `VerifiedPlayEvent`를 방출하고, V-Archive 연동 시
레이팅/기록 최근성 데이터까지 이미 갖고 있는 만큼, 이 데이터를 추천 신호로 더 쓰기로 했다.

**용어 정정 한 가지**: 설계 초기에 "DJPOWER 기반"으로 논의했으나, 공식 DJ POWER(시그모이드, 97~99% 구간
급상승)와 Overmax가 실제로 동기화하는 `varchive_records.rating`(V-Archive 자체 지수형 레이팅)은
서로 다른 곡선의 별개 시스템이다. 이후 모든 문서에서 후자를 **"V-Archive 레이팅"**으로만 지칭한다.

---

## 로드맵

| 단계 | 내용 | 건드리는 크레이트 | 선행조건 | 상태 | 문서 |
|---|---|---|---|---|---|
| 선행 작업 | `play_events` 이력 로그 스키마 | overmax_data | 없음 | 별도 진행 중 | (별도 트래킹) |
| Phase 1 | 레인 B(정체/재도전) — rate 격차 × 최근성 가중 블렌딩 | overmax_data | 없음 (스키마 변경 없음) | 완료 | `2026-08-21-recommend-phase1-retry-lane-spec.md` |
| Phase 2 | 레인 A(V-Archive 레이팅 top-50 경계 후보) | overmax_data | `get_varchive_top50_rank` 확장 | 완료 | `2026-08-21-recommend-phase2-top50-boundary-spec.md` |
| Phase 3 | 레인 D(세션 모멘텀 & 최근 플레이 히스토리 기반 동적 플로우) | overmax_data | Phase 1 이후 권장 | 스펙 완료, 착수 대기 | `2026-08-21-recommend-phase3-session-flow-spec.md` |
| Phase 4 | 위 세 레인 + play_events 기반 신호 통합, "이유 배지" UI 표시 | overmax_data, overmax_app | 선행 작업 완료 + Phase 1~3 | 착수 전 | - |

Phase 1~3은 서로 독립적으로 구현 가능하고(각자 다른 데이터 소스 사용), 순서를 바꿔도 무방하다.
다만 UI 노출(이유 배지, 크레이트 경계를 넘는 변경)은 전부 Phase 4로 미뤄, 각 단계는
`overmax_data` 크레이트 내부에서만 저위험으로 끝나도록 설계했다.

---

## 공통 설계 원칙

- **단일 리스트로 블렌딩**: 오버레이 UI가 6줄 고정 슬롯이라 레인별 탭 UI는 만들지 않는다.
  `final_score = w_A·A + w_B·B + w_C·C + w_D·D` 형태로 합산해 기존 `LocalFloorRecommender`
  결과 리스트 하나에 얹는다. `RecommendEntry.score` 필드(현재 외부 provider 전용)를 재사용할 수 있다.
- **명시적 `now` 주입**: 시간 기반 가중치는 전부 `SystemTime::now()`를 함수 내부에서 부르지 않고
  파라미터로 받는 순수 함수로 분리한다 (디텍션 파이프라인의 기존 관례와 동일 — 테스트 결정론성 확보).
- **가중치는 이름 붙은 상수로**: `jacket_matcher.rs`의 `match_score` 모듈처럼, 매직넘버 대신
  네이밍된 `const`로 분리해서 실사용 피드백만으로 튜닝 가능하게 한다.
- **크레이트 경계 최소 침범**: Phase 1~3은 `overmax_data` 단일 크레이트 내에서 끝낸다.
  UI/표시 관련 변경은 Phase 4로 모아 별도 스펙·별도 PR로 처리한다.

---

## 다음 액션

1. Phase 1(레인 B) 완료 (`a55b79d`).
2. Phase 2(레인 A) 완료.
3. Phase 3(레인 D) 스펙 작성 및 착수.
4. 선행 작업(`play_events`) 완료 시점에 맞춰 Phase 4 스펙 작성 착수.
