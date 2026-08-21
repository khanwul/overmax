# Local Top-50 Fallback Recommendation Specification

> 작성일자: 2026-08-22  
> 관련 이슈: V-Archive 미연동/오프라인 환경에서의 추천 레벨 및 Top-50 레인 활성화

---

## 1. 개요 및 배경

### 1.1 현상
- 현재 추천 엔진(`LocalFloorRecommender`)은 Top 50 요약(`get_varchive_top50_summary`)을 `varchive_records` 테이블에서만 조회한다.
- V-Archive 연동을 하지 않았거나 오프라인 환경인 유저는 `varchive_records`가 비어있으므로:
  1. 기동 즉시 하단 추천 레벨(`derive_recommended_level`)이 `None`으로 표시됨.
  2. Top-50 수성/돌파 컷라인 추천(레인 A) 혜택을 받지 못함.

### 1.2 개선 방향
- 로컬 SQLite `records` 테이블에 이미 수집된 인게임 플레이 기록(`rate`, `is_max_combo`, `song_id`, `difficulty`)과 자체 Performance Rating 공식(`calculate_performance_rating`)을 결합하여, **공식 V-Archive 기록이 없을 때 로컬 플레이 기록 기반의 Top 50 요약을 실시간으로 산출(Fallback)**한다.

---

## 2. 세부 설계

### 2.1 Fallback 로직 및 우선순위
1. **1순위 (공식 V-Archive 기록)**: `varchive_records`에 해당 버튼 모드 기록이 1건 이상 존재하면 V-Archive 공식 Top 50을 100% 신뢰하여 사용.
2. **2순위 (로컬 기록 Fallback)**: `varchive_records`가 비어있을 때:
   - 로컬 `records` 테이블에서 해당 버튼 모드의 모든 유효 기록(`rate > 0`)을 조회.
   - 각 패턴의 난이도(`Floor`)를 조회하여 `calculate_performance_rating(floor, rate)` 산출.
   - 레이팅 내림차순으로 정렬하여 상위 최대 50개 패턴으로 `VArchiveTop50Summary` 구성.
3. **3순위 (완전 신규 유저)**: `records`도 비어있으면 `VArchiveTop50Summary::default()` (빈 맵) 반환.

### 2.2 성능 가드
- 로컬 `records` 테이블의 버튼당 행 수는 통상 수백 건($N \le 1000$).
- $O(N \log N)$ 정렬 및 레이팅 산출 소요 시간은 $0.05\text{ms}$ 미만으로 인게임 성능 영향 0.

---

## 3. 단계별 실행 계획

- [x] **Step 1: `RecordDB` 및 `RecordManager`에 로컬 기록 기반 Top-50 산출 메서드 구현**
  - `RecordDB::get_local_records_by_mode`: 버튼 모드별 `records` 조회.
  - `RecordManager::get_top50_summary_with_fallback`: V-Archive 우선 + 로컬 Fallback.
- [x] **Step 2: `RecommendStrategy` (`Smart`) 연동**
  - `strategy.rs`의 `sort_and_annotate` 및 `derive_footer_level`에서 `get_top50_summary_with_fallback`을 호출하도록 연동.
- [x] **Step 3: 단위 및 통합 테스트 작성**
  - V-Archive 미연동 상태에서 로컬 `records`만으로 Top 50 요약 및 추천 레벨이 정상 도출되는지 검증.
  - V-Archive 기록 존재 시 공식 기록이 우선하는지 검증.
- [x] **Step 4: 검증 및 문서화**
  - `cargo test --workspace`, `cargo clippy --all-targets` 검증.
  - Decision Log 및 `CONTEXT.md` 동기화.
