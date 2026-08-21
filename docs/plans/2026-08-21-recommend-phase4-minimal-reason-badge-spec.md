# Phase 4: 추천 사유(Reason) 모델링 및 미니멀 오버레이 UI 시각화 스펙

- **작성일**: 2026-08-21
- **목표**: Phase 1(재도전), Phase 2(Top-50 경계), Phase 3(세션 모멘텀)에서 산출된 최고 기여 점수를 `RecommendReason`으로 매핑하고, 오버레이 UI 6칸 슬롯에서 가로 공간 침범과 시각적 조잡함을 억제한 **미니멀 뱃지(18px) 및 호버 툴팁**으로 표출한다.

---

## 1. 요구사항 및 디자인 원칙

### 1.1 절제된 UI 원칙 (De-cluttering)
1. **평범한 기본곡(`Fit`) 뱃지 생략**:
   - 일반적인 난이도 일치 곡은 뱃지를 아예 렌더링하지 않아 **곡명 공간을 100% 확보하고 시선 분산을 방지**한다.
2. **특별한 곡만 선택적 뱃지 노출**:
   - Top-50 돌파/수성, 세션 모멘텀, 방치 재도전 등 명확한 추천 이유가 있는 상위 1~2곡만 **18px 미니멀 뱃지**로 은은하게 표출한다.
3. **호버 툴팁 제공**:
   - 마우스 호버 시 "🎯 Top-50 컷라인 -0.8 돌파 타깃 곡", "🧗 세션 상승 모멘텀 도전 곡" 등의 친절한 상세 설명을 제공한다.

---

## 2. 데이터 계층 설계 (`overmax_data`)

### 2.1 `RecommendReasonKind` 및 `RecommendReason` 구조체

**파일**: `rust/overmax_data/src/service/recommend.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendReasonKind {
    /// V-Archive Top-50 컷라인 돌파 타깃
    Top50Attack,
    /// V-Archive Top-50 41~50위 수성
    Top50Defend,
    /// 세션 상승 모멘텀 상위 난이도 도전
    Climbing,
    /// 세션 회복/손풀기
    Recovery,
    /// 방치된 90~99% 기록 재도전
    Retry,
}

impl RecommendReasonKind {
    pub fn badge_label(&self) -> &'static str {
        match self {
            Self::Top50Attack => "TOP",
            Self::Top50Defend => "DEF",
            Self::Climbing => "UP",
            Self::Recovery => "REST",
            Self::Retry => "TRY",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecommendReason {
    pub kind: RecommendReasonKind,
    pub detail: String,
}
```

### 2.2 `RecommendEntry` 필드 확장
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecommendEntry {
    pub song_id: i32,
    pub song_name: String,
    pub composer: String,
    pub button_mode: Mode,
    pub difficulty: Difficulty,
    pub level: Option<u32>,
    pub floor: Option<f64>,
    pub floor_name: Option<String>,
    pub rate: Option<f64>,
    pub is_max_combo: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<RecommendReason>,
}
```

### 2.3 추천 사유 결정 알고리즘 (`derive_recommend_reason`)
각 후보 곡의 정렬 점수($\text{retry\_score}, \text{top50\_score}, \text{flow\_score}$) 중 **가장 지배적인 점수(Score > 1.5)**를 가진 레인을 추천 사유로 선정:

1. $\text{top50\_score} \ge \text{retry\_score} \land \text{top50\_score} \ge \text{flow\_score} \land \text{top50\_score} \ge 1.5$:
   - 순위가 41~50위 $\rightarrow$ `Top50Defend` ("Top-50 수성 (N위)")
   - 컷라인 돌파 $\rightarrow$ `Top50Attack` ("Top-50 컷라인 돌파 타깃")
2. $\text{flow\_score} \ge \text{retry\_score} \land \text{flow\_score} \ge 1.5$:
   - `Climbing` 추세 $\rightarrow$ `Climbing` ("세션 상승 모멘텀 도전")
   - `Recovery` 추세 $\rightarrow$ `Recovery` ("세션 회복/손풀기")
3. $\text{retry\_score} \ge 1.5$:
   - `Retry` ("방치된 기록 재도전")
4. 그 외 $\rightarrow$ `None` (기본 곡, 뱃지 생략)

---

## 3. UI 렌더링 계층 설계 (`overmax_app`)

### 3.1 뱃지 렌더링 사양 (`draw_reason_badge`)

* **크기**: 폭 $20\text{px} \times \text{scale}$, 높이 $14\text{px} \times \text{scale}$
* **모서리 반경**: $3.0\text{px}$
* **폰트**: $8.0\text{px}$, Bold, White
* **테마 색상**:
  * `Top50Attack` ("TOP"): 보라 (`Color32::from_rgb(160, 110, 255)`)
  * `Top50Defend` ("DEF"): 주황/골드 (`Color32::from_rgb(240, 150, 40)`)
  * `Climbing` ("UP"): 스카이블루 (`Color32::from_rgb(50, 180, 255)`)
  * `Recovery` ("REST"): 에메랄드 (`Color32::from_rgb(40, 190, 130)`)
  * `Retry` ("TRY"): 코랄레드 (`Color32::from_rgb(255, 95, 95)`)

---

## 4. 검증 계획
1. `overmax_data` 단위 테스트: 사유 매핑 로직(`derive_recommend_reason`) 및 점수별 올바른 사유 도출 검증
2. `overmax_app` UI 렌더링 테스트: 뱃지 유무에 따른 곡명 가로 폭 유지 및 레이아웃 검증
3. `cargo test --workspace`, `cargo clippy`, `cargo fmt` 통과 확인
