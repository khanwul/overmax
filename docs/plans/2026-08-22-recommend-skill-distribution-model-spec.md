# TrueSkill 기반 실력 분포(μ, σ) 모델링 및 추천 시스템 고도화 스펙

**작성일**: 2026-08-22  
**작업 브랜치**: `feat/recommend-skill-model`  
**관련 모듈**: `overmax_data::service::recommend` (`scoring.rs`, `strategy.rs`, `local.rs`, `types.rs`), `overmax_app::ui`  

---

## 1. 문제 정의 & 배경

### 1.1 패드 패턴(NM/HD/MX) 추천 일관성 결여 및 SC 우선 분리 미흡
- **현상**: SC 난이도는 추천 레벨이 안정적으로 유지되나, 일반 패드 패턴(NM/HD/MX) 선택 시 패턴마다(비공식 난이도 유무에 따라) 추천 레벨이 달라지거나 공백이 됨.
- **원인**:
  - `floor_summary` 호출 시 현재 커서가 놓인 패턴에 비공식 난이도가 있는지 여부(`floor_name.is_some()`)에 따라 `use_official` 플래그가 결정되어 `find_floor` 클로저의 동작이 바뀜.
  - 4B/6B처럼 Top 50이 100% SC로만 채워진 모드에서는 `is_sc = false` 필터링 시 Top 50 앵커가 완전히 소실(`None`)되어 추천 레벨이 정상 산출되지 않음.

### 1.2 저난도 저조 플레이 후 고난도 곡에서 'REST' 뱃지 오발동 모순
- **현상**: 손풀기/저난도(예: 8B SC 9.3 End of the Moonlight)에서 다소 아쉬운 성과(99.57%)를 기록하여 세션 트렌드가 `Recovery`로 전환되었을 때, 고난도(SC 12~15) 곡으로 커서를 이동하면 해당 고난도 곡들에 전부 `REST`("세션 회복/손풀기 적정 난이도") 뱃지가 붙어 추천됨.
- **원인**:
  - `SessionTrend::Recovery` 상태일 때 `session_flow_score`가 동일 Floor 내 과거 성과만을 보고 flow 점수를 부여하며, `derive_recommend_reason`은 트렌드가 `Recovery`라는 이유만으로 `REST` 뱃지를 부여함.
  - 유저의 실력보다 훨씬 높은 고난도 곡임에도 "REST(회복/휴식)" 뱃지가 부여되는 도메인적 모순 발생.

---

## 2. 해결 아키텍처: 통계적 실력 분포 모델 (μ, σ)

Microsoft TrueSkill / TrueMatch의 핵심 아이디어인 **실력의 정규분포 모델링** $\mathcal{N}(\mu, \sigma^2)$을 도입하여 플레이어의 실력과 난이도 매칭을 수학적으로 정립한다.

```
                  ┌────────────────────────┐
                  │   V-Archive Top 50     │
                  │   & 누적 플레이 기록   │
                  └───────────┬────────────┘
                              │
                              ▼
        ┌───────────────────────────────────────────────┐
        │   장기 기본 실력 프로필 (Base Skill Model)    │
        │   - SC 실력:  N(μ_sc,  σ_sc²)                │
        │   - Pad 실력: N(μ_pad, σ_pad²)               │
        └─────────────────────┬─────────────────────────┘
                              │
                              ▼
        ┌───────────────────────────────────────────────┐
        │   당일 세션 컨디션 갱신 (Session Bayesian)    │
        │   - 최근 플레이 성과로 μ_session, σ_session 갱신 │
        └─────────────────────┬─────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              ▼                               ▼
    [Footer 추천 레벨 도출]          [후보 곡 세그먼트 & 뱃지 가드]
    - SC:  "SC <round(μ_sc)>"        - REST:  Floor ≤ μ - 0.8σ (저난도만 허용)
    - Pad: "<round(μ_pad)>"          - UP:    μ ≤ Floor ≤ μ + 1.2σ
    (커서 위치와 무관하게 고정)      - TRY:   |Floor - μ| ≤ σ (재도전)
```

### 2.1 버튼 모드별 2-Track 실력 모델
각 버튼 모드(4B, 5B, 6B, 8B)에 대해 SC와 일반(Pad) 2개의 독립된 스킬 분포를 산출한다.

1. **SC 스킬 분포 $\mathcal{N}(\mu_{sc}, \sigma_{sc}^2)$**:
   - **$\mu_{sc}$ (평균 Floor)**: Top 50 중 SC 패턴들의 Floor 가중치/중앙값.
   - **$\sigma_{sc}$ (표준편차)**: Top 50 SC 패턴 Floor들의 표준편차 ($\text{clamp}(0.8, 2.5)$).
2. **일반(Pad) 스킬 분포 $\mathcal{N}(\mu_{pad}, \sigma_{pad}^2)$**:
   - Top 50에 일반 패턴이 5개 이상 존재할 경우 해당 패턴들의 통계치 사용.
   - Top 50이 SC 위주일 경우 (Cross-Track Fallback): SC 실력 $\mu_{sc}$로부터 환산 앵커링 ($\mu_{pad} \approx \min(15.0, \mu_{sc} + 1.0)$, 단 일반 공식 레벨 1~15 스케일).
3. **엣지 케이스 방어 (Zero-Division Guard)**:
   - 기록 수 $N < 3$일 때는 안전 기본값($\mu_{sc}=5.0, \sigma_{sc}=1.5$) 적용.
   - $\sigma$는 상하한 가드($\text{clamp}(0.8, 2.5)$)를 적용하여 분모 0 방지.

### 2.2 당일 세션(오늘) 컨디션 실시간 조정
- **모멘텀 델타 $\Delta\mu$**: 세션 내 주력 플레이들의 Performance Rating 편차 기반으로 $\mu_{session} = \mu_{base} + \Delta\mu$ ($\Delta\mu \in [-0.5, +0.5]$).
- **분산 $\sigma_{session}$**:
  - 연속 고득점/맥스콤보 달성 시: $\sigma$ 소폭 감소 (자신감 상승, 타깃 집중).
  - 저조/기복 발생 시: $\sigma$ 소폭 증가 (탐색 범위 확대 및 안전한 저난도 쉬어가기 유도).

### 2.3 추천 사유 뱃지 발동 조건 (Domain Gating)

| 뱃지 | 명칭 | 조건 (Floor $F$) | 설명 |
|---|---|---|---|
| **REST** | 회복/손풀기 | $F \le \mu - 0.8\sigma$ AND $\text{Trend} = \text{Recovery}$ | **고난도 곡에는 절대 발동 불가**. 내 평균보다 확실히 낮은 안전 구간에서만 발동 |
| **UP** | 상위 도전 | $\mu \le F \le \mu + 1.2\sigma$ AND $\text{Trend} = \text{Climbing}$ | 컨디션 쾌조 시 내 평균 이상의 상위 난이도 도전 |
| **TRY** | 기록 경신 | $\|F - \mu\| \le 1.0\sigma$ AND $\text{Rate Gap} > 0$ | 내 주력 실력 범위 내에서 점수가 아쉬운 곡 |
| **TOP** | Top-50 방어/돌파 | $F \approx \text{Cutoff Floor}$ AND $\text{Rating} \approx \text{Cutoff}$ | Top 50 경계 구간 곡 |
| **CLR** | 신규 클리어 | 미플레이 AND $F \le \mu + 0.5\sigma$ | 클리어 가능한 미플레이 곡 |

---

## 3. 단계별 실행 계획

- [x] **Plan: 상세 스펙 및 자가 비판 검증 문서화**
- [x] **Step 1: Skill Distribution (`SkillProfile`) 코어 데이터 모델 구현**
  - `overmax_data::service::recommend::scoring`에 `SkillProfile` 구조체 정의 ($\mu, \sigma$, 2-Track SC/Pad).
  - Top 50 및 누적 기록 기반 프로필 산출 순수 함수 구현 및 단위 테스트.
- [x] **Step 2: `derive_footer_level` 리팩토링 (커서 오염 제거 & SC 우선)**
  - 현재 곡 패턴의 `use_official`에 의존하지 않고, 현재 탭 모드(SC vs Pad)에 맞춰 고정된 `SkillProfile` 기반 레벨 표기.
- [x] **Step 3: `derive_recommend_reason` 및 `session_flow_score`에 실력 분포 게이팅 적용**
  - $F > \mu - 0.8\sigma$인 고난도 곡에서 `REST` 뱃지가 생성되는 현상 원천 차단.
  - 컨디션 저조 시 고난도 곡 탐색 중에는 무리한 flow 보너스 대신 안정 정렬로 폴백.
- [x] **Step 4: 단위 테스트 및 실데이터(8B End of Moonlight 시나리오) 회귀 검증**
  - 실제 사용자 DB 이력을 모의한 테스트 케이스 작성 및 검증.
- [x] **Step 5: 정적 검증(`cargo clippy`, `cargo test`) 및 문서 동기화 (`CONTEXT.md`, Decision Log)**

---

## 4. 트레이드오프 및 기대 효과

1. **사용자 경험 (UX)**:
   - 곡 목록을 스크롤하거나 커서를 옮겨도 하단 추천 레벨이 흔들리지 않고 일관되게 유지됨.
   - 고난도 곡에 "REST"가 뜨는 부자연스러움이 완전히 해소되고, 직관적인 이유 뱃지가 표시됨.
2. **성능**:
   - `SkillProfile` 산출은 레코드 갱신 시 $O(1)$ 캐싱 전략으로 선곡 화면 렌더링 핫패스에 오버헤드 0ms.
3. **확장성**:
   - $\mu, \sigma$ 기반 모델은 향후 온라인 매칭, AI 추천, 외부 레이팅 시스템 연동 시 표준 인터페이스로 쉽게 확장 가능.
