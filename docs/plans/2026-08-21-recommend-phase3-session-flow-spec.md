# Overmax 추천 엔진 확장 — Phase 3 작업 지시서

**대상 레인**: 레인 D(세션 모멘텀 & 최근 플레이 히스토리 기반 동적 플로우) 를 기존 추천 파이프라인(`LocalFloorRecommender`)에 블렌딩  
**대상 크레이트**: `overmax_data` 단일 크레이트 (UI, engine, core 미변경)  
**선행 조건**: Phase 1, Phase 2 완료 (`ac0fe02`)

---

## 0. 배경

기존 마일스톤 계획에서는 Phase 3를 `sheet_meta`(공략 시트 태그) 기반 추천으로 구상했으나,
공략 배치 태그(핲랜/맥랜/어시스트 등)는 선곡 후 플레이 시 참고하는 팁에 가까워 선곡 유인 신호로는 체감이 낮다는 결론에 도달했다.

반면 Overmax는 인게임 결과창 인식(`VerifiedPlayEvent`)과 로컬 기록 DB(`records.updated_at`)를 통해
**"플레이어가 방금 어떤 난이도에서 어떤 성과를 냈는지"**를 실시간으로 정확히 추적할 수 있다.

리듬게임 플레이어의 실제 세션 흐름:
1. **상승 모멘텀 (Climbing)**: 직전 판이 개인 세션 평균 대비 $+0.5\%$ 이상이거나 Max Combo를 달성한 경우 $\rightarrow$ 현재 Floor 기준 $+0.1 \sim +0.4$ 상위 난이도 도전곡을 추천하여 성장 동기 부여.
2. **동급 안정화 (Steady)**: 개인 세션 평균 대비 $\pm 1.0\%$ 이내 적정 난이도를 유지 중이거나 세션 첫 판인 경우 $\rightarrow$ 현재 Floor 기준 $\pm 0.15$ 범위의 동급 연습곡을 추천.
3. **피로 회복/쿨다운 (Recovery)**: 개인 세션 평균 대비 $-1.0\%$ 미만으로 고전하거나 폭사한 경우 $\rightarrow$ 현재 Floor 기준 $-0.2 \sim -0.5$ 살짝 낮은 Floor의 손풀기/회복곡을 추천하여 좌절감 완화.
4. **세션 만료/데이터 부재**: 직전 플레이 후 4시간 이상 경과(`SESSION_IDLE_TIMEOUT_HOURS`)했거나 플레이 기록이 없으면 점수 `0.0`으로 자연스럽게 기존 레인(A, B, C)으로 폴백.

---

## 1. 스코프

### In-Scope (이번 작업)
- `RecordDB`에 최근 플레이 쿼리 전용 복합 인덱스 `idx_records_recent` 추가
- `RecordDB`에 `with_rate_map_connection` 스레드 로컬 커넥션을 사용하는 `get_recent_records` 추가
- `RecordManager`에 pass-through 메서드 추가
- `SESSION_IDLE_TIMEOUT_HOURS` 세션 유효 윈도우(4시간) 가드 적용
- `SessionTrend`, `session_flow_score()` 개인화 상대 편차 순수 함수 추가 및 단위 테스트
- `LocalFloorRecommender::recommend()`에서 세션 모멘텀 점수를 블렌딩 가중치로 합산하여 최종 정렬 반영
- 단위 테스트 추가 및 전체 워크스페이스 검증

### Out-of-Scope (건드리지 말 것 / 후속 마일스톤)
- UI 변경 및 배지 노출 (Phase 4에서 통합 처리)
- `play_events` 별도 로깅 테이블 변경 (선행 작업과 독립)
- DB 테이블 컬럼 구조 변경 (`records` 테이블 기존 구조 유지)

---

## 2. 상세 설계

### 2.1 인덱스 추가 (`record_db.rs`)

`create_records_table` 및 `ensure_schema`에 최근 플레이 쿼리 전용 인덱스를 추가한다:

```sql
CREATE INDEX IF NOT EXISTS idx_records_recent ON records (steam_id, button_mode, updated_at DESC);
```

### 2.2 `RecentRecordEntry` 구조체 및 `RecordDB` 조회 메서드

**파일**: `rust/overmax_data/src/store/record_db.rs`

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct RecentRecordEntry {
    pub song_id: i32,
    pub button_mode: Mode,
    pub difficulty: Difficulty,
    pub rate: f64,
    pub is_max_combo: bool,
    pub updated_at: i64,
}

impl RecordDB {
    pub fn get_recent_records(
        &self,
        steam_id: &str,
        mode: Mode,
        limit: usize,
    ) -> Vec<RecentRecordEntry> {
        if !self.is_ready || steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID || limit == 0 {
            return Vec::new();
        }

        let button_mode = mode.as_str();
        let query = "SELECT song_id, difficulty, rate, is_max_combo, updated_at
                     FROM records
                     WHERE steam_id = ?1 AND button_mode = ?2
                     ORDER BY updated_at DESC
                     LIMIT ?3";

        let mut results = Vec::new();
        let _ = self.with_rate_map_connection(|conn| {
            if let Ok(mut stmt) = conn.prepare(query) {
                if let Ok(mut rows) = stmt.query(rusqlite::params![steam_id, button_mode, limit as i64]) {
                    while let Ok(Some(row)) = rows.next() {
                        if let (Ok(song_id_str), Ok(diff_str), Ok(rate), Ok(mc_int), Ok(updated_at)) = (
                            row.get::<_, String>(0),
                            row.get::<_, String>(1),
                            row.get::<_, f64>(2),
                            row.get::<_, i32>(3),
                            row.get::<_, i64>(4),
                        ) {
                            if let (Ok(sid), Some(diff)) = (
                                song_id_str.parse::<i32>(),
                                Difficulty::from_str(&diff_str),
                            ) {
                                results.push(RecentRecordEntry {
                                    song_id: sid,
                                    button_mode: mode,
                                    difficulty: diff,
                                    rate,
                                    is_max_combo: mc_int != 0,
                                    updated_at,
                                });
                            }
                        }
                    }
                }
            }
        });
        results
    }
}
```

### 2.3 `RecordManager` pass-through

**파일**: `rust/overmax_data/src/service/record_manager.rs`

```rust
impl RecordManager {
    pub fn get_recent_records(&self, mode: Mode, limit: usize) -> Vec<RecentRecordEntry> {
        let steam_id = self.record_db.get_steam_id();
        self.record_db.get_recent_records(&steam_id, mode, limit)
    }
}
```

### 2.4 세션 모멘텀 점수 순수 함수 (`session_flow_score`)

**파일**: `rust/overmax_data/src/service/recommend.rs`

- **상수 정의**:
  - `MOMENTUM_CLIMB_DELTA: f64 = 0.5;` (상대 상승 편차 기준치)
  - `MOMENTUM_RECOVERY_DELTA: f64 = -1.0;` (상대 저조 편차 기준치)
  - `MOMENTUM_MAX_BONUS: f64 = 4.0;` (모멘텀 추천 최대 보너스)
  - `SESSION_IDLE_TIMEOUT_HOURS: f64 = 4.0;` (세션 만료 기준 시간)

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTrend {
    /// 직전 성과 상승 추세 (개인 평균 대비 +0.5% 이상 또는 맥콤) -> 상위 난이도(+0.1 ~ +0.4) 도전 권장
    Climbing,
    /// 직전 성과 평이/세션 시작 (개인 평균 대비 ±1.0% 이내) -> 동급 난이도(±0.15) 안정화 권장
    Steady,
    /// 직전 성과 저조 (개인 평균 대비 -1.0% 미만) -> 살짝 낮은 난이도(-0.2 ~ -0.5) 회복 권장
    Recovery,
}

impl SessionTrend {
    pub fn from_recent_plays(
        recent_plays: &[RecentRecordEntry],
        now_unix: i64,
    ) -> Option<Self> {
        let last_play = recent_plays.first()?;
        let elapsed_hours = ((now_unix - last_play.updated_at).max(0) as f64) / 3600.0;
        if elapsed_hours > SESSION_IDLE_TIMEOUT_HOURS {
            return None;
        }

        let session_plays: Vec<&RecentRecordEntry> = recent_plays
            .iter()
            .filter(|r| {
                ((now_unix - r.updated_at).max(0) as f64) / 3600.0 <= SESSION_IDLE_TIMEOUT_HOURS
                    && r.rate > 0.0
            })
            .collect();

        if session_plays.is_empty() {
            return None;
        }

        if session_plays.len() == 1 {
            return Some(Self::Steady);
        }

        let avg_rate =
            session_plays.iter().map(|r| r.rate).sum::<f64>() / session_plays.len() as f64;
        let delta = last_play.rate - avg_rate;

        if delta >= MOMENTUM_CLIMB_DELTA || (last_play.is_max_combo && delta >= 0.0) {
            Some(Self::Climbing)
        } else if delta >= MOMENTUM_RECOVERY_DELTA {
            Some(Self::Steady)
        } else {
            Some(Self::Recovery)
        }
    }
}

/// 직전 플레이 성과와 후보 곡의 Floor 관계를 평가하여 세션 모멘텀 가중치를 반환한다.
fn session_flow_score(
    cand_floor: f64,
    ref_floor: f64,
    trend: Option<SessionTrend>,
) -> f64 {
    let Some(trend) = trend else {
        return 0.0;
    };

    let delta = cand_floor - ref_floor;

    match trend {
        SessionTrend::Climbing => {
            // +0.1 ~ +0.4 구간에서 최고 점수 (최대 4.0)
            if (0.0..=0.4).contains(&delta) && delta > 0.0 {
                let center = 0.25;
                let dist = (delta - center).abs();
                (1.0 - (dist / 0.25)).max(0.0) * MOMENTUM_MAX_BONUS
            } else {
                0.0
            }
        }
        SessionTrend::Steady => {
            // ±0.15 이내 동급 난이도에서 최고 점수 (최대 3.0)
            if delta.abs() <= 0.15 {
                (1.0 - (delta.abs() / 0.15)) * (MOMENTUM_MAX_BONUS * 0.75)
            } else {
                0.0
            }
        }
        SessionTrend::Recovery => {
            // -0.5 ~ -0.1 구간에서 최고 점수 (최대 4.0)
            if (-0.5..0.0).contains(&delta) {
                let center = -0.3;
                let dist = (delta - center).abs();
                (1.0 - (dist / 0.3)).max(0.0) * MOMENTUM_MAX_BONUS
            } else {
                0.0
            }
        }
    }
}
```

### 2.5 `LocalFloorRecommender` 정렬에 통합

`LocalFloorRecommender::recommend()`에서:
1. `now_unix` 시점에 `let recent_plays = self.rdb.get_recent_records(ctx.button_mode, 1);` 조회
2. 직전 판으로 `let trend = recent_plays.first().and_then(|r| SessionTrend::from_recent_play(r.rate, r.updated_at, now_unix));` 판별
3. 후보 정렬 시 복합 우선순위 점수 계산:
   $$\text{priority} = \text{retry\_priority} + \text{top50\_boundary\_score} + \text{session\_flow\_score}(\text{cand\_floor}, \text{final\_ref\_floor}, \text{trend})$$
   - 플레이된 곡 간 정렬: `priority` 내림차순 -> `floor` 오름차순

---

## 3. 테스트 계획

1. **`SessionTrend` 및 `session_flow_score` 순수 함수 단위 테스트**:
   - 4시간 초과 시 `SessionTrend::from_recent_play`가 `None`을 반환하는 세션 만료 검증
   - `Climbing` 추세일 때 $+0.25$ 위쪽 후보 곡에 최고 보너스(~4.0) 반환 검증
   - `Steady` 추세일 때 동급 난이도($\pm 0.0$)에 안정화 보너스(~3.0) 반환 검증
   - `Recovery` 추세일 때 $-0.3$ 아래쪽 후보 곡에 회복 보너스(~4.0) 반환 검증
   - `trend == None`일 때 0.0 반환(폴백) 검증
2. **`RecordDB::get_recent_records` 단위 테스트**:
   - mock 레코드 삽입 후 `updated_at DESC` 정렬 및 limit 동작 검증
3. **전체 통합 회귀 검증**:
   - `cargo test --workspace` 전체 통과

---

## 4. Definition of Done

- [x] `records` 테이블에 `idx_records_recent` 복합 인덱스 추가
- [x] `RecentRecordEntry` 정의 및 `RecordDB::get_recent_records` 추가
- [x] `RecordManager::get_recent_records` pass-through 추가
- [x] `SessionTrend`, `session_flow_score()` 순수 함수 및 단위 테스트 작성
- [x] `LocalFloorRecommender`에 세션 모멘텀 점수 블렌딩 통합
- [x] `cargo test --workspace --locked` 전체 통과
- [x] `cargo clippy --workspace --all-targets --locked` 경고 없음
- [x] `overmax_data` 크레이트 외 파일 변경 없음
