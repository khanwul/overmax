# Overmax 추천 엔진 확장 — Phase 2 작업 지시서

**대상 레인**: 레인 A(V-Archive 레이팅 Top-50 경계 후보) 를 기존 추천 파이프라인(`LocalFloorRecommender`)에 블렌딩  
**대상 크레이트**: `overmax_data` 단일 크레이트 (UI, engine, core 미변경)  
**선행 조건**: Phase 1 완료 (`a55b79d`)

---

## 0. 배경

V-Archive를 사용하는 플레이어의 주된 동기 중 하나는 **"Top-50 레이팅 올리기"**이다.
V-Archive의 모드별 레이팅은 플레이한 전체 곡 중 레이팅 상위 50곡의 합산으로 결정된다.

실제 V-Archive 기록(`cache/varchive/...`) 데이터를 분석한 결과:
- 50위 컷라인 레이팅(`cutoff_rating`) 부근(40위~50위, 51위~60위)의 레이팅 격차는 불과 **0.5 ~ 1.5점** 내외로 매우 촘촘하다.
  - 4B 기준: 1위(185.85) ~ 40위(173.78) ~ 50위(172.96) ~ 51위(172.85) ~ 60위(172.03)
  - 6B 기준: 1위(185.77) ~ 40위(178.11) ~ 50위(177.50) ~ 51위(177.41) ~ 60위(176.67)
- 따라서 다음 두 부류의 곡이 가장 높은 레이팅 상승 유인을 갖는다:
  1. **수성/안정화 타깃 (41위~50위)**: 현재 Top-50의 하위권에 있어 곧 밀려날 위험이 있는 곡. 조금만 점수를 개선해도 Top-50 내 순위를 방어할 수 있다.
  2. **돌파/갱신 타깃 (51위~60위 또는 컷라인 -2.0 이내)**: 컷라인 바로 아래에 위치하여, 약간의 점수 향상(예: 99.0% -> 99.5%)만으로 즉시 50위 권내로 진입해 전체 레이팅을 끌어올릴 수 있는 곡.

**V-Archive 미연동 사용자 폴백 원칙**:
- V-Archive 미연동 사용자나 기록이 50개 미만인 경우, 레인 A 점수는 `0.0`으로 계산되어 기존 **레인 B(재도전) + 레인 C(Floor 근접)** 로직만으로 100% 자연스럽게 폴백한다.

---

## 1. 스코프

### In-Scope (이번 작업)
- `RecordDB`에 `idx_varchive_top50` 복합 인덱스 추가 (쿼리 성능 최적화)
- `RecordDB`에 모드별 Top-50 요약 정보(`VArchiveTop50Summary`: 컷라인 레이팅, 상위 50곡 순위 맵) 조회 메서드 추가
- `RecordDB`에 후보 곡들의 V-Archive `rating` 배치 조회 메서드(`get_varchive_rating_map`) 추가
- `RecordManager`에 pass-through 메서드 2종 추가
- `top50_boundary_score()` 모듈 레벨 순수 함수 추가 및 단위 테스트
- `LocalFloorRecommender::recommend()`에서 레인 A 점수 블렌딩 가중치 합산 및 최종 정렬 반영
- 단위 테스트 추가 및 전체 워크스페이스 검증

### Out-of-Scope (건드리지 말 것 / 후속 마일스톤)
- 레인 D (결과창 직후 `sheet_meta` 태그 기반 후속 추천 — Phase 3)
- UI 배지 표시 (Top-50 경계 배지 등 — Phase 4)
- DB 테이블 컬럼 구조 변경 (`varchive_records` 기존 스키마 유지, 인덱스만 추가)

---

## 2. 상세 설계

### 2.1 복합 인덱스 추가 (`record_db.rs`)

`create_varchive_records_table` 및 `ensure_schema`에 Top-50 쿼리 전용 복합 인덱스를 추가한다:

```sql
CREATE INDEX IF NOT EXISTS idx_varchive_top50 ON varchive_records (steam_id, button_mode, rating DESC);
```

### 2.2 `VArchiveTop50Summary` 및 `RecordDB` 조회 메서드

**파일**: `rust/overmax_data/src/store/record_db.rs`

```rust
#[derive(Debug, Clone, Default, PartialEq)]
pub struct VArchiveTop50Summary {
    /// 50위 곡의 레이팅 (50개 미만인 경우 가장 낮은 곡의 레이팅 또는 0.0)
    pub cutoff_rating: f64,
    /// 1위 ~ 50위 곡들의 순위 맵 (RecordKey -> 1-based rank)
    pub rank_map: std::collections::HashMap<RecordKey, usize>,
    /// 모드 내 등록된 유효 레이팅(rating > 0) 곡 수
    pub total_recorded_count: usize,
}

impl RecordDB {
    pub fn get_varchive_top50_summary(
        &self,
        steam_id: &str,
        mode: Mode,
    ) -> VArchiveTop50Summary {
        if !self.is_ready || steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return VArchiveTop50Summary::default();
        }

        let Ok(conn) = self.open_conn() else {
            return VArchiveTop50Summary::default();
        };

        let button_mode = mode.as_str();
        let query = "SELECT song_id, difficulty, rating
                     FROM varchive_records
                     WHERE steam_id = ?1 AND button_mode = ?2 AND rating > 0
                     ORDER BY rating DESC
                     LIMIT 50";

        let mut rank_map = std::collections::HashMap::new();
        let mut cutoff_rating = 0.0f64;
        let mut total_count = 0usize;

        if let Ok(mut stmt) = conn.prepare(query) {
            if let Ok(mut rows) = stmt.query(rusqlite::params![steam_id, button_mode]) {
                let mut rank = 1;
                while let Ok(Some(row)) = rows.next() {
                    if let (Ok(song_id_str), Ok(diff_str), Ok(rating)) = (
                        row.get::<_, String>(0),
                        row.get::<_, String>(1),
                        row.get::<_, f64>(2),
                    ) {
                        if let (Ok(sid), Some(diff)) = (
                            song_id_str.parse::<i32>(),
                            Difficulty::from_str(&diff_str),
                        ) {
                            rank_map.insert((sid, mode, diff), rank);
                            cutoff_rating = rating;
                            total_count += 1;
                            rank += 1;
                        }
                    }
                }
            }
        }

        VArchiveTop50Summary {
            cutoff_rating,
            rank_map,
            total_recorded_count: total_count,
        }
    }

    pub fn get_varchive_rating_map(
        &self,
        song_ids: &[i32],
    ) -> std::collections::HashMap<RecordKey, f64> {
        if !self.is_ready || song_ids.is_empty() {
            return std::collections::HashMap::new();
        }

        let steam_id = self.get_steam_id();
        if steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return std::collections::HashMap::new();
        }

        let placeholders = vec!["?"; song_ids.len()].join(",");
        let query = format!(
            "SELECT song_id, button_mode, difficulty, rating
             FROM varchive_records
             WHERE steam_id=?1 AND song_id IN ({}) AND rating > 0",
            placeholders
        );

        let mut map = std::collections::HashMap::new();
        let _ = self.with_rate_map_connection(|conn| {
            if let Ok(mut stmt) = conn.prepare(&query) {
                let mut p = Vec::new();
                p.push(&steam_id as &dyn rusqlite::ToSql);
                let song_ids_str: Vec<String> = song_ids.iter().map(|s| s.to_string()).collect();
                for id_str in &song_ids_str {
                    p.push(id_str as &dyn rusqlite::ToSql);
                }
                if let Ok(mut rows) = stmt.query(&*p) {
                    while let Ok(Some(row)) = rows.next() {
                        if let (Ok(song_id_str), Ok(button_mode), Ok(difficulty), Ok(rating)) = (
                            row.get::<_, String>(0),
                            row.get::<_, String>(1),
                            row.get::<_, String>(2),
                            row.get::<_, f64>(3),
                        ) {
                            if let Ok(sid) = song_id_str.parse::<i32>() {
                                if let (Some(m), Some(d)) = (
                                    Mode::from_str(&button_mode),
                                    Difficulty::from_str(&difficulty),
                                ) {
                                    map.insert((sid, m, d), rating);
                                }
                            }
                        }
                    }
                }
            }
        });
        map
    }
}
```

### 2.3 `RecordManager` pass-through

**파일**: `rust/overmax_data/src/service/record_manager.rs`

```rust
impl RecordManager {
    pub fn get_varchive_top50_summary(&self, mode: Mode) -> VArchiveTop50Summary {
        let steam_id = self.record_db.get_steam_id();
        self.record_db.get_varchive_top50_summary(&steam_id, mode)
    }

    pub fn get_varchive_rating_map(
        &self,
        song_ids: &[i32],
    ) -> std::collections::HashMap<RecordKey, f64> {
        self.record_db.get_varchive_rating_map(song_ids)
    }
}
```

### 2.4 Top-50 경계 가중치 순수 함수 (`top50_boundary_score`)

**파일**: `rust/overmax_data/src/service/recommend.rs`

- **상수 정의**:
  - `TOP50_DEFENSE_RANK_START: usize = 41;` (41위부터 50위까지 방어 대상)
  - `TOP50_ATTACK_RATING_DELTA: f64 = 2.0;` (컷라인 -2.0 이내 곡을 돌파 대상으로 선정)
  - `TOP50_DEFENSE_BONUS: f64 = 5.0;` (41~50위 수성 보너스 기준점)
  - `TOP50_ATTACK_MAX_BONUS: f64 = 6.0;` (51위 이하 컷라인 최근접 돌파 보너스 최대치)

```rust
/// Top-50 경계 추천 점수 계산 순수 함수.
/// - rank가 Some(41..=50): 컷라인 방어 타깃 (순위가 50위에 가까울수록 높은 보너스 3.0 ~ 5.0)
/// - rank가 None이거나 51위 이상이고 rating이 cutoff_rating - 2.0 이상: 컷라인 돌파 타깃 (0.0 ~ 6.0)
/// - Top-50 데이터가 부족하거나(50개 미만) 무관한 구간: 0.0
fn top50_boundary_score(
    varchive_rating: Option<f64>,
    rank: Option<usize>,
    cutoff_rating: f64,
    total_top_count: usize,
) -> f64 {
    if total_top_count < 50 || cutoff_rating <= 0.0 {
        return 0.0;
    }

    if let Some(r) = rank {
        if (TOP50_DEFENSE_RANK_START..=50).contains(&r) {
            // 41위(3.0) -> 50위(5.0) 로 갈수록 위기감이 크므로 가중치 상승
            let t = (r - TOP50_DEFENSE_RANK_START) as f64 / (50 - TOP50_DEFENSE_RANK_START) as f64;
            return 3.0 + (t * (TOP50_DEFENSE_BONUS - 3.0));
        }
    }

    if let Some(rating) = varchive_rating {
        if rating < cutoff_rating && rating >= (cutoff_rating - TOP50_ATTACK_RATING_DELTA) {
            // 컷라인에 가까울수록 (delta가 0에 가까울수록) 높은 점수
            let delta = cutoff_rating - rating;
            let ratio = 1.0 - (delta / TOP50_ATTACK_RATING_DELTA).clamp(0.0, 1.0);
            return ratio * TOP50_ATTACK_MAX_BONUS;
        }
    }

    0.0
}
```

### 2.5 `LocalFloorRecommender` 정렬에 통합

`LocalFloorRecommender`에서:
1. `RawCandidate`에 `varchive_rating: Option<f64>` 필드 추가 (초기값 `None`)
2. `merge_record_rates()`에서 `self.rdb.get_varchive_rating_map(&unique_ids)` 병합
3. `recommend()` 시작 시 `let top50 = self.rdb.get_varchive_top50_summary(ctx.button_mode);` 조회
4. 후보 정렬 시 복합 우선순위 점수 계산:
   $$\text{priority} = \text{retry\_priority}(\text{rate}, \text{updated\_at}, \text{now\_unix}) + \text{top50\_boundary\_score}(\text{varchive\_rating}, \text{rank}, \text{cutoff}, \text{count})$$
   - V-Archive 연동이 없거나 기록이 50개 미만인 경우 $\text{top50\_boundary\_score} = 0.0$이 되므로 기존 Phase 1 동작과 100% 동일하게 폴백
   - 플레이된 곡 간 정렬: 복합 점수 내림차순 -> `floor` 오름차순

---

## 3. 테스트 계획

1. **`top50_boundary_score` 순수 함수 단위 테스트**:
   - 41위~50위 방어 곡에 대해 순위별 적절한 보너스(3.0 ~ 5.0) 반환 검증
   - 컷라인 직하 51위급 곡 (cutoff - 0.1)에 높은 돌파 보너스 (~5.7) 반환 검증
   - 컷라인에서 멀리 떨어진 곡 (cutoff - 5.0) 및 상위 1~40위 안정권 곡에 0.0 반환 검증
   - Top-50 미만(기록 30개 등) 환경에서 0.0 반환(폴백) 검증
2. **`RecordDB::get_varchive_top50_summary` 및 `get_varchive_rating_map` 단위 테스트**:
   - 50개 이상 mock 레코드 삽입 후 컷라인 및 rank_map 정상 생성 검증
   - V-Archive rating 맵 배치 조회 일치 검증
3. **전체 통합 회귀 검증**:
   - `cargo test --workspace` 전체 통과
   - V-Archive 미연동 사용자 환경에서 Phase 1 동작과 100% 동일하게 동작하는지 검증

---

## 4. Definition of Done

- [ ] `varchive_records` 테이블에 `idx_varchive_top50` 복합 인덱스 추가
- [ ] `VArchiveTop50Summary` 정의 및 `RecordDB::get_varchive_top50_summary` 추가
- [ ] `RecordDB::get_varchive_rating_map` 추가
- [ ] `RecordManager` pass-through 메서드 2종 추가
- [ ] `top50_boundary_score()` 순수 함수 및 단위 테스트 4종 작성
- [ ] `LocalFloorRecommender`에 레인 A 점수 블렌딩 통합
- [ ] `cargo test --workspace --locked` 전체 통과
- [ ] `cargo clippy --workspace --all-targets --locked` 경고 없음
- [ ] `overmax_data` 크레이트 외 파일 변경 없음
