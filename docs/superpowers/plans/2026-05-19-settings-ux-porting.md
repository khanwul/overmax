# Settings Window UX Porting Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port Python settings UX to Rust, specifically fixing scaling isolation and adding S/M/L/XL scale buttons.

**Architecture:** Remove global scale modification (`pixels_per_point`) from the settings UI. Implement per-window scaling in overlays. Refactor `settings_ui.rs` to match Python's simplified layout.

**Tech Stack:** Rust, egui (eframe)

---

### Task 1: Fix Scaling Isolation & Cleanup `settings_ui.rs`

**Files:**
- Modify: `rust/overmax_app/src/settings_ui.rs`

- [ ] **Step 1: Remove global scale modification**
Search for and remove any calls to `ctx.set_pixels_per_point()` in `settings_ui.rs`. This ensures the Settings window and Log window remain at 1.0 scale.

- [ ] **Step 2: Refactor UI Tab - Scale Buttons**
Replace the scale slider with S/M/L/XL buttons.
```rust
// In ui_tab
ui.horizontal(|ui| {
    ui.label("크기");
    for (label, val) in [("S", 0.75), ("M", 1.0), ("L", 1.25), ("XL", 1.5)] {
        if ui.selectable_label(current_scale == val, label).clicked() {
            overlay.insert("scale".into(), json!(val));
        }
    }
});
```

- [ ] **Step 3: Refactor UI Tab - Opacity Slider**
Set the opacity slider to step by 0.1.
```rust
ui.add(Slider::new(&mut opacity, 0.1..=1.0).step_by(0.1).text("기본 투명도"))
```

- [ ] **Step 4: Remove Unused Sections**
Remove the Hotkey section in `ui_tab` and the interval sliders in `system_tab`.

- [ ] **Step 5: Verify implementation compile**
Run: `cargo check` in `rust/overmax_app`

- [ ] **Step 6: Commit**
```bash
git add rust/overmax_app/src/settings_ui.rs
git commit -m "ui: port python settings UX and fix global scaling issue"
```

### Task 2: Implement Per-Overlay Scaling

**Files:**
- Modify: `rust/overmax_app/src/overlay_ui.rs`
- Modify: `rust/overmax_app/src/native_app_viewports.rs` (if needed for window size)

- [ ] **Step 1: Apply scale to overlay window**
In the overlay rendering loop (e.g., `render_overlay_ui`), ensure the local `pixels_per_point` is set for that specific window context.
```rust
// Inside the window's update/render function
let scale = settings.get("overlay").and_then(|o| o.get("scale")).and_then(|s| s.as_f64()).unwrap_or(1.0) as f32;
ctx.set_pixels_per_point(scale); 
```
*Note: Since each overlay is a separate viewport/window in eframe, calling `ctx.set_pixels_per_point` within that window's update loop should only affect that window.*

- [ ] **Step 2: Verify scaling**
Run the app and change scale in Settings. Verify that only Overlay windows change size, while Settings and Log remain stable.

- [ ] **Step 3: Commit**
```bash
git add rust/overmax_app/src/overlay_ui.rs
git commit -m "feat: implement per-window scaling for overlays"
```

### Task 3: Final Polish & Validation

**Files:**
- Modify: `rust/overmax_data/src/settings.rs`

- [ ] **Step 1: Ensure settings normalization**
Verify `normalize_settings` in `overmax_data` correctly handles the fixed scale values (0.75, 1.0, 1.25, 1.5).

- [ ] **Step 2: Final Integration Test**
1. Run application.
2. Open Settings -> UI Tab.
3. Click 'XL'. Verify overlay grows.
4. Drag opacity slider. Verify 0.1 steps.
5. Check System tab. Verify clean layout.
6. Restart app. Verify settings persisted in `settings.user.json`.

- [ ] **Step 3: Commit**
```bash
git add rust/overmax_data/src/settings.rs
git commit -m "test: final validation of settings port"
```
