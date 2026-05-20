# Atomic Play Context Sync Implementation Plan (Updated)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Improve Rate/MaxCombo saving reliability by unifying pattern recognition (Jacket, Mode, Difficulty) and OCR (Rate) into an atomic, synchronized state that must be stable before any data is recorded.

**Architecture:** Refactor the detection pipeline to perform all detections (including Rate OCR) synchronously for every frame. A single stabilization buffer will track the entire `AtomicPlayContext` (Song, Mode, Diff, Rate, MaxCombo). The state is only "committed" when all components are stable together.

**Tech Stack:** Rust, egui (UI), Windows OCR, OpenCV (via overmax_cv).

---

### Task 1: Define Atomic Context Types

**Files:**
- Modify: `rust/overmax_core/src/game_state.rs`

- [ ] **Step 1: Update `PlayContext` to include all record-related fields**

```rust
// rust/overmax_core/src/game_state.rs

#[derive(Clone, Debug, PartialEq)]
pub struct PlayContext {
    pub song_id: u32,
    pub mode: String,
    pub diff: String,
    pub rate: f32,          // Moved from GameSessionState
    pub is_max_combo: bool, // Moved from GameSessionState
}
```

- [ ] **Step 2: Update `GameSessionState` to simplify**

```rust
// rust/overmax_core/src/game_state.rs

#[derive(Clone, Debug, PartialEq)]
pub struct GameSessionState {
    pub context: Option<PlayContext>,
    pub is_stable: bool,
}
```

- [ ] **Step 3: Commit**

```bash
git add rust/overmax_core/src/game_state.rs
git commit -m "refactor: define unified PlayContext for atomic sync"
```

---

### Task 2: Implement Per-Frame Atomic Detection

**Files:**
- Modify: `rust/overmax_app/src/play_state.rs`
- Modify: `rust/overmax_app/src/detection_pipeline.rs`

- [ ] **Step 1: Refactor `PlayStateDetector` to perform OCR every frame**

Move OCR call inside the per-frame loop and remove `detect_rate_once`.

- [ ] **Step 2: Update `RawPlayState` to use the new `PlayContext`**

```rust
// rust/overmax_app/src/play_state.rs

#[derive(Clone, Debug, PartialEq)]
struct RawPlayState {
    context: Option<PlayContext>,
}
```

- [ ] **Step 3: Synchronize Jacket detection result with PlayState**

Ensure `DetectionPipeline` passes the current frame's `song_id` to `PlayStateDetector` without internal "stickiness" during the stabilization phase.

- [ ] **Step 4: Commit**

```bash
git add rust/overmax_app/src/play_state.rs rust/overmax_app/src/detection_pipeline.rs
git commit -m "feat: implement synchronous per-frame atomic detection"
```

---

### Task 3: Unified Stabilization and Commit Logic

**Files:**
- Modify: `rust/overmax_app/src/play_state.rs`

- [ ] **Step 1: Implement stabilization for the entire `PlayContext`**

The state is only stable if `(id, mode, diff, rate, max_combo)` are identical for N frames.

- [ ] **Step 2: Adjust stabilization window (e.g., 5 frames) to handle OCR noise**

- [ ] **Step 3: Commit**

```bash
git add rust/overmax_app/src/play_state.rs
git commit -m "feat: enforce atomic stability check for all record fields"
```

---

### Task 4: Final Integration and Record Saving Fix

**Files:**
- Modify: `rust/overmax_app/src/native_app_recommend.rs`

- [ ] **Step 1: Update `drain_detection_results` to use the new `state.context` fields**

- [ ] **Step 2: Verify that records are only saved when the unified context is stable**

- [ ] **Step 3: Commit**

```bash
git add rust/overmax_app/src/native_app_recommend.rs
git commit -m "fix: align record saving with unified atomic context"
```
