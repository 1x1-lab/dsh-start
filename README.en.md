# DSH-start 🐳

> The auto-install & auto-start host for [DSH (DeepSeek Harness)](https://github.com/deepseek-ai/deepseek-harness) — cross-platform, acrylic UI, bilingual.

**Stack**: Tauri v2 (Rust) 🦀 + Vue 3 💚 + Vite ⚡ + TypeScript 🔷

[简体中文](README.md) · **English**

## ✨ Highlights

- 🚀 **One-click DSH install**: first-run wizard checks the environment, then installs `@deepseek-ai/dsh` into an app-managed directory (your global npm stays clean); missing Node.js? Guided install via winget / brew / apt
- 🔁 **Hosted auto-start**: "Auto Start" launches on login; "Auto-restart on Crash" brings DSH back with exponential backoff (up to 5 retries)
- 🕵️ **External instance detection**: started DSH yourself in a terminal? It's recognized — status shows "Running · External" instead of a false "not running", and the app never fights you for control
- 📞 **Dual-channel callback restart** (no user commands needed, same restart routine):
  - **HTTP**: `POST http://127.0.0.1:3081/api/restart` (localhost-only + CORS whitelist); `GET /api/status` for status
  - **CLI**: registers `dsh-start restart` into PATH — DSH's own bash/pwsh tools call it directly, forwarded via single-instance
- 🎛️ **Configurable control port**: defaults to DSH port + 1, auto-scans the next 10 ports when occupied, or pin your own in Settings — rebinds instantly on save. Port conflicts solved
- 🔄 **Smart updates**: the "Update to vX.X.X" button only appears when the npm registry actually has a newer version
- 🖥️ **Console**: stat cards (port / control port / version / uptime), start / stop / restart, live logs (in-memory ring + rolling file)
- 🪟 **Acrylic UI**: transparent window with real system blur, Linear-style two-layer layout, custom titlebar, double-click to maximize
- 🧷 **System tray**: left-click toggles show/hide, right-click menu with status-aware items (external instances are read-only); closing the window minimizes to tray
- 🌍 **Bilingual**: Chinese ⇄ English in Settings — UI and tray menu switch together; more languages easy to add

## 🚀 Quick Start (Dev)

Prerequisites: Node.js 18+, Rust (stable), platform [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)

```bash
npm install
npm run tauri:dev
```

Build 📦:

```bash
npm run tauri:build
```

## 📖 Usage

1. First run opens the setup wizard 🧙: environment check (Node.js / DSH) → click "Install DSH" (nothing auto-starts)
2. Two ways to start DSH:
   - Enable "Auto Start": starts now and on every login
   - Click "Start DSH" on the console
3. "Open DSH Console" opens `http://127.0.0.1:3080` in your system browser (port configurable)
4. Closing the window minimizes to tray; quit via the tray menu 🚪

## 📞 Callback Restart

| Channel | How | Notes |
| --- | --- | --- |
| HTTP | `POST http://127.0.0.1:3081/api/restart` | Control port = DSH port + 1 by default (customizable); localhost-only, CORS restricted to the DSH web origin |
| CLI | `dsh-start restart` | Requires "Register Callback Command"; callable from DSH's bash/pwsh tools |

Both trigger: stop DSH → start → readiness probe → status events & tray notification. Repeat requests within 1.5s are throttled ⏱️

## 🗂️ Project Layout

```
src/                Vue frontend (console / wizard / logs / settings / i18n)
src-tauri/
  src/
    manager.rs      Process hosting: spawn / monitor / backoff restart / readiness & external probes
    runtime.rs      Node detection + managed npm install + version resolve / update check
    control.rs      127.0.0.1 control HTTP endpoint (rebindable + port-conflict fallback)
    cli.rs          dsh-start restart callback shim & PATH registration
    tray.rs         Tray: status text / bilingual menu / status-aware items
    commands.rs / settings.rs / logger.rs / state.rs
  tauri.conf.json   Window (transparent acrylic), bundling (NSIS / dmg / deb / appimage), icons
```

## 🏗️ Cross-platform Builds

GitHub Actions builds all three platforms (`.github/workflows/build.yml`): CI on push, draft Release on `v*` tags.
Artifacts: Windows NSIS installer, macOS dmg, Linux deb / AppImage.

## 🔒 Security Notes

- The control endpoint binds `127.0.0.1` only; CORS allows just `http://127.0.0.1:<dsh-port>` / `http://localhost:<dsh-port>`
- v1 has no auth and exposes only two verbs: `status` and `restart`
- DSH user data (default `~/.dsh`, governed by `DSH_HOME`) lives apart from this app's managed directory — we only manage the process and installation
