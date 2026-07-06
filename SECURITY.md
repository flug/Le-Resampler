# Security Policy

## Supported versions

| Version | Supported |
|---------|-----------|
| Latest release | ✅ |
| Older releases | ❌ |

Only the latest release receives security fixes. Please update before reporting.

## Reporting a vulnerability

**Do not open a public GitHub issue for security vulnerabilities.**

Send a detailed report by email to **flugv1@gmail.com** with:

- A description of the vulnerability and its potential impact
- Steps to reproduce the issue
- The version of Le Resampler you are using
- Your operating system and version

You can expect an acknowledgement within **48 hours** and a status update within **7 days**.

If the vulnerability is confirmed, a fix will be released as soon as possible and you will be credited in the release notes (unless you prefer to remain anonymous).

## Scope

Le Resampler is a local desktop application. It reads audio files from your filesystem and writes to a destination folder you choose. It does not transmit any data over the network.

Areas of particular interest:

- Path traversal in the export logic (`src-tauri/src/export.rs`)
- Arbitrary file read/write via Tauri IPC commands
- SQLite injection in `src-tauri/src/db.rs`
