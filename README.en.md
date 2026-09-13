# Overmax

[한국어](README.md) | [English](README.en.md)

An overlay tool that shows unofficial V-Archive-based difficulty ratings in real time on the DJMAX RESPECT V song-select screen.

> **🚀 Native Rust app**: Overmax is built as a native Rust application for a lightweight, fast experience.
> - **Lightweight and fast**: minimal memory footprint and executable size, with strong overall runtime performance.
> - **Minimal external dependencies**: no heavy OpenCV or OS OCR dependency — uses a pure-Rust Perceptual Hash/histogram jacket-matching engine and a pure-Rust CV template-matching engine instead.
> - **Fully backward compatible**: works with existing users' settings (`settings.json`) and local records (`record.db`), and preserves the existing portable environment as-is.

---

## User Guide

### What does it do?

It displays the **unofficial V-Archive difficulty** and an **intelligent personalized recommendation list** for the currently selected song, right next to the game screen.

- **Real-time unofficial difficulty**: shows unofficial difficulty ratings for the current song across all button modes (NM/HD/MX/SC)
- **Intelligent multi-dimensional recommendation engine**: analyzes player skill profiles (TrueSkill SC/Pad 2-track) based on Top 50 records to suggest optimal practice/challenge songs with reason badges (`BEST`, `RETRY`, `REST`, `PUSH`, etc.)
- **Real-time Rate / Max Combo capture**: purely Rust-native template matching detects score and rate without error (with quick V-Archive upload notification on new records)
- **Ultra-low-latency 0.62ms GPU ROI Atlas screen capture**: 512×512 atlas packaging and double-buffered staging textures deliver sub-millisecond responsiveness without in-game stuttering
- **Zero-config Windows HDR (scRGB) auto-detection**: detects monitor color space and SDR white level automatically, restoring color fidelity via a 64KB high-speed inverse LUT (DXGI capture)
- **Real-time local IPC streaming and remote control**: broadcast real-time game events (SSE `GET /events`) and accept JSON-RPC 2.0 remote calls for OBS broadcast widgets and third-party tools
- **Lite Mode**: compact layout (approx. 60px tall) minimizing screen obstruction with jitter-free automatic corner snapping
- **Global multilingual support**: native UI in Korean, English, and Japanese with automatic OS display language detection

The app never reads process memory or injects into the game — it works purely and safely through **window tracking + screen capture**.

### Installation

#### Windows

1. Download the latest `overmax.zip` from [Releases](https://github.com/orphera/overmax/releases).
2. Unzip and run `overmax.exe`. (Fully compatible with both portable mode and installed mode at `%LOCALAPPDATA%\Overmax`)
3. Launch DJMAX RESPECT V while it's running and detection starts automatically.

> **Auto-update**: on startup, the app automatically checks for a newer version and for song DB (`image_index.db`) updates, and applies them.

#### Linux (early support)

1. Download `overmax-linux-x86_64.tar.gz` from Releases and extract it into a directory you can write to.
2. Run `./overmax` from that directory. Settings and cache are stored in the run directory.
3. Launch DJMAX RESPECT V in the same session via Proton/XWayland.

For supported environments, how to check compatibility, current implementation status, and unsupported features, see the [Linux support guide](docs/guides/linux-support.en.md).

### Requirements

- Windows 10 or later (64-bit), or x86_64 Linux meeting the early-support scope above
- DJMAX RESPECT V (Steam)
- An active internet connection while running (for V-Archive data, DB, and app update checks)

> ⚠️ **Important: game display settings**
> * **Borderless fullscreen (windowed fullscreen) is recommended**: to have the overlay window display correctly on top of the game while playing, set the game's display option to **"Borderless Fullscreen"**.
> * **If using exclusive fullscreen**: running the game in regular **"Fullscreen"** mode causes the overlay to render behind the game instead of on top of it, due to Windows OS and the game's anti-cheat (XIGNCODE3) restrictions. If you must use exclusive fullscreen, drag the overlay window onto a **secondary monitor** in a dual-monitor setup and use it there instead.

> **Note**: the overlay UI supports multilingual interface (Korean, English, Japanese), which can be switched from the settings window at any time.

### Settings

- Click the **gear button (⚙)** in the overlay header to open the settings window.
- From the settings window you can adjust **overlay size (S / M / L / XL)**, **opacity**, and **display language (한국어 / English / 日本語)**.
- The overlay can be moved anywhere on screen via mouse drag, and its position is saved automatically.
- **Lite Mode** can be enabled from the settings window. While Lite Mode is active, accidental drag movement is blocked, and the overlay automatically snaps to and locks onto the configured screen corner (top-left, top-right, bottom-left, bottom-right) without jitter.
- **Advanced settings**: supports capture backend switching (DXGI / GDI), GPU ROI Atlas acceleration, local IPC server toggling, data directory inspection, and opening the system folder directly.

---

## Developer Guide

### Build & run

```bash
# Requires Rust (rustup)
cargo build --release -p overmax-app
./target/release/overmax-rs
```

### Project structure (Rust)

- `rust/overmax_app`: main application (egui/winit-based native multi-viewport UI, settings/debug windows, IPC server, and event loop)
- `rust/overmax_engine`: screen capture (DXGI GPU Atlas / GDI / X11), HDR 2-anchor inverse transform, detection pipeline, state machines, and telemetry
- `rust/overmax_core`: core state model (`VerifiedPlayEvent`, etc.) and common domain types
- `rust/overmax_data`: settings (`settings.user.json`), DB (SQLite `record.db`), recommendation engine, and V-Archive API integration
- `rust/overmax_cv`: pure-Rust image processing core algorithms (Perceptual Hash, histogram, template-matching engine, etc.)

### Build & release scripts

- `scripts/package-rust.ps1`: automates the portable build and produces `overmax.zip` and `release_manifest.json`
- `scripts/package-msix.ps1`: builds Windows Desktop Bridge (Centennial) Microsoft Store / MSIX packages
- `scripts/package-linux.sh`: builds an x86_64 Linux `tar.gz` targeting the Ubuntu 22.04/glibc 2.35 ABI, with a smoke check

---

## Data source

- [V-Archive](https://v-archive.net)

---

## Roadmap (v0.5.0)

Overmax is currently focused on the following goals per the backlog for the next version (v0.5.0). See [TASKS.md](TASKS.md) for detailed status and issue tracking.

1. **In-game Utilities & Controls**: global/in-game hotkeys support, practice lane blind/curtain overlay (SUDDEN / HIDDEN / BLIND effects)
2. **Record Automation**: background auto-upload to V-Archive upon result screen verification
3. **Broaden detected scenes**: support detecting ladder matches (ban/pick screen, waiting lobby, and result screen)

---

## License

MIT
