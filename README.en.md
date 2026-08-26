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

It displays the **unofficial V-Archive difficulty** and a **list of similar-difficulty recommendations** for the currently selected song, right next to the game screen.

- Shows the unofficial difficulty of the currently selected song for each button mode (NM/HD/MX/SC)
- **V-Archive record sync**: imports your V-Archive play records, and can register locally collected records to V-Archive
- **Real-time Rate / Max Combo capture**: automatically detects and saves new records locally as you play (with quick V-Archive upload support when a new best is detected in real time)
- **Similar-difficulty recommendations**: recommends other patterns with a similar difficulty to the current one (sorted by lowest Rate first, then unplayed)
- **Lite Mode**: hides non-essential elements like the recommendation list, showing only essential info (song info, real-time Rate, etc.) in a compact layout (roughly 60px tall)
- **Real-time new-record and quick-upload notifications**: if the Rate detected during play is higher than your existing V-Archive record, an **upload button (⬆)** lights up in the overlay header so you can easily sync your latest record to V-Archive.

The app never reads process memory or modifies game files — it works purely by **window tracking + screen capture**.

### Installation

#### Windows

1. Download the latest `overmax.zip` from [Releases](https://github.com/orphera/overmax/releases).
2. Unzip and run `overmax.exe`.
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

> **Note**: the overlay UI supports multilingual interface (Korean, English, Japanese), which can be switched from the settings window.

### Settings

- Click the **gear button (⚙)** in the overlay header to open the settings window.
- From the settings window you can adjust **overlay size (S / M / L / XL)**, **opacity**, and **display language (한국어 / English / 日本語)**.
- The overlay uses egui's native drag support, so you can smoothly move it anywhere with the mouse; its position is saved automatically.
- **Lite Mode** can be enabled from the settings window. While Lite Mode is active, accidental drag movement is blocked, and the overlay automatically snaps to and locks onto the configured screen corner (top-left, top-right, bottom-left, bottom-right) without jitter.

---

## Developer Guide

### Build & run

```bash
# Requires Rust (rustup)
cargo build --release -p overmax-app
./target/release/overmax-rs
```

### Project structure (Rust)

- `rust/overmax_app`: main application (egui/winit-based native multi-viewport UI and event loop)
- `rust/overmax_engine`: screen capture (DXGI/GDI/X11), detection pipeline, state machines, and telemetry
- `rust/overmax_core`: core state model and common domain types
- `rust/overmax_data`: settings, DB (SQLite), recommendation engine, and V-Archive API integration
- `rust/overmax_cv`: pure-Rust image processing core algorithms (Perceptual Hash, histogram, template-matching engine, etc.)

### Build & release scripts

- `scripts/package-rust.ps1`: automates the full build and produces the release `overmax.zip` and `release_manifest.json` (kept in the same format as the existing release layout)
- `scripts/package-linux.sh`: builds an x86_64 Linux `tar.gz` targeting the Ubuntu 22.04/glibc 2.35 ABI, with a smoke check

---

## Data source

- [V-Archive](https://v-archive.net)

---

## Roadmap

Overmax is currently focused on the following goals per the backlog for the next version (v0.5.0). See [TASKS.md](TASKS.md) for detailed status and issue tracking.

1. **IPC & Extensibility Protocol**: real-time outbound event streaming, MCP (Model Context Protocol) inbound RPC, and recommend-provider consolidation
2. **In-game Utilities & Controls**: global hotkeys support and practice lane blind/curtain overlay
3. **Record Automation**: background auto-upload to V-Archive upon result screen verification
4. **Broaden detected scenes**: support detecting more in-game situations, such as ladder matches (ban/pick and result screens)

---

## License

MIT
