# Detection Pipeline Architecture

이 문서는 Overmax 디텍션 파이프라인의 구조, ROI 세부 사양, 프레임 원자성(Atomicity) 및 기록 DB 무결성 보장 아키텍처를 정의한다.

---

## 1. 파이프라인 개요 및 3대 핵심 원칙

Overmax 디텍션 파이프라인은 DJMAX RESPECT V 인게임 화면을 실시간 분석하여 현재 선곡 정보 및 플레이 결과를 추출한다.

1. **Zero Process Injection**: 메모리 접근 및 프로세스 인젝션 없이 100% 화면 캡처 및 Win32/X11 API 추적 방식으로만 동작한다.
2. **Game Performance First**: 동적 폴링, 픽셀 체크섬 Early Return, OCR 1-Pass 제한, 순수 Rust Native CV(해시+히스토그램)로 인게임 프레임 드랍을 차단한다.
3. **Verified Flow & Frame Atomicity**: 단일 캡처 프레임에서 모든 ROI를 동시 크롭하여 원자적 `PlayContext`를 생성하고, 연속 N프레임 다수결 검증(`is_stable`)을 통과해야만 기록을 Commit한다.

---

## 2. 4단계 파이프라인 데이터 흐름 (4-Stage Flow)

```
[ 1. 캡처 & 동적 폴링 ] ──► [ 2. 씬 & 곡 감지 ] ──► [ 3. 세부 상태 감지 ] ──► [ 4. 원자적 확정 및 저장 ]
 (GDI/DXGI/XComposite)      (OCR-Free 재킷매칭)     (모드/난이도/Rate/콤보)      (N프레임 검증, DB저장)
```

### Stage 1: 프레임 캡처 및 스케줄링 (Frame Capture & Scheduling)
* **주요 모듈**: `rust/overmax_engine/src/capture/window_tracker.rs`, `adaptive_capture.rs`
* **동적 폴링**: 창 이동 중(16ms/60FPS), 정지 중(300ms), 창 미발견(1000ms) 주기로 시스템 콜 오버헤드를 차단.
* **적응형 백엔드**: Borderless/전체화면 시 DXGI/XComposite 백엔드, 창모드 시 GDI 백엔드 사용. DXGI 캡처 실패 시 3초 쿨다운 적용.

### Stage 2: 씬 판별 및 곡 매칭 (Scene & Song Identification)
* **주요 모듈**: `rust/overmax_engine/src/detector/detection_pipeline.rs`, `rust/overmax_data/src/service/jacket_matcher.rs`
* **OCR-Free 씬 감지**: 로고 OCR 스팸을 완전히 제거. 재킷 영역 엣지 강도(`JACKET_EDGE_THRESHOLD = 15.0`) 또는 4px 우측 카테고리 띠 단색성을 판정하여 즉시 곡 매칭으로 진입.
* **Native CV 곡 매칭**: 1차 u64 Perceptual Hash (즐겨찾기 마스킹 적용) + 2차 2x2 분할 그리드 히스토그램 L1 벌점 WTA 방식으로 씬 및 `song_id`를 동시에 빠른 확정.
* **히스테리시스 버퍼**: `HysteresisBuffer`를 통한 다수결 필터링으로 씬 튀는 현상(Jitter) 방지.

### Stage 3: 세부 플레이 상태 감지 (Play State Detection)
* **주요 모듈**: `rust/overmax_engine/src/detector/play_state.rs`, `ocr_engine.rs`
* 선곡창(Freestyle/OpenMatch)과 결과창(Result)을 독립 픽셀 매칭으로 감지하여 캐시 전염 방지.
* **버튼 모드**: 선곡창 ROI BGR 평균 색상 대조(<=60) / 결과창 독립 템플릿 매칭.
* **난이도**: 선곡창 패널 상대 밝기 / 결과창 독립 템플릿 매칭.
* **Max Combo 뱃지**: 사전 정의 대표 뱃지와 해시 거리 대조 (선곡창 <=10.0, 결과창 <=20.0).
* **Rate & Score**: 픽셀 체크섬(`compute_pixel_checksum`) 변화 시에만 200ms 간격 1-Pass OCR 실행. Score 역산값(`Rate = Score / 10,000`) 대조를 통한 교차 검증 및 0.1% 오차 자동 보정.

### Stage 4: 원자적 상태 확정 및 DB 저장 (Atomic Commit & Sync)
* **주요 모듈**: `rust/overmax_engine/src/detector/detection_pipeline.rs`, `rust/overmax_data/src/store/record_db.rs`
* 추출된 정보를 단일 구조체 `PlayContext`로 결합.
* 연속 3프레임 이상 100% 동일 시 `is_stable = true` 승인 후 `cache/record.db`에 upsert.

---

## 3. ROI (Region of Interest) 사양 및 쓰임새

ROI 좌표 및 크기는 `rust/overmax_data/src/config/scene_config.rs` 및 `rust/overmax_engine/src/detector/roi.rs`에 정의되어 런타임 해상도 스케일링이 적용된다.

### 선곡 화면 (Freestyle / OpenMatch)
| ROI 이름 | ROI 목적 및 활용 방식 |
| :--- | :--- |
| `jacket` | 선택 곡의 재킷 이미지(60x60)를 크롭하여 `JacketMatcher`가 곡 `song_id` 식별 |
| `btn_mode` | 5x5 ROI BGR 평균 색상을 모드 대표 색상과 대조하여 버튼 모드(4B, 5B, 6B, 8B) 감지 |
| `diff_panel` | 난이도 패널 영역(NM, HD, MX, SC)의 상대적 밝기 비교로 난이도 감지 |
| `rate` | 선곡창 해당 패턴의 내 기록 판정률(Rate %) OCR 추출 |
| `score` | 선곡창 내 기록 점수(Score) OCR 추출 (Rate OCR 검증용) |
| `max_combo_badge` | 36x36 뱃지 영역 해시 대조로 [M] (Max Combo) / [P] (Perfect Play) 감지 |

### 결과 화면 (ResultFreestyle / ResultOpen3 / ResultOpen2)
| ROI 이름 | ROI 목적 및 활용 방식 |
| :--- | :--- |
| `jacket` | 결과창 상단 재킷(60x60) 인식으로 결과창 진입 확정 및 곡 ID 검증 |
| `mode_digit` / `openmatch_mode` | 결과창 전용 독립 템플릿 매칭으로 버튼 모드 감지 (선곡창 캐시 의존 0%) |
| `diff_panel` / `openmatch_diff` | 결과창 전용 독립 템플릿 매칭으로 난이도 감지 |
| `rate` | 이번 플레이의 최종 판정률(Rate %) OCR 추출 |
| `score` | 이번 플레이의 최종 점수(Score) OCR 추출 (Rate OCR 교차 검증용) |
| `max_combo_badge` | 75x75 대형 뱃지 영역 해시 대조로 이번 플레이의 Max Combo 달성 여부 감지 |
| `player_panel` / `mode_colorbar` | 오픈매치 결과창 엣지 감지 및 BGA 명암 조건 판정 정밀화 |

### 전역 (Global)
| ROI 이름 | ROI 목적 및 활용 방식 |
| :--- | :--- |
| `logo` | 상단 340x75 전역 ROI (ROI 좌표계 기준점 제공) |

---

## 4. 단일 프레임 원자성 및 DB 오기록 방지 5중 검증 아키텍처

```
[ 캡처: 단일 CapturedFrame 버퍼 ] 
       │ (모든 ROI 동시 crop)
       ▼
[ 원자적 PlayContext 생성 ] ──► [ 5중 검증 가드 (5-Layer Safeguards) ] ──► [ RecordDb 저장 ]
```

### 1) 단일 프레임 캡처 원자성 (Single Frame Snapshot)
캡처 백엔드가 제공하는 단 **한 장의 `CapturedFrame` 메모리 버퍼**를 통째로 파이프라인에 전달하며, 모든 ROI 크롭과 CV/OCR 연산이 그 단일 프레임에서 동시에 실행된다. 프레임 찢어짐(Tearing)이나 서로 다른 프레임의 정보가 섞이지 않는다.

### 2) 5중 DB 오기록 방지 통제망 (5-Layer Safeguards)
1. **N프레임 다수결 검증 가드 (`is_stable == true`)**: `PlayContext` 전체 튜플이 연속 3프레임 이상 100% 토씨 하나 틀리지 않고 동일해야만 승인. 연출 롤링 중 노이즈는 저장 차단.
2. **최소 유효 판정률 가드 (`MIN_VALID_RATE = 80.0%`)**: DJMAX RESPECT V 유효 판정 기준인 80% 미만 수치나 미플레이(`0.0%`) 수치는 DB 저장 시도 100% 차단.
3. **Score ↔ Rate 교차 역산 검증**: Rate OCR과 Score OCR(`Rate = Score / 10,000`) 대조로 0.1% 미만 오차 자동 보정 및 100% 초과 이상치 제거.
4. **기록 갱신 전용 승인 (`only_if_improved`)**: 기존 DB 기록보다 Rate가 높거나 Max Combo를 새로 달성한 경우에만 DB를 업데이트.
5. **선곡창 캐시 전염 완전 차단**: 결과창 진입 시 선곡창 캐시를 완전 삭제하고 결과창 픽셀로만 독립 감지.

---

## 5. 주요 코드 진입점 맵

* **엔진 파이프라인 구동**: `rust/overmax_engine/src/detector/detection_pipeline.rs`
* **분석 워커 스레드**: `rust/overmax_engine/src/detector/detection_worker.rs`
* **플레이 상태 & ROI 감지**: `rust/overmax_engine/src/detector/play_state.rs`
* **ROI 변환 & 설정**: `rust/overmax_engine/src/detector/roi.rs`, `rust/overmax_data/src/config/scene_config.rs`
* **재킷 이미지 매칭**: `rust/overmax_data/src/service/jacket_matcher.rs`
* **기록 DB 연동**: `rust/overmax_data/src/store/record_db.rs`, `rust/overmax_app/src/ui/native_app_recommend.rs`
