# Song Detection Context Refactoring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor `GameSessionState` to use a bundled `PlayContext` for song metadata, improving state management and type safety.

**Architecture:** Introduce a `PlayContext` struct to hold `song_id`, `mode`, and `diff`. Update `GameSessionState` to use `Option<PlayContext>` instead of individual fields.

**Tech Stack:** Rust

---

### Task 1: Refactor `GameSessionState` Structure

**Files:**
- Modify: `rust/overmax_core/src/game_state.rs`

- [ ] **Step 1: Write failing tests for the new structure**

Since this is a breaking change to existing code, I will first update the existing tests to use the new structure and watch them fail to compile.

```rust
#[cfg(test)]
mod tests {
    use super::{GameSessionState, PlayContext};

    #[test]
    fn song_id_zero_is_valid_when_state_is_stable() {
        let state = GameSessionState {
            context: Some(PlayContext {
                song_id: 0,
                mode: "4B".to_string(),
                diff: "MX".to_string(),
            }),
            is_stable: true,
            is_max_combo: false,
            rate: None,
        };

        assert!(state.is_valid());
    }

    #[test]
    fn unstable_state_is_not_valid() {
        let state = GameSessionState {
            context: Some(PlayContext {
                song_id: 1,
                mode: "4B".to_string(),
                diff: "MX".to_string(),
            }),
            is_stable: false,
            is_max_combo: false,
            rate: Some(99.1),
        };

        assert!(!state.is_valid());
    }

    #[test]
    fn rate_none_and_zero_are_not_stored() {
        let mut state = GameSessionState::detecting();
        assert!(!state.should_store_rate());

        state.rate = Some(0.0);
        assert!(!state.should_store_rate());

        state.rate = Some(1.0);
        assert!(state.should_store_rate());
    }
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p overmax_core`
Expected: FAIL (Compilation error)

- [ ] **Step 3: Implement `PlayContext` and update `GameSessionState`**

```rust
use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub struct PlayContext {
    pub song_id: u32,
    pub mode: String,
    pub diff: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GameSessionState {
    pub context: Option<PlayContext>,
    pub is_stable: bool,
    pub is_max_combo: bool,
    pub rate: Option<f32>,
}

impl GameSessionState {
    pub fn detecting() -> Self {
        Self {
            context: None,
            is_stable: false,
            is_max_combo: false,
            rate: None,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.context.is_some() && self.is_stable
    }

    pub fn should_store_rate(&self) -> bool {
        self.rate.is_some_and(|rate| rate > 0.0)
    }
}

impl Default for GameSessionState {
    fn default() -> Self {
        Self::detecting()
    }
}

impl fmt::Display for GameSessionState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.is_stable { "STABLE" } else { "DETECTING" };
        let mc_status = if self.is_max_combo { " (MAX COMBO)" } else { "" };
        
        match &self.context {
            Some(ctx) => write!(
                f,
                "[{status}] {} | {} | {}{mc_status}",
                ctx.song_id, ctx.mode, ctx.diff
            ),
            None => write!(f, "[{status}] None | None | None{mc_status}"),
        }
    }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p overmax_core`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add rust/overmax_core/src/game_state.rs
git commit -m "refactor: bundle song detection state into PlayContext"
```
