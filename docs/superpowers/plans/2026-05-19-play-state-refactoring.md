# Play State Refactoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor the song detection pipeline in `rust/overmax_app/src/play_state.rs` to use the atomic `PlayContext` and update the state detection logic accordingly.

**Architecture:** 
1. `RawPlayState` will be updated to hold `Option<PlayContext>` instead of individual fields.
2. `PlayStateDetector::detect` will create `PlayContext` only when all fields are present and confident.
3. `GameSessionState` (from `overmax_core`) already supports the new structure; we just need to update how it's populated.
4. History buffer logic (`push_raw`) will be updated to use `context.is_some()`.

**Tech Stack:** Rust

---

### Task 1: Update Imports and Struct Definitions

**Files:**
- Modify: `rust/overmax_app/src/play_state.rs`

- [ ] **Step 1: Update imports and `RawPlayState` definition**

```rust
use overmax_core::{GameSessionState, PlayContext}; // Update imports
// ...
#[derive(Clone, Debug, PartialEq)]
struct RawPlayState {
    context: Option<PlayContext>, // Use PlayContext
    is_max_combo: bool,
}
```

- [ ] **Step 2: Update `PlayStateDetector` definition**

```rust
pub struct PlayStateDetector {
    history_size: usize,
    history: VecDeque<Option<RawPlayState>>,
    last_stable_state: Option<GameSessionState>,
    ocr_done_for: Option<PlayContext>, // Use PlayContext for OCR key
}
```

### Task 2: Update `PlayStateDetector::detect` Implementation

**Files:**
- Modify: `rust/overmax_app/src/play_state.rs`

- [ ] **Step 1: Update `detect` logic to create `PlayContext`**

```rust
    pub fn detect(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        song_id: Option<u32>,
        ocr: &OcrDetector,
    ) -> GameSessionState {
        let mode = detect_button_mode(frame, rois);
        let (diff, confident) = detect_difficulty(frame, rois);
        let is_max_combo = detect_max_combo(frame, rois);

        let context = if let (Some(sid), Some(m), Some(d)) = (song_id, mode, diff) {
            if confident {
                Some(PlayContext {
                    song_id: sid,
                    mode: m,
                    diff: d,
                })
            } else {
                None
            }
        } else {
            None
        };

        let raw = RawPlayState {
            context: context.clone(),
            is_max_combo,
        };
        self.push_raw(raw.clone()); // Simplified push_raw signature

        if let Some(stable) = self.stable_raw() {
            let stable = stable.clone();
            let rate = self.detect_rate_once(frame, rois, &stable, ocr);
            let state = stable_state(&stable, rate);
            self.last_stable_state = Some(state.clone());
            return state;
        }

        GameSessionState {
            context: raw.context,
            is_stable: false,
            is_max_combo: raw.is_max_combo,
            rate: None,
        }
    }
```

### Task 3: Update Helper Methods

**Files:**
- Modify: `rust/overmax_app/src/play_state.rs`

- [ ] **Step 1: Update `push_raw`**

```rust
    fn push_raw(&mut self, raw: RawPlayState) {
        if self.history.len() == self.history_size {
            self.history.pop_front();
        }
        let valid = raw.context.is_some();
        self.history.push_back(valid.then_some(raw));
    }
```

- [ ] **Step 2: Update `detect_rate_once`**

```rust
    fn detect_rate_once(
        &mut self,
        frame: &CapturedFrame,
        rois: &RoiManager,
        raw: &RawPlayState,
        ocr: &OcrDetector,
    ) -> Option<f32> {
        let ctx = raw.context.as_ref()?;
        if self.ocr_done_for.as_ref() == Some(ctx) {
            return self.last_stable_state.as_ref().and_then(|state| state.rate);
        }
        self.ocr_done_for = Some(ctx.clone());
        // ... rest of logic remains same, just access ctx.mode/diff
```

- [ ] **Step 3: Update `stable_state` and remove `state_key`**

```rust
fn stable_state(raw: &RawPlayState, rate: Option<f32>) -> GameSessionState {
    GameSessionState {
        context: raw.context.clone(),
        is_stable: true,
        is_max_combo: raw.is_max_combo,
        rate,
    }
}
```

### Task 4: Fix Unit Tests

**Files:**
- Modify: `rust/overmax_app/src/play_state.rs`

- [ ] **Step 1: Update `marks_state_stable_after_repeated_valid_frames` test**
Update the test assertions to match the new `GameSessionState` structure.

- [ ] **Step 2: Verify with `cargo test --lib play_state`**
Note: It might need `cargo check` if other files are broken, but we should aim for logic correctness.

---
