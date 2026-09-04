# GPU ROI Atlas Packing & Multi-Resolution Normalization Architecture

> **상태**: Draft / Proposal  
> **대상 마일스톤**: v0.5.0 (In-game Utilities & Performance Optimization)  
> **관련 모듈**: `overmax_engine::capture::dxgi`, `overmax_engine::detector::roi`, `overmax_engine::detector::roi_config`

---

## 1. 배경 및 동기 (Background & Motivation)

### 1.1 현재 아키텍처의 성능 병목 분석
현재 Overmax의 DXGI 캡처 및 디텍션 파이프라인은 1080p 전체 화면(`1920×1080×4` = **8,294,400 바이트, 약 8.3 MB**)을 매 프레임 GPU VRAM에서 CPU 시스템 메모리로 전송(`Map(D3D11_MAP_READ)`)하고 있습니다.

* **실측 지연 시간 분해 (Telemetry 측정값)**:
  * **GPU ➔ CPU 메모리 복사 (`Map` + DMA)**: **4.0 ~ 5.5 ms** (전체 파이프라인 지연의 ~50%)
  * **CPU 컴퓨터 비전(CV) 연산**: **3.0 ~ 5.0 ms** (`[u32; 32]` 비트마스크, CPU `popcnt`, 2×2 히스토그램 L1 벌점 WTA)
* **문제 정의**:
  * 실제 디텍션 알고리즘에서 참조하는 유효 픽셀은 자켓, 모드, 난이도, 레이트, 스코어, 뱃지 등 **극히 일부분의 ROI(전체의 약 11.6%)**에 불과합니다.
  * 나머지 88.4%의 불필요한 배경/노트 픽셀을 CPU로 퍼 올리기 위해 초당 수십~수백 MB의 PCI-e 대역폭과 메모리 버스 대역폭을 낭비하고 있습니다.

### 1.2 핵심 아이디어 (Core Proposal)
1. **GPU 내부 사전 패킹 (GPU Static ROI Atlas)**:
   * CPU로 전체 1080p 화면을 복사하는 대신, GPU VRAM 상에서 모든 씬의 ROI를 **단 1장의 초소형 아틀라스(Atlas, 512×512) 텍스처에 다닥다닥 모아 붙입니다**.
2. **단 1회의 초소형 Map 전송**:
   * 모아 붙인 아틀라스 텍스처(512×512, **약 1 MB**)만 CPU로 `Map()` 복사합니다.
   * 복사 데이터량 **88% 절감**, 캡처 지연시간 **4.9ms ➔ ~0.5ms로 10배 단축**.
3. **아틀라스 트랜슬레이터 (Atlas Translator)**:
   * 기존 ROI 설정(`(SceneType, "score")`)을 아틀라스 내부의 `(atlas_x, atlas_y, w, h)`로 1:1 매핑해 주는 `AtlasTranslator`를 구축합니다.
   * 기존 디텍션 파이프라인([`ImageView`](../architecture/detection_pipeline.md), `matching.rs`, `digit.rs`, `jacket_matcher.rs`) 코드를 **100% 무수정으로 재사용**합니다.

---

## 2. ROI 통계 및 정적 아틀라스 사양 (Atlas Specifications)

### 2.1 전체 ROI 전수 조사 데이터
[`roi_config.rs`](../../rust/overmax_engine/src/detector/roi_config.rs)에 등록된 모든 씬의 ROI를 전수 집계한 결과입니다.

| 씬 (SceneType) | 포함 ROI 목록 | ROI 개수 | 총 픽셀 수 (px) |
| :--- | :--- | :---: | :---: |
| **Freestyle** | jacket, rate, score, btn_mode, max_combo_badge, diff_panel(NM/HD/MX/SC 4종) | 9 | 24,000 |
| **OpenMatch** | jacket, rate, score, btn_mode, max_combo_badge, diff_panel(NM/HD/MX/SC 4종) | 9 | 24,244 |
| **ResultFreestyle** | jacket, rate, mode, mode_digit, mode_colorbar, diff_panel(4종), max_combo_badge, score | 11 | 79,252 |
| **ResultOpen3** | jacket, rate, openmatch_mode, openmatch_diff, max_combo_badge, score, player_panel | 7 | 56,125 |
| **ResultOpen2** | rate, openmatch_mode, openmatch_diff, max_combo_badge, score, player_panel | 6 | 46,477 |
| **공통 (Common)** | logo (100×100) | 1 | 10,000 |
| **총합 (Total)** | **모든 씬 고유 ROI 일괄 수용** | **43개** | **240,098 px** |

### 2.2 아틀라스 크기 및 대역폭 절감 수치
* **1080p 원본 프레임**: $1920 \times 1080 \times 4\text{ bytes} = 8,294,400\text{ bytes}$ (**7.91 MB**)
* **필요 픽셀 총합**: $240,098\text{ px} \times 4\text{ bytes} = 960,392\text{ bytes}$ (**937.9 KB**)
* **권장 아틀라스 규격**: **$512 \times 512$** ($262,144\text{ px}$, **1.00 MB**)
  * 필요 픽셀(240,098 px)을 **100% 온전히 수용** ($512 \times 512$ 면적의 약 91.6% 패킹 밀도).
  * 2의 거듭제곱(Power of Two) 텍스처 규격으로 GPU 텍스처 정렬 최적화.
* **대역폭 절감율**: **87.9% 절감** ($7.91\text{ MB} \rightarrow 1.00\text{ MB}$)

---

## 3. 해상도 정규화 및 다중 해상도 대응 전략 (Multi-Resolution Normalization)

DJMAX RESPECT V는 다양한 해상도(720p, 1080p, 1440p QHD, 4K UHD) 및 화면비(16:9, 16:10, 21:9)에서 실행됩니다.  
Direct3D11의 고속 복사 API인 `CopySubresourceRegion`은 **1:1 픽셀 크기 복사만 지원**하므로, 해상도 대응을 위한 명확한 전략이 필요합니다.

```
[ 게임 원본 텍스처 ]
  (720p / 1080p / 1440p / 4K / 16:10 레터박스)
         │
         ▼
[ Stage 0: GPU Normalizer (뷰포트 정규화) ]
  * 1080p 네이티브: No-op (Pass-through)
  * 비-1080p (<1080p 또는 >1080p): D3D11 Bilinear Blit / RenderTarget ➔ 1920x1080 Offscreen RT
         │
         ▼ (완벽한 1920x1080 텍스처)
[ Stage 1: Static Atlas Blit ]
  * CopySubresourceRegion x 43회 (VRAM 간 고속 전송, < 50µs)
         │
         ▼ (512x512 Atlas Texture)
[ Stage 2: 1회 Map(D3D11_MAP_READ) ]
  * CPU로 1MB만 전송 (~0.5ms)
         │
         ▼
[ Stage 3: AtlasTranslator ]
  * 기존 ROI 호출을 아틀라스 오프셋으로 매핑 ➔ 기존 CV 파이프라인 무수정 실행
```

### 3.1 1080p보다 작은 해상도 대응 (< 1080p: 720p, 900p, Steam Deck 1280×800)
* **문제 상황**:
  * 720p(1280×720) 환경에서는 1080p 기준 60×60 자켓이 화면 상에 약 40×40 픽셀로 렌더링됩니다.
  * Overmax의 템플릿 매칭 엔진([`templates`](../../rust/overmax_engine/src/detector/templates))은 1080p 폰트 규격에 최적화된 마스크를 사용하므로, 스케일이 다르면 인식이 실패합니다.
* **해결 방안 (GPU Normalization Pass)**:
  * **D3D11 Hardware Bilinear Blit**:
    * 1080p보다 작은 프레임이 감지되면, GPU 상에 상주하는 $1920 \times 1080$ 크기의 `normalized_texture`를 중간 렌더 타겟(RenderTarget)으로 둡니다.
    * Direct3D11의 하드웨어 샘플러(Bilinear Filter)를 사용해 원본 프레임을 1080p로 1차 업스케일 렌더링(Draw Quad 1회, GPU 소요시간 < 10µs)합니다.
    * 1080p로 정규화된 텍스처에서 아틀라스로 `CopySubresourceRegion`을 1:1 복사합니다.
  * **이점**:
    * 아틀라스의 2D 패킹 좌표와 템플릿 크기가 **해상도에 관계없이 항상 1080p 정적 규격으로 영구 고정**됩니다.
    * CPU에서 수행하던 리사이즈 연산이 완전히 소거됩니다.

### 3.2 1080p보다 큰 해상도 대응 (> 1080p: 1440p QHD, 2160p 4K)
* **방식**:
  * 1440p/4K 화면 역시 Stage 0의 `normalized_texture`($1920 \times 1080$)로 하드웨어 Bilinear 다운샘플링 렌더링합니다.
  * 4K의 거대한 프레임(3840×2160×4 = **33.1 MB**)을 CPU로 가져오던 엄청난 대역폭 낭비를 1MB로 압축하여 방지합니다.

### 3.3 종횡비(Aspect Ratio) 정규화 및 블랙바(Letterbox/Pillarbox) 제거
DJMAX RESPECT V의 모든 UI와 판정 레이아웃은 **기준 비율 16:9 ($1.7778$)**에 고정되어 렌더링됩니다.  
모니터가 16:9가 아니거나 창모드 크기가 임의로 조절된 상태에서 단순 스트레칭(Stretching) 다운스케일을 거치면 화면이 찌그러져 템플릿 매칭이 완전히 깨지므로, **GPU Normalizer 단계에서 16:9 종횡비 보정(UV Crop)**을 반드시 선행해야 합니다.

```
[ 비-16:9 원본 화면 (21:9 울트라와이드 or 16:10) ]
┌────────┬─────────────────────────────┬────────┐
│ Black  │     유효 16:9 게임 영역     │ Black  │  <-- Pillarbox (21:9)
│ (버림) │  (UV: u_min ~ u_max 샘플링) │ (버림) │
└────────┴─────────────────────────────┴────────┘
                        │
                        ▼ (GPU Sampler Draw Quad)
┌───────────────────────────────────────────────┐
│     완벽하게 정규화된 16:9 오프스크린 텍스처   │  <-- 1920x1080 (or 640x360)
└───────────────────────────────────────────────┘
```

#### ① 종횡비 판정 공식 (기존 `RoiManager` 수학 로직의 GPU 이식)
입력 프레임의 크기가 $(W, H)$일 때 종횡비 $A = W / H$:
* **허용 오차 범위 ($\epsilon = 0.005$)**:
  * 창모드 테두리 패딩(예: $1920 \times 1053$, $A = 1.823$)이나 $3838 \times 2159$ 등 미세한 16:9 오차는 스케일 왜곡 없이 즉시 16:9로 처리.
* **가로가 더 긴 경우 ($A > 16/9 + \epsilon$, 예: 21:9, 32:9)**:
  * 게임 좌우에 검은색 기둥(Pillarbox)이 존재.
  * 유효 가로 폭: $W_{\text{valid}} = H \times (16/9)$
  * 좌우 오프셋: $X_{\text{offset}} = (W - W_{\text{valid}}) / 2$
  * **GPU 정규화 텍스처 UV 매핑**:
    $$u_{\text{min}} = \frac{X_{\text{offset}}}{W}, \quad u_{\text{max}} = 1.0 - u_{\text{min}}, \quad v_{\text{min}} = 0.0, \quad v_{\text{max}} = 1.0$$
* **세로가 더 긴 경우 ($A < 16/9 - \epsilon$, 예: 16:10 Steam Deck $1280 \times 800$, 4:3 피벗)**:
  * 게임 상하에 검은색 띠(Letterbox)가 존재.
  * 유효 세로 높이: $H_{\text{valid}} = W / (16/9)$
  * 상하 오프셋: $Y_{\text{offset}} = (H - H_{\text{valid}}) / 2$
  * **GPU 정규화 텍스처 UV 매핑**:
    $$u_{\text{min}} = 0.0, \quad u_{\text{max}} = 1.0, \quad v_{\text{min}} = \frac{Y_{\text{offset}}}{H}, \quad v_{\text{max}} = 1.0 - v_{\text{min}}$$

#### ② D3D11 하드웨어 뷰포트/UV 매핑 처리
* D3D11 Vertex Shader / Draw Quad 호출 시 정점의 텍스처 좌표(UV)를 위에서 계산된 $(u_{\text{min}}, v_{\text{min}}) \sim (u_{\text{max}}, v_{\text{max}})$로 설정합니다.
* GPU 래스터라이저와 텍스처 샘플러(Bilinear)가 블랙바 영역을 완전히 건너뛰고 **순수 16:9 게임 화면만 왜곡 없이 오프스크린 텍스처(1080p 또는 360p)에 100% 꽉 채워 렌더링**합니다.
* **결과**:
  * 16:10 모니터, 21:9 울트라와이드 모니터, 임의 크기의 창모드에서도 **아틀라스 내부의 ROI 위치와 종횡비가 1080p 16:9 기준과 1픽셀 오차도 없이 완벽히 일치**하게 됩니다.

### 3.4 [실험적 탐색] 극단적 초저해상도(360p / 540p) 다운샘플링 검증 (Extreme Downsampling Exploration)
* **아이디어 가설**:
  * 만약 GPU에서 1080p가 아니라 **360p($640 \times 360$)** 또는 **540p($960 \times 540$)** 수준으로 대폭 다운샘플링해도 기존 인식률이 유지된다면:
    * 360p 전체 화면 크기 자체가 $640 \times 360 \times 4 = \mathbf{921.6\text{ KB}}$로 **1 MB 미만**입니다.
    * 즉, 아틀라스 패킹 복사 연산마저 생략하고 **360p 프레임 통째를 1회 Map(수십 µs)해도 기존 1080p 전체 복사 대비 대역폭을 89% 절감**할 수 있습니다.
    * 만약 360p 상에서 아틀라스 패킹까지 결합하면 최종 전송량은 **수십 KB(0.1 MB 미만)** 수준으로 극단적 다이어트가 가능합니다.
* **핵심 검증 과제 (Crucial Test Cases)**:
  1. **초소형 ROI의 보존성**:
     * `btn_mode`(1080p 기준 $5 \times 5\text{ px}$): 360p에서는 약 $1.6 \times 1.6\text{ px}$로 축소되므로 Bilinear 블러링에 의한 색상 왜곡 여부 검증 필요.
     * `mode_colorbar`(1080p 기준 $6 \times 96\text{ px}$): 360p에서는 가로폭이 $2\text{ px}$에 불과해 단색성(Solidity) 판정 가능 여부 검증 필요.
  2. **숫자/소수점 세그멘테이션 분해능**:
     * Rate의 소수점(`.`, 1080p에서 약 2~3px)이 360p에서 서브픽셀화되어 증발하지 않는지 확인.
     * Score의 얇은 폰트(8 vs 3, 6 vs 5) 획 분별력 유지 여부.
  3. **자켓 해시 민감도**:
     * $60 \times 60$ 자켓이 $20 \times 20$으로 축소되었을 때 Perceptual Hash(u64) 및 2×2 히스토그램 L1 벌점의 변별력 유지 한계점 측정.
* **단계별 해상도 실험 매트릭스 (Resolution Scaling Benchmark Matrix)**:
  * **1080p** (Reference, 100%): 8.3 MB
  * **720p** (66.7% 스케일): 3.68 MB (안정적 폰트 분해능 유지 예상)
  * **540p** (50.0% 스케일): 2.07 MB (Sweet Spot 후보)
  * **360p** (33.3% 스케일): 0.92 MB (초경량 한계선 테스트)

---

## 4. 아키텍처 컴포넌트 설계

### 4.1 `AtlasLayout` (컴파일 타임 베이킹 정적 테이블)
런타임 힙 할당(Heap Allocation) 및 해시맵 룩업 오버헤드를 **0**으로 만들기 위해, 43개 ROI의 아틀라스 배치는 **컴파일 타임에 `const` 배열로 완전 베이킹(Baking)**합니다.

* **런타임 오버헤드 0**: 런타임 2D 패킹 계산 없음, `HashMap` 할당 없음, 콜드 스타트 지연 0ns.
* **컴파일 타임 안전성**: 슬롯이 512×512 경계를 벗어나거나 중복되는지 여부를 `const_assert` 및 단위 테스트로 컴파일 타임에 100% 검증.

```rust
#[derive(Clone, Copy, Debug)]
pub struct AtlasSlot {
    pub scene: SceneType,
    pub name: &'static str,
    pub src_rect: RawRoiRect,   // 1080p 원본 좌표 (x, y, w, h)
    pub atlas_rect: RawRoiRect, // 512x512 아틀라스 좌표 (x, y, w, h)
}

/// 컴파일 타임에 사전 패킹된 43개 슬롯의 정적 상수 테이블 (Zero Heap Allocation)
pub const ATLAS_SLOTS: [AtlasSlot; 43] = [
    // [Row 0] 60x60 자켓 및 대형 뱃지들 배치
    AtlasSlot { scene: SceneType::Freestyle, name: "jacket", src_rect: RawRoiRect { x: 710, y: 533, width: 60, height: 60 }, atlas_rect: RawRoiRect { x: 0, y: 0, width: 60, height: 60 } },
    AtlasSlot { scene: SceneType::OpenMatch, name: "jacket", src_rect: RawRoiRect { x: 664, y: 533, width: 60, height: 60 }, atlas_rect: RawRoiRect { x: 60, y: 0, width: 60, height: 60 } },
    AtlasSlot { scene: SceneType::ResultFreestyle, name: "jacket", src_rect: RawRoiRect { x: 705, y: 14, width: 60, height: 60 }, atlas_rect: RawRoiRect { x: 120, y: 0, width: 60, height: 60 } },
    // ... (나머지 슬롯들 사전 계산 배치)
];
```

### 4.2 `AtlasTranslator` (컴파일 타임 `const fn` 점프 테이블)
아틀라스 패킹 위치가 컴파일 타임에 고정되므로, **트랜슬레이터의 매핑 규칙 역시 런타임 루프나 룩업 테이블 탐색 없이 `const fn match` 단일 점프 테이블(Jump Table)로 컴파일 타임에 완전 결정**됩니다.

* **루프(Loop) 제로**: `for slot in &ATLAS_SLOTS` 같은 배열 순회조차 불필요.
* **호출 비용 0 (Zero-Cost Inlining)**: `const fn` 및 `#[inline(always)]`를 통해 호출 위치에서 즉시 인라인 레지스터 상수로 환원.
* **타입 안전성**: 정의되지 않은 씬/ROI 조합은 컴파일 타임에 즉시 배제.

```rust
pub struct AtlasTranslator;

impl AtlasTranslator {
    /// 컴파일 타임에 100% 결정되는 정적 점프 테이블 (Zero Runtime Overhead)
    #[inline(always)]
    pub const fn get_roi_for_scene(name: &str, scene: SceneType) -> Option<RoiRect> {
        // 컴파일러가 O(1) 점프 테이블 또는 인라인 상수로 직접 최적화
        match (scene, name.as_bytes()) {
            // [Freestyle]
            (SceneType::Freestyle, b"jacket") => Some(RoiRect { x1: 0, y1: 0, x2: 60, y2: 60 }),
            (SceneType::Freestyle, b"rate") => Some(RoiRect { x1: 60, y1: 0, x2: 164, y2: 22 }),
            (SceneType::Freestyle, b"score") => Some(RoiRect { x1: 164, y1: 0, x2: 268, y2: 24 }),
            (SceneType::Freestyle, b"btn_mode") => Some(RoiRect { x1: 268, y1: 0, x2: 273, y2: 5 }),
            (SceneType::Freestyle, b"max_combo_badge") => Some(RoiRect { x1: 273, y1: 0, x2: 309, y2: 36 }),

            // [ResultFreestyle]
            (SceneType::ResultFreestyle, b"jacket") => Some(RoiRect { x1: 0, y1: 60, x2: 60, y2: 120 }),
            (SceneType::ResultFreestyle, b"score") => Some(RoiRect { x1: 60, y1: 60, x2: 467, y2: 154 }),
            (SceneType::ResultFreestyle, b"rate") => Some(RoiRect { x1: 0, y1: 154, x2: 129, y2: 186 }),
            (SceneType::ResultFreestyle, b"mode") => Some(RoiRect { x1: 129, y1: 154, x2: 469, y2: 229 }),

            // ... (전체 43개 규칙 컴파일 타임 분기)
            _ => None,
        }
    }
}
```

### 4.3 `GpuAtlasCaptureEngine` (D3D11 파이프라인)
[`dxgi.rs`](../../rust/overmax_engine/src/capture/capture_engine/windows/dxgi.rs) 내부에서 텍스처를 처리하는 흐름입니다.

```rust
// 1. 초기 프레임 수신 (DXGI Desktop Duplication)
let desktop_tex = self.duplication.AcquireNextFrame(...)?;

// 2. 해상도 정규화 (1080p가 아닐 때만 1회 Blit)
let source_1080p = if self.is_native_1080p {
    &desktop_tex
} else {
    self.normalizer.render_to_1080p(&self.context, &desktop_tex, self.screen_rect);
    &self.normalized_texture
};

// 3. GPU VRAM 내 아틀라스 배치 복사 (CopySubresourceRegion)
for slot in &self.atlas_layout.slots {
    let src_box = to_d3d11_box(slot.src_rect);
    self.context.CopySubresourceRegion(
        &self.staging_atlas,
        0,
        slot.atlas_rect.x as u32,
        slot.atlas_rect.y as u32,
        0,
        source_1080p,
        0,
        Some(&src_box),
    );
}

// 4. 단 1회 Map (512x512, 1MB)
context.Map(&self.staging_atlas, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
// out_frame에 1MB memcpy (기존 8.3MB 대비 88% 절감)
context.Unmap(&self.staging_atlas, 0);
```

---

## 5. 단계별 구현 로드맵 (Phased Implementation Plan)

### Phase 1: 1080p 정적 아틀라스 패킹 & 트랜슬레이터 프로토타입 (Milestone 1)
* [ ] `AtlasLayout` 2D Shelf 패킹 알고리즘 및 정적 512×512 슬롯 맵 생성기 구현
* [ ] `AtlasTranslator` 모듈 작성 및 기존 `RoiManager`와의 1:1 단위 테스트 검증
* [ ] 1080p 환경에서 `CopySubresourceRegion` 기반 512×512 Staging Texture 1회 Map 검증
* [ ] 기존 템플릿 매칭 및 자켓 매칭 테스트셋(117개) 무손실 통과 확인

### Phase 2: 다중 해상도 정규화 뷰포트 구축 (Milestone 2)
* [ ] D3D11 Draw Quad / Sampler 기반 `GpuNormalizer` 구현
* [ ] 720p, 900p, 1440p, 4K 가상 해상도 캡처 테스트 및 Bilinear 보간 템플릿 매칭 정확도 검증
* [ ] 16:10 (Steam Deck 1280×800) 레터박스 오프셋 연동
* [ ] **[실험] 360p / 540p 초저해상도 인식 한계 벤치마크 테스트**:
  * [ ] 360p($640 \times 360$) 다운샘플링 프레임 대상 Rate 소수점(`.`) 인식률 검증
  * [ ] 360p 대상 Score 얇은 폰트(8 vs 3) 마스크 매칭 정확도 검증
  * [ ] 360p 대상 `btn_mode`(1.6px) 색상 왜곡 여부 및 한계 해상도 도출 (360p vs 540p vs 720p)

### Phase 3: 텔레메트리 벤치마크 및 프로덕션 롤아웃 (Milestone 3)
* [ ] `telemetry.log`에 `atlas_capture_us` 메트릭 추가 및 실측 레이턴시 검증 (목표: < 0.8ms)
* [ ] 최적 해상도 규격(1080p 고정 vs 540p/360p 초경량 모드) 최종 확정 및 기본값 반영
* [ ] D3D 디바이스 로스트 및 예외 상황 시 기존 1080p 전체 캡처로 무중단 안전 폴백(Fallback) 보장

---

## 6. 트레이드오프 및 리스크 완화 (Risks & Mitigations)

| 잠재적 리스크 | 분석 및 영향도 | 완화 전략 (Mitigation) |
| :--- | :--- | :--- |
| **GPU CopySubresourceRegion 43회 API 오버헤드** | D3D11 커맨드 기록 오버헤드 발생 가능 (수십 µs) | VRAM 내부 DMA 복사는 병렬로 수행되므로 총 지연시간은 0.1ms 미만. 필요 시 겹치는 ROI를 병합(Union Bounding Box)하여 복사 횟수를 20회 이하로 축소 |
| **<1080p 업스케일 시 템플릿 매칭 엣지 블러링** | 720p ➔ 1080p Bilinear 업스케일 시 폰트 경계선이 다소 흐려질 수 있음 | 이진화 임계치(Threshold) 허용 범위 검증. 필요 시 Point/Bilinear 혼합 필터링 적용 |
| **360p 초저해상도 시 소수점/초소형 ROI 증발** | 360p에서 Rate 점(`.`) 및 5px 버튼 모드가 1px 이하 서브픽셀로 축소되어 오인식 가능 | 1080p ➔ 720p ➔ 540p ➔ 360p 단계별 해상도 스케일링 테스트셋을 통해 오인식 0%가 보장되는 최소 임계 해상도(Sweet Spot)를 확정 |
| **Linux/X11 호환성** | Direct3D11 전용 API 사용으로 인한 크로스 플랫폼 분기 | Linux 백엔드(XSHM/Wayland)는 기존의 무복사 CPU `ImageView` 파이프라인을 유지하고, Windows DXGI 백엔드에 선택적(Feature Gate)으로 적용 |
