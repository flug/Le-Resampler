# Contributing to Le Resampler

Thank you for your interest in contributing! Here is everything you need to get started.

## Prerequisites

- [Rust](https://rustup.rs/) stable toolchain
- [Tauri CLI](https://tauri.app/start/): `cargo install tauri-cli`
- On Linux: `libwebkit2gtk-4.1-dev libappindicator3-dev librsvg2-dev libasound2-dev pkg-config`

## Development setup

```bash
git clone https://github.com/flugv1/Le-Resampler.git
cd Le-Resampler
cd src-tauri && cargo tauri dev
```

## Code conventions

- All code, comments, commit messages, and CI steps must be written in **English**
- Rust: follow `cargo fmt` and `cargo clippy -- -D warnings` (both run in CI)
- JS/CSS: vanilla only — no bundler, no framework, no build step
- **100% test coverage is required.** Any new business logic must live in a testable helper function, not directly in `commands.rs` or hardware-dependent functions

## Running tests

```bash
cd src-tauri
cargo test --lib          # unit tests
cargo clippy --lib --tests -- -D warnings
cargo fmt --check
```

Coverage (requires nightly):

```bash
cargo install cargo-tarpaulin
cargo tarpaulin
```

## Architecture overview

| Layer | Location | Notes |
|-------|----------|-------|
| Backend | `src-tauri/src/` | Rust — Tauri IPC commands, SQLite, audio, scanner, export |
| Frontend | `src/` | Vanilla JS + CSS, no build step |
| IPC | `invoke('command_name', { camelCaseArgs })` | Rust commands use `snake_case` params |

See `CLAUDE.md` (not committed) for the full architecture reference.

## What never to commit

- `CLAUDE.md` and any AI/agent config files (`.claude/`, `.cursor/`, etc.)
- Build artefacts (`src-tauri/target/`, `src-tauri/gen/`)
- Databases (`*.db`, `*.db-shm`, `*.db-wal`)

## Submitting a pull request

1. Fork the repository and create a branch from `main`
2. Make your changes with tests if applicable
3. Ensure `cargo test --lib`, `cargo clippy`, and `cargo fmt --check` all pass
4. Open a pull request with a clear title and description of the change

## Reporting bugs

Please use the [Bug Report](.github/ISSUE_TEMPLATE/bug_report.md) issue template.

## Questions

Open a [GitHub Discussion](https://github.com/flugv1/Le-Resampler/discussions) or reach out at flugv1@gmail.com.
