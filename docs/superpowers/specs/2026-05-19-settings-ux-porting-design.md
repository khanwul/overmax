# 2026-05-19 Settings Window UX Porting Design

Settings window implementation in Rust, focusing on porting the UX from the original Python version and fixing scaling issues.

## 1. Goals
- **UX Alignment**: Restore the "S/M/L/XL" button-based scaling and 0.1-step opacity slider from Python.
- **Scaling Isolation**: Fix the bug where changing overlay scale affects all app windows (Settings, Log, etc.).
- **Simplification**: Remove unnecessary settings (hotkeys, interval sliders) as per user request.

## 2. UI Components (egui)

### UI Tab
- **Overlay Scale**:
  - Labels: `S`, `M`, `L`, `XL`
  - Values: `0.75`, `1.0`, `1.25`, `1.5`
  - Implementation: `ui.selectable_label` or `ui.button` in a horizontal layout.
- **Base Opacity**:
  - Slider: `0.1` to `1.0`
  - Step: `0.1`
- **Removed**: Toggle Hotkey section.

### V-Archive Tab
- Keep existing Steam account mapping and manual fetch buttons (4B/5B/6B/8B).
- Ensure styling matches the dark theme.

### System Tab
- **Removed**: Interval sliders (Window Tracker, Screen Capture, Jacket Matcher).
- **Keep**: Auto-update checkbox and Version string.

## 3. Technical Architecture

### Scaling Isolation
The current bug is caused by calling `ctx.set_pixels_per_point()` which affects the entire application.
- **Fix**: Remove all global `pixels_per_point` modifications.
- **New Approach**: Pass the `scale` value to the overlay rendering logic. The overlay window will multiply its internal sizes/positions by this scale, while the Settings and Log windows remain at the default `1.0` scale.

### Data Flow
- Settings are stored in `Value` (JSON) and synced via `SettingsUiContext`.
- Changes in the Settings window should immediately trigger a refresh in the Overlay windows.

## 4. Testing & Validation
- **Visual Check**: Open Settings, Log, and Overlay simultaneously. Change scale and verify only Overlay components change size.
- **Value Persistence**: Verify "S/M/L/XL" selection and Opacity are saved correctly to `settings.user.json`.
- **Functionality**: Verify V-Archive fetch buttons still work after UI cleanup.
