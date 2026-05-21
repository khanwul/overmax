# ROI Manager 리팩토링 계획

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (추천) 또는 superpowers:executing-plans를 사용하여 이 계획을 실행하세요.

**Goal:** 씬별 독립적인 ROI 설정을 지원하고, 씬 감지에 따라 자동으로 ROI를 동기화하는 구조로 리팩토링합니다.

**Architecture:** 
- `RoiManager`에 씬 타입(SceneType) enum을 도입.
- 기존 ROI 하드코딩을 씬별 설정 데이터 구조로 이동.
- `DetectionPipeline`이 `GameSessionState`를 통해 씬 정보를 `RoiManager`에 업데이트.

---

### Task 1: 씬 정보 모델 정의 및 데이터 구조화

- [ ] **Step 1: `overmax_core`에 `SceneType` enum 정의**
Modify: `rust/overmax_core/src/lib.rs`
```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SceneType {
    Freestyle,
    Online,
}
```

- [ ] **Step 2: `overmax_data`에 씬별 ROI 설정 데이터 구조 작성**
Create: `rust/overmax_data/src/scene_config.rs`
(씬별 좌표 맵 정의)

### Task 2: RoiManager의 동적 ROI 로딩으로 리팩토링

- [ ] **Step 1: `RoiManager` 구조체 수정**
Modify: `rust/overmax_app/src/roi.rs`
(SceneType 필드 추가 및 씬 전환 메서드 추가)

- [ ] **Step 2: 하드코딩 ROI 분리**
Modify: `rust/overmax_app/src/roi.rs`
(get_roi에서 씬 설정 기반 조회로 변경)

### Task 3: DetectionPipeline의 씬 감지 및 ROI 동기화

- [ ] **Step 1: `DetectionPipeline` 씬 업데이트 로직**
Modify: `rust/overmax_app/src/detection_pipeline.rs`
(현재 씬 판단 후 rois.set_scene() 호출)
