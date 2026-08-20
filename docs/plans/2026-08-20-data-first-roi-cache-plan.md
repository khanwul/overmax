# Data-First RoiCache Refactoring Implementation Plan

**작성일**: 2026-08-20  
**목표**: 상태 조작 산발화를 방지하고, 데이터 우선(Data-First) 원칙에 입각한 `PatternRecord` 모델링 및 `RoiCache` 컴포넌트 캡슐화를 통해 점수/모드/난이도 감지 안정성 극대화.  
**대상 크레이트**: `overmax_core`, `overmax_engine`  
**불변 제약 조건**: 
- 100% OCR-Free 템플릿 매칭 유지
- 인게임 및 선곡 화면 성능 최적화(체크섬 기반 Early Return 및 0.2초 쿨다운 유지)
- 기존 `PlayContext` 및 상위 UI 인터페이스와의 호환성 유지

---

## 1. 문제 정의

1. **상태(State)와 수명 주기(Lifecycle)의 산발적 파편화**:
   - `PlayStateDetector` 내부에 `last_rate_checksums`, `last_rate_result`, `last_target_pattern`, `last_rate_detection_ts`, `result_rate_window` 등이 평평하게(flat) 흩어져 있음.
   - `reset()`, `clear_detected_cache()`, `detect()`, `process_rate_detection()` 등 여러 함수에서 이 변수들을 수동으로 한 줄씩 갱신/초기화하여 누락 시 잔류 버그(Stale Cache) 발생 위험 상존.
2. **모호한 점수 데이터 표현 (`f32` vs 명확한 상태)**:
   - 점수가 `0.0`인 상황이 "미플레이"인지, "인식 실패"인지, "실제 0.00% 기록"인지 구분되지 않아 캐시 무효화 시 혼선 유발.
3. **모드/난이도와 점수 감지의 비대칭적 구조**:
   - 모드/난이도는 `ModeDiffCache`와 `last_mode_diff_checksums`를 사용하고, 점수는 수동 변수를 사용하여 두 감지기 간 일관성이 결여됨.

---

## 2. 세부 구현 계획

### Phase 1: 데이터 모델 명확화 (`overmax_core::game_state`)

1. **`PatternRecord` Enum 정의**:
   ```rust
   #[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
   pub enum PatternRecord {
       /// 기록 없음 / 미플레이 (0.00%)
       Unplayed,
       /// 유효 기록 존재 (80.0% ~ 100.0%)
       Played {
           rate: f32,
           is_max_combo: bool,
       },
   }

   impl PatternRecord {
       #[inline]
       pub fn rate(&self) -> f32 {
           match self {
               Self::Unplayed => 0.0,
               Self::Played { rate, .. } => *rate,
           }
       }

       #[inline]
       pub fn is_max_combo(&self) -> bool {
           match self {
               Self::Unplayed => false,
               Self::Played { is_max_combo, .. } => *is_max_combo,
           }
       }
   }
   ```
2. 기존 `PlayContext` 헬퍼 메서드 호환성 유지.

---

### Phase 2: 단일 화면 관측 캐시 컴포넌트 구축 (`overmax_engine::detector::play_state`)

1. **`RoiCache<Key, Checksum, Value>` 구조체 구현**:
   - `Key`: 상위 컨텍스트 식별자 (`RecordKey` 또는 `()`)
   - `Checksum`: 화면 ROI 체크섬 스냅샷
   - `Value`: 판독된 감지 데이터 (`PatternRecord` 또는 `(Option<Mode>, Option<Difficulty>, bool)`)
2. **핵심 불변식(Invariants) 캡슐화**:
   - `sync_key(current_key)`: 키(곡/모드/난이도)가 변경되면 이전 캐시를 자동으로 즉시 무효화(Invalidate).
   - `get_or_detect(checksum, now, detect_fn)`:
     - 쿨다운(`interval_sec`)과 체크섬 변경 여부를 단일 진입점에서 검사.
     - 미플레이(`None`) 결과도 안전하게 캐싱(Negative Caching)하여 불필요한 재연산 차단.
   - `clear()`: 내부 상태를 완전히 비움 (결과창 이탈 및 파이프라인 리셋 시 단 한 줄로 완결).

---

### Phase 3: `PlayStateDetector` 내부 캐시 통합 및 단순화

1. **`PlayStateDetector` 필드 재구성**:
   - 흩어져 있던 개별 캐시 필드들을 2개의 `RoiCache` 필드로 통합:
     - `mode_diff_cache: RoiCache<(), ModeDiffChecksums, (Option<Mode>, Option<Difficulty>, bool)>`
     - `rate_cache: RoiCache<RecordKey, RateInputChecksums, PatternRecord>`
2. **감지 흐름(Data Flow) 정돈**:
   - 1단계: 모드/난이도 감지 (`mode_diff_cache.get_or_detect`)
   - 2단계: `pattern_key: Option<RecordKey>` 구성 및 `rate_cache.sync_key(pattern_key)`
   - 3단계: 점수/Rate 감지 (`rate_cache.get_or_detect`)
   - 4단계: `PlayContext` 구성 및 Hysteresis 갱신

---

## 3. 검증 전략

1. **단위 테스트**:
   - `cargo test --workspace`
   - 미플레이 곡 전환 시 캐시 리셋 테스트
   - 동일 곡 내 체크섬 변경 시 0.0 클리어 테스트
   - 결과창 래치 및 1회 이벤트 방출 테스트
2. **린트 및 포맷 검증**:
   - `cargo fmt --check`
   - `cargo clippy --all-targets`
3. **성능 벤치마크 및 오버헤드 검증**:
   - 핫 루프 캡처 틱 내 Zero Allocation 유지 여부 검증.
