# Overmax 추천 엔진 확장 — Phase 1 작업 지시서

**대상 레인**: 레인 B(정체/재도전) 를 기존 레인 C(`LocalFloorRecommender`)에 블렌딩
**대상 크레이트**: `overmax_data` 단일 크레이트 (UI, engine, core 미변경)
**선행 조건**: 없음 (스키마 변경 없음, `play_events` 관련 별도 진행 중인 작업과 독립)

---

## 0. 배경

현재 `overmax_data::service::recommend::LocalFloorRecommender`는 floor(비공식/공식 난이도) 근접 곡을
"플레이한 곡은 rate 오름차순 → 미플레이 곡은 floor 오름차순"으로만 정렬해서 추천한다
(`rust/overmax_data/src/service/recommend.rs` 의 `RecommendationSource for LocalFloorRecommender::recommend()`).

이 정렬은 "낮은 점수부터 채워라"는 신호만 담고 있고, **얼마나 오래 안 쳤는지(최근성)** 는 전혀 반영하지 않는다.
`records.updated_at` 컬럼(unix epoch 초, `rust/overmax_data/src/store/record_db.rs`의 `create_records_table`)이 이미 존재하므로
스키마 변경 없이 최근성 신호를 더할 수 있다.

설계 원칙: "목표 정확도까지의 격차(`rate_gap`)"와 "마지막 플레이 이후 경과일"을 곱한 우선순위로,
방금 친 곡은 당일 억제하고 2주 이상 방치된 곡은 격차를 100% 반영해서 밀어올린다.

---

## 1. 스코프

### In-Scope (이번 작업)
- `RecordDB`에 곡별 `updated_at` 배치 조회 메서드 추가
- `RecordManager`에 pass-through 메서드 추가
- `LocalFloorRecommender::recommend()`의 정렬 로직을 "재도전 우선순위" 기반으로 교체
- 단위 테스트 추가

### Out-of-Scope (건드리지 말 것 / 별도 스펙 예정)
- **play_events 이력 로그 스키마 작업** — 별도로 선행 진행 중. 이 스펙에서는 참조도 하지 않는다.
- 레인 A (V-Archive rating top-50 경계 후보) — `get_varchive_top50_rank` 확장이 필요한 별도 작업
- 레인 D (결과창 직후 `sheet_meta` 태그 기반 후속 추천)
- UI 변경 (`overmax_app` 크레이트의 `overlay_recommend_ui.rs` 등) — "이유 배지" 같은 표시는 이번 스코프 아님
- `floor_summary()` / footer 통계(avg_rate, has_record_count, total_count) — 변경 없음, 정렬에만 영향

이 작업은 `overmax_data` 크레이트 내부, 그중에서도 사실상 3개 파일(`record_db.rs`, `record_manager.rs`, `recommend.rs`)만 건드린다.
ENGINEERING_TASTE.md의 Red Flag(3개 이상 크레이트에 걸친 핵심 데이터 모델 수정)에 해당하지 않으므로 owner 확인 없이 진행 가능.

---

## 2. 상세 설계

### 2.1 `RecordDB::get_updated_at_map` 추가 (신규, 파일: `rust/overmax_data/src/store/record_db.rs`)

기존 `get_rate_map`과 완전히 동일한 패턴(같은 thread-local 커넥션 헬퍼 `with_rate_map_connection` 재사용)으로,
컬럼만 `updated_at`으로 바꾼 배치 조회 메서드를 추가한다. 기존 메서드는 손대지 않는다.

```rust
pub fn get_updated_at_map(
    &self,
    song_ids: &[i32],
) -> std::collections::HashMap<RecordKey, i64> {
    if !self.is_ready || song_ids.is_empty() {
        return std::collections::HashMap::new();
    }

    let steam_id = self.get_steam_id();
    let placeholders = vec!["?"; song_ids.len()].join(",");
    let query = format!(
        "SELECT song_id, button_mode, difficulty, updated_at
         FROM records
         WHERE steam_id=?1 AND song_id IN ({})",
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
                    if let (
                        Ok(song_id_str),
                        Ok(button_mode),
                        Ok(difficulty),
                        Ok(updated_at),
                    ) = (
                        row.get::<_, String>(0),
                        row.get::<_, String>(1),
                        row.get::<_, String>(2),
                        row.get::<_, i64>(3),
                    ) {
                        if let Ok(sid) = song_id_str.parse::<i32>() {
                            if let (Some(m), Some(d)) = (
                                Mode::from_str(&button_mode),
                                Difficulty::from_str(&difficulty),
                            ) {
                                map.insert((sid, m, d), updated_at);
                            }
                        }
                    }
                }
            }
        }
    });
    map
}
```

`with_rate_map_connection`은 이미 `impl RecordDB` 블록 내부의 private 헬퍼이므로 같은 파일 안에서만 재사용 가능 — 문제 없음.

### 2.2 `RecordManager::get_local_updated_at_map` 추가 (신규, 파일: `rust/overmax_data/src/service/record_manager.rs`)

```rust
pub fn get_local_updated_at_map(
    &self,
    song_ids: &[i32],
) -> std::collections::HashMap<RecordKey, i64> {
    self.record_db.get_updated_at_map(song_ids)
}
```

V-Archive 캐시(`varchive_cache`) 쪽 최근성은 이번 스코프에 포함하지 않는다 — 로컬 기록(`records` 테이블) 기준으로만 계산한다.

### 2.3 `LocalFloorRecommender` 정렬 로직 교체 (파일: `rust/overmax_data/src/service/recommend.rs`)

#### 2.3.1 `RawCandidate`에 필드 추가

```rust
struct RawCandidate<'a> {
    // ... 기존 필드 유지 ...
    updated_at: Option<i64>,   // 신규
}
```

`get_candidates()`에서 `RawCandidate`를 만드는 지점에 `updated_at: None`으로 초기화 후,
`merge_record_rates()`에서 rate와 함께 채운다.

#### 2.3.2 `merge_record_rates()`에 최근성 병합 추가

기존에 `self.rdb.get_rate_map(&unique_ids)`를 호출하는 지점 바로 아래에
`self.rdb.get_local_updated_at_map(&unique_ids)` 호출을 추가하고, entry 순회 시 함께 채운다.

```rust
fn merge_record_rates(&self, candidates: &mut [RawCandidate<'_>]) {
    if !self.rdb.is_ready() {
        return;
    }
    let mut unique_ids = Vec::new();
    for c in candidates.iter() {
        if !unique_ids.contains(&c.song_id) {
            unique_ids.push(c.song_id);
        }
    }

    let rate_map = self.rdb.get_rate_map(&unique_ids);
    let updated_at_map = self.rdb.get_local_updated_at_map(&unique_ids); // 신규

    for entry in candidates.iter_mut() {
        let key = (entry.song_id, entry.mode, entry.diff);
        if let Some(&(rate, is_max_combo)) = rate_map.get(&key) {
            entry.rate = Some(rate as f64);
            entry.is_max_combo = is_max_combo;
        }
        entry.updated_at = updated_at_map.get(&key).copied(); // 신규
    }
}
```

#### 2.3.3 재도전 우선순위 계산 (모듈 레벨 순수 함수, 신규)

`recommend.rs` 파일 최상단(구조체 정의 이전 또는 이후 아무 곳)에 아래 상수와 함수를 추가한다.
**순수 함수로 분리**해서 `now_unix`를 명시적 인자로 받게 하면 `SystemTime::now()` 없이 단위 테스트가 가능하다
(이 코드베이스에서 디텍션 파이프라인이 `now: f64`를 항상 명시적으로 주입받는 관례와 동일한 패턴).

```rust
/// 재도전 목표 정확도. 100.0(퍼펙트) 대신 현실적인 재도전 유인 지점으로 설정.
const RETRY_TARGET_RATE: f64 = 99.5;
/// 이 시간 이내에 플레이한 곡은 재도전 우선순위에서 사실상 제외(당일 반복 추천 억제).
const RETRY_RECENCY_GRACE_HOURS: f64 = 12.0;
/// 이 일수가 지나면 격차(rate_gap)를 100% 반영(가중치 1.0로 포화).
const RETRY_RECENCY_RAMP_DAYS: f64 = 14.0;

/// 재도전 우선순위 = (목표 정확도 - 현재 rate) * 최근성 가중치.
/// `updated_at`이 없으면(레거시 데이터 등) 최근성 가중 없이 격차만 반환한다.
fn retry_priority(rate: f64, updated_at: Option<i64>, now_unix: i64) -> f64 {
    let rate_gap = (RETRY_TARGET_RATE - rate).max(0.0);
    let Some(updated_at) = updated_at else {
        return rate_gap;
    };
    let days_since = ((now_unix - updated_at).max(0) as f64) / 86400.0;
    let grace_days = RETRY_RECENCY_GRACE_HOURS / 24.0;
    let recency_weight = ((days_since - grace_days) / RETRY_RECENCY_RAMP_DAYS).clamp(0.0, 1.0);
    rate_gap * recency_weight
}
```

#### 2.3.4 `recommend()` 내 정렬 교체

기존 코드:

```rust
candidates.sort_by(|a, b| {
    if a.is_played() && !b.is_played() {
        Ordering::Less
    } else if !a.is_played() && b.is_played() {
        Ordering::Greater
    } else if let (Some(ra), Some(rb)) = (a.rate, b.rate) {
        ra.partial_cmp(&rb)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal))
    } else {
        a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal)
    }
});
```

교체 후:

```rust
let now_unix = Self::now_unix();
candidates.sort_by(|a, b| {
    match (a.is_played(), b.is_played()) {
        (true, false) => Ordering::Less,
        (false, true) => Ordering::Greater,
        (true, true) => {
            let pa = retry_priority(a.rate.unwrap_or(0.0), a.updated_at, now_unix);
            let pb = retry_priority(b.rate.unwrap_or(0.0), b.updated_at, now_unix);
            // 우선순위 내림차순 정렬: cmp(b, a) 형태로 비교
            pb.partial_cmp(&pa)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal))
        }
        (false, false) => a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal),
    }
});
```

`Self::now_unix()`는 `impl LocalFloorRecommender` 블록에 추가:

```rust
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
```

`candidates.truncate(ctx.max_results)`는 정렬 이후 그대로 유지(변경 없음).

---

## 3. 구현 순서

1. `record_db.rs`: `get_updated_at_map` 추가 + 단위 테스트
2. `record_manager.rs`: `get_local_updated_at_map` pass-through 추가
3. `recommend.rs`: `RawCandidate.updated_at` 필드, `retry_priority()` 순수 함수 + 상수 3개, `merge_record_rates()` 병합, 정렬 교체
4. `recommend.rs`: `retry_priority()` 단위 테스트 추가
5. `cargo test -p overmax-data`, `cargo clippy -p overmax-data --all-targets` 로컬 검증

각 단계는 독립적으로 컴파일 가능해야 하며, 커밋을 단계별로 쪼개는 걸 권장한다(2~3개 이하 저위험 항목 묶음 원칙).

---

## 4. 테스트 계획

### 4.1 `retry_priority()` 순수 함수 테스트 (recommend.rs)

```rust
#[test]
fn retry_priority_weights_gap_by_recency_and_grace_period() {
    let now = 1_000_000i64;

    // 1시간 전 플레이 (12시간 유예 이내) -> 가중치 거의 0
    let just_played = retry_priority(90.0, Some(now - 3600), now);
    assert!(just_played < 0.01);

    // 유예(12h) + 램프(14일)를 완전히 지남 -> 가중치 1.0, 격차 100% 반영
    let long_ago = retry_priority(90.0, Some(now - (14 * 86400 + 13 * 3600)), now);
    assert!((long_ago - 9.5).abs() < 0.05); // 99.5 - 90.0 = 9.5

    // 이미 목표치 이상 -> 격차 0, 최근성과 무관하게 0
    let maxed = retry_priority(99.5, Some(now - 30 * 86400), now);
    assert_eq!(maxed, 0.0);

    // updated_at 없음(레거시 로우) -> 최근성 가중 없이 순수 격차만 반환 (기존 동작과 호환)
    let no_timestamp = retry_priority(95.0, None, now);
    assert_eq!(no_timestamp, 4.5);
}
```

### 4.2 `RecordDB::get_updated_at_map` 테스트 (record_db.rs)

`get_rate_map` 관련 기존 테스트가 없으므로, `test_concurrent_record_db_writes` 근처에
`upsert()` 후 `get_updated_at_map()`으로 조회해 값이 채워지는지 확인하는 소규모 테스트를 추가한다.

### 4.3 회귀 확인

- 기존 `test_recommendation_caching_and_stats` (record_manager.rs) — 정렬 방식이 바뀌어도 이 테스트의 단언(entries.len(), avg_rate 등)이 깨지지 않는지 확인. 이 테스트는 미플레이 후보 하나만 남는 케이스라 영향 없을 가능성이 높지만, 실행해서 확인할 것.
- `test_composite_recommender_with_varchive_db_preserves_provider` — provider 로직은 안 건드리므로 영향 없어야 함.

---

## 5. Definition of Done

- [x] `RecordDB::get_updated_at_map` 추가 및 단위 테스트 통과
- [x] `RecordManager::get_local_updated_at_map` 추가
- [x] `LocalFloorRecommender`의 `RawCandidate`, `merge_record_rates`, 정렬 로직 반영
- [x] `retry_priority()` 단위 테스트 4종 통과
- [x] `cargo test --workspace --locked` 전체 통과 (특히 `overmax-data` 패키지)
- [x] `cargo clippy --workspace --all-targets --locked` 경고 없음
- [x] `overmax_data` 크레이트 외 파일 변경 없음 (git diff로 확인)
- [x] `LocalFloorRecommender`/`CompositeRecommender`의 공개 시그니처 불변 (하위 호환 유지 — `overmax_app`, `overmax_engine` 등 호출부 무변경 확인)

---

## 6. 리스크 & 롤백

- 스키마 변경 없음 (기존 `updated_at` 컬럼 재사용) → 롤백 시 DB 마이그레이션 이슈 없음, 커밋 리버트로 충분.
- 정렬 로직만 바뀌므로 최악의 경우에도 추천 "순서"만 달라질 뿐, 추천 후보 집합/카운트/footer 통계는 불변 — 사용자 체감 리스크가 작음.
- `RETRY_TARGET_RATE`(99.5), 유예/램프 상수(12h/14일)는 실사용 데이터 없이 설계 논의에서 잠정 확정한 값이다. 네이밍된 `const`로 분리해뒀으니, 실사용 후 owner 피드백에 따라 숫자만 조정하면 된다 — 로직 재작성 불필요.

---

## 7. 후속 스펙 예고 (이번 작업 범위 아님)

- **Phase 2**: 레인 A — `get_varchive_top50_rank`를 "단일 곡 순위 조회"에서 "top-N 리스트 + 경계값 조회"로 확장. V-Archive 계정 미연동 사용자는 자동으로 레인 B+C만 사용하도록 폴백.
- **Phase 3**: 레인 D — `VerifiedPlayEvent` 방출 직후 `PatternSheetMeta` 태그(gold/assist/keypart) 기반 후속 추천 부스트.
- **Phase 4**: play_events 이력 로그 기반 신호 (별도 선행 작업, 이미 진행 중 — 완료 후 별도 스펙 문서 작성 예정).
