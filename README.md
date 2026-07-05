# Le Resampler

**Le Resampler** is a desktop audio sample manager built for music producers who use physical samplers (Akai MPC and compatible devices). Scan your library, organise files by category and tags, preview them, then export them to an SD card in one click — automatically recreating the folder structure expected by your sampler (`/kick/`, `/snare/`, `/loop/`…).

Built with [Tauri](https://tauri.app/) (Rust + Vanilla JS): lightweight, native, available on macOS, Windows and Linux.

[![Buy Me A Coffee](https://www.buymeacoffee.com/assets/img/custom_images/orange_img.png)](https://buymeacoffee.com/flugv1t)

---

## Features

- **Library scanning** — Add a folder and Le Resampler recursively indexes all audio files (WAV, MP3, FLAC, OGG, AIFF…)
- **Auto-refresh** — Re-scan known folders in one click to pick up newly added files
- **Metadata display** — BPM, key, duration, category and type — auto-inferred from filename and audio metadata
- **Inline editing** — Edit category, BPM, key and tags directly in the table
- **Tag system** — Add/remove custom tags per sample; filter the list by tag
- **Audio preview** — Click ▶ on any row to preview; Space bar to stop. Auto-play mode available
- **Volume control** — Slider in the player bar
- **Multi-select export** — Check samples individually or use "Select All", then click **Export** to copy them to a destination folder. Files are automatically sorted into subfolders by category (`/kick/`, `/snare/`, `/loop/`…)
- **MPC Sampler compatible** — Export directly to an SD card formatted for the Akai MPC. The folder structure created by Le Resampler matches the MPC's expected layout, so your samples are immediately browsable on the device
- **100% test coverage** — Rust backend fully tested with [cargo-tarpaulin](https://github.com/xd009642/tarpaulin)
- **CI/CD** — GitHub Actions pipeline (fmt, clippy, tests, coverage, weekly releases)

---

## Installation

### macOS — Homebrew

```bash
brew tap flugv1/Le-Resampler https://github.com/flugv1/Le-Resampler
brew install --cask le-resampler
```

### Windows & Linux

Download the latest installer from the [Releases](https://github.com/flugv1/sampli/releases) page:

| Platform | File |
|----------|------|
| macOS (Apple Silicon + Intel) | `Le Resampler_*_universal.dmg` |
| Windows | `Le Resampler_*_x64-setup.exe` |
| Linux | `le-resampler_*_amd64.AppImage` |

---

## Development

### Prerequisites

- [Rust](https://rustup.rs/) (stable)
- [Tauri CLI](https://tauri.app/start/): `cargo install tauri-cli`
- On Linux: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev libasound2-dev pkg-config`

### Run in development

```bash
cd src-tauri
cargo tauri dev
```

### Build for production

```bash
cd src-tauri
cargo tauri build
```

---

## Project Structure

```
le-resampler/
├── src/                  # Frontend (Vanilla JS + CSS)
│   ├── index.html
│   ├── main.js
│   └── style.css
└── src-tauri/            # Rust backend (Tauri)
    ├── src/
    │   ├── audio.rs      # Audio preview (rodio)
    │   ├── commands.rs   # Tauri IPC commands
    │   ├── db.rs         # SQLite (rusqlite)
    │   ├── export.rs     # Copy/organise samples
    │   ├── metadata.rs   # BPM / key / category inference
    │   └── scanner.rs    # Recursive folder scanning
    └── Cargo.toml
```

---

## Running Tests

```bash
cd src-tauri
cargo test --lib
```

For coverage (requires nightly):

```bash
cargo install cargo-tarpaulin
cargo tarpaulin
```

---

## License

MIT
