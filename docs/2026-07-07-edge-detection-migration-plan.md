# 로고/Result Mode 인식: 이진화 마스크 매칭 → 엣지 디텍션 전환 계획

## 1. 문제 정의

로고 영역(`logo` ROI)과 결과 화면 모드 숫자(`mode_digit` ROI)는 **BGA(Background Animation)**가 관통하는 공간에 위치합니다. 현재 파이프라인은 이 영역에 대해 두 가지 접근법을 사용합니다:

| 영역 | 현재 방식 | 문제 |
|------|-----------|------|
| **로고** (`logo` ROI) | Windows OCR Color 1-Pass → 키워드 매칭 | BGA에 의해 OCR 텍스트가 왜곡/미인식 → `SceneType::Unknown` 빈발 |
| **결과 모드** (`mode_digit` ROI) | 고정 threshold=120 이진화 → 50×68 마스크 매칭 | BGA가 배경 밝기를 변동시켜 이진화 결과가 불안정 → 매칭 실패 |

> [!IMPORTANT]
> 두 영역 모두 글자(숫자)의 **윤곽선(contour)**은 BGA와 관계없이 상대적으로 안정적입니다.
> 이진화 후 절대 밝기 기반 비교보다 **엣지(gradient) 기반 비교**가 BGA 변동에 강건합니다.

---

## 2. 원인 분석

### 로고 영역
- `detect_logo()` ([ocr_engine.rs](../rust/overmax_engine/src/detector/ocr_engine.rs#L50-L58))는 Windows OCR Color 1-Pass에 의존
- BGA가 투과되면 OCR이 배경 패턴을 문자로 오인식하거나, 글자와 BGA 색상이 겹쳐 인식 불가
- 현재 이미 `detect_rect_edges()`가 마지막 폴백으로 사용 중 ([detection_pipeline.rs](../rust/overmax_engine/src/detector/detection_pipeline.rs#L253-L278))
- HOG 템플릿(`logo_templates.rs`)이 존재하지만 **현재 미사용**

### Result Mode
- `detect_freestyle_mode()` ([ocr_engine.rs](../rust/overmax_engine/src/detector/ocr_engine.rs#L265-L327))는 고정 threshold=120으로 이진화
- BGA에 의해 배경 밝기가 120을 넘으면 글자와 배경이 혼재 → 마스크 매칭 점수 하락
- 이미 `set_logo_mode()`을 통한 OCR 파싱 폴백이 존재 ([play_state.rs](../rust/overmax_engine/src/detector/play_state.rs#L74-L79))

---

## 3. 해결 방법 (옵션별)

### Option A: 엣지 그래디언트 기반 템플릿 매칭 (추천)

**핵심 아이디어**: 이진화된 "절대 밝기 마스크" 대신, Sobel 등의 엣지 필터를 적용한 "그래디언트 크기 맵"을 생성하고, 이 맵에 대해 템플릿 매칭을 수행합니다.

```
현재:  BGRA → Luma → Threshold(120) → Binary Mask → Pixel Match
제안:  BGRA → Luma → Sobel Edge    → Edge Map    → Normalized Cross-Correlation
```

**수정 범위**:
1. `overmax_cv`에 Sobel 엣지 맵 생성 함수 추가 (1개 함수)
2. `detect_freestyle_mode()` 내부 전처리를 이진화→엣지로 교체 (1개 함수)
3. 결과 모드 템플릿을 엣지 기반으로 재생성 (데이터 파일 1개)

### Option B: 기존 detect_rect_edges() 패턴을 mode_digit에 확장

**핵심 아이디어**: 이미 `detect_rect_edges()`가 결과 화면 판별에 사용 중이므로, 유사한 경계선 기반 분석을 숫자 4/5/6/8의 고유한 엣지 패턴(획 수, 꺾임 위치 등)으로 확장합니다.

> [!WARNING]
> 4/5/6/8 숫자 간의 엣지 통계 차이만으로 구별하기 어려울 수 있습니다 (특히 5와 6).

### Option C: Canny Edge + 윤곽선 기반 형상 분류

**핵심 아이디어**: Canny edge detection → contour 추출 → 윤곽선 형상 특징(면적, 둘레, 볼록성 등)으로 분류합니다.

> [!WARNING]
> 새로운 인프라(contour extraction, shape descriptor)가 필요하여 범위가 큼.

---

## 4. 트레이드오프

| 기준 | Option A (Sobel+NCC) | Option B (Edge 통계 확장) | Option C (Canny+Contour) |
|------|----------------------|--------------------------|--------------------------|
| BGA 강건성 | ★★★★ | ★★★ | ★★★★★ |
| 구현 복잡도 | 낮음 | 매우 낮음 | 높음 |
| 수정 범위 | 2~3 함수 + 템플릿 재생성 | 1~2 함수 | 신규 모듈 추가 |
| 기존 파이프라인 호환 | 완전 호환 (drop-in) | 호환 | 파이프라인 변경 필요 |
| 정확도 (4/5/6/8 변별) | 높음 | 낮음 (5↔6 혼동) | 높음 |
| 성능 영향 | 무시 가능 | 무시 가능 | Sobel 대비 2~3x 비쌈 |

---

## 5. 추천안: Option A (Sobel 엣지 맵 + NCC 템플릿 매칭)

### 5.1 단계별 실행 계획

#### Phase 1: `overmax_cv`에 Sobel 엣지 맵 함수 추가

**파일**: [image.rs](../rust/overmax_cv/src/image.rs)

```rust
/// Sobel 3×3 엣지 그래디언트 크기 맵을 계산합니다.
/// 입력: BGRA 버퍼, 출력: 0~255 정규화된 엣지 강도 맵
pub fn compute_edge_map(data: &[u8], width: usize, height: usize) -> Vec<u8>
```

- Sobel 3×3 커널 (Gx, Gy)
- `sqrt(Gx² + Gy²)` → 0~255 정규화
- 기존 `to_gray()` 재사용

**파일**: [lib.rs](../rust/overmax_cv/src/lib.rs)
- 퍼블릭 래퍼 함수 `compute_edge_map()` 추가

#### Phase 2: `detect_freestyle_mode()` 전처리 교체

**파일**: [ocr_engine.rs](../rust/overmax_engine/src/detector/ocr_engine.rs#L265-L327)

**변경 내용**:
```diff
 // 현재: 고정 threshold=120 이진화
-let threshold = 120u8;
-binary[y * w + x] = if luma >= threshold { 1 } else { 0 };
 
 // 변경: Sobel 엣지 맵 → 이진화
+let edge_map = overmax_cv::compute_edge_map(&mode_img.bgra, w, h);
+let edge_threshold = /* Otsu or adaptive */;
+binary[y * w + x] = if edge_map[y * w + x] >= edge_threshold { 1 } else { 0 };
```

- 리사이징 로직(50×68 → target) 동일 유지
- 매칭 로직(pixel-by-pixel 비교) 동일 유지
- **변경되는 것은 입력 데이터(edge map)와 템플릿 데이터 뿐**

#### Phase 3: 엣지 기반 템플릿 재생성

**파일**: [result_mode.rs](../rust/overmax_engine/src/detector/templates/result_mode.rs)

- 기존 `collect_templates` 바이너리(`bin/collect_templates.rs`)를 수정하여 엣지 맵 기반 템플릿 수집
- 또는 별도 스크립트로 기존 스크린샷에서 엣지 기반 마스크 추출
- 결과물은 동일 구조 (`ResultModeTemplate { mask: &[u8], ... }`)

#### Phase 4: 검증 및 테스트

- `verify_templates` 바이너리로 기존 스크린샷에 대해 엣지 기반 매칭 정확도 확인
- `test_screenshots` 테스트에서 다양한 BGA 배경 조건의 스크린샷을 추가하여 회귀 검증
- 기존 OCR 폴백 (`set_logo_mode`) 경로는 유지하여 안전망 보존

### 5.2 로고 영역 관련

로고 영역(`detect_logo`)은 현재 **Windows OCR → 키워드 매칭** 방식이므로, 이진화 마스크 매칭이 아닙니다. 엣지 디텍션 전환의 직접 대상이 아닙니다.

다만, 이미 `detect_rect_edges()`가 결과 화면 폴백으로 동작 중이므로:

> [!TIP]
> 로고 영역의 BGA 문제는 **HOG 템플릿 매칭 활성화**로 해결하는 것이 더 적합합니다.
> `logo_templates.rs`에 이미 Freestyle/Online HOG 피처가 존재하며, HOG 자체가 그래디언트 기반이므로 BGA에 강건합니다.
> 이는 별도 태스크로 분리하여 진행하는 것을 권장합니다.

### 5.3 수정 파일 목록

| # | 파일 | 변경 유형 | 설명 |
|---|------|-----------|------|
| 1 | [image.rs](../rust/overmax_cv/src/image.rs) | 함수 추가 | `compute_edge_map()` |
| 2 | [lib.rs](../rust/overmax_cv/src/lib.rs) | 래퍼 추가 | pub fn 노출 |
| 3 | [ocr_engine.rs](../rust/overmax_engine/src/detector/ocr_engine.rs) | 로직 수정 | `detect_freestyle_mode()` 전처리 교체 |
| 4 | [result_mode.rs](../rust/overmax_engine/src/detector/templates/result_mode.rs) | 데이터 재생성 | 엣지 기반 마스크로 교체 |
| 5 | [collect_templates.rs](../rust/overmax_app/src/bin/collect_templates.rs) | 수집기 수정 | 엣지 맵 기반 수집 |

---

## 6. 주의사항 (파이프라인 보존)

- ✅ 기존 폴백 경로 (`set_logo_mode`, Windows OCR) 완전 보존
- ✅ `detect_result_difficulty()`, `detect_badge_mode_diff()` 등 관련 없는 함수 미수정
- ✅ 1-Pass OCR 제약 조건 준수 (템플릿 매칭은 OCR이 아니므로 해당 없음)
- ✅ `HysteresisBuffer` 기반 안정화 흐름 무변경
- ✅ 성능 영향: Sobel 3×3은 50×68=3,400 픽셀에 대해 실행 → ~수 μs, 무시 가능
