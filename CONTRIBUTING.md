# Contributing

Threshold is a small desktop app with a deliberately small surface. Bug reports,
reproductions and focused pull requests are welcome.

## What the two crates are

```
core/            threshold-core: cue model, scheduler, config, anchors, layout
app/src/         React UI (settings + overlay)
app/src-tauri/   Tauri shell: windows, tray, audio device, OS edges
```

Decisions live in `docs/adr/`; the shared vocabulary is in `GLOSSARY.md`. If a
change makes an ADR or a README statement false, update it in the same pull
request.

## Prerequisites

- Rust stable (1.90 or newer; the repo pins stable in `rust-toolchain.toml`)
  with `rustfmt` and `clippy`
- Node 22.22+, 24.15+, or 26+
- Windows: WebView2 (installed by default on Windows 10/11) and the MSVC build
  tools
- Linux: `libwebkit2gtk-4.1-dev`, `libappindicator3-dev`, `librsvg2-dev`,
  `patchelf`, `libgtk-3-dev`, `libasound2-dev`, `libssl-dev`
- macOS: Xcode command-line tools

## Build and run

```sh
cd app
npm ci
npm run tauri dev
```

## Checks

Run these before opening a pull request; CI runs the same commands.

```sh
cargo fmt --manifest-path core/Cargo.toml --check
cargo fmt --manifest-path app/src-tauri/Cargo.toml --check
cargo clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings
cargo clippy --manifest-path app/src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path core/Cargo.toml
cargo test --locked --manifest-path app/src-tauri/Cargo.toml
node scripts/check-versions.mjs
cd app && npm ci && npm test && npm run build
```

Scheduling and layout behaviour belongs in `core/`, where it can be tested
without a window. If you fix a bug in the shell, add the smallest test that
would have caught it; `core/src/scheduler.rs` is the model for a decision that
moved out of the shell so it could be tested.

## Screenshots

The images in the README are rendered from the UI itself:

```sh
cd app
npx playwright install chromium   # once; or use --channel chrome below
npm run screenshots
```

`scripts/screenshots.mjs` starts Vite and stages a desktop around the real
overlay and settings window. It writes `docs/screenshots/card.jpg` (1920×1080)
and `settings.jpg` (1600×1000). Pass options to the script directly:

```sh
node scripts/screenshots.mjs --scene card
node scripts/screenshots.mjs --wallpaper nebula
node scripts/screenshots.mjs --wallpaper ~/Pictures/wall.jpg --text "REVIEW|MAIL"
```

The default backdrop is an Unsplash photo, cached in `.screenshots-cache/` and
never committed; a built-in gradient is used when the download fails. A local
Chrome or Edge is used automatically when the bundled Chromium is missing.

## Versioning and releases

- Version lives in `app/package.json`, `app/src-tauri/tauri.conf.json`,
  `app/src-tauri/Cargo.toml` and `core/Cargo.toml`. `scripts/check-versions.mjs`
  enforces that they agree, and CI runs it.
- Every user-visible change adds a line to `CHANGELOG.md` under `[Unreleased]`.
  At release time the section is renamed to the version and dated.
- A release is a tag: `git tag v0.2.0 && git push origin v0.2.0`. The release
  workflow builds installers for Windows, macOS and Linux and publishes them to
  GitHub Releases.
- Installers are currently **unsigned**. Windows SmartScreen and macOS
  Gatekeeper will warn; the release notes say so.

## Pull requests

Keep the change focused, say what it changes and how it was checked, and note
any platform you could not test. Please do not include generated build output,
`node_modules`, or screenshots that are not referenced by the docs.
