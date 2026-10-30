# Changelog

All notable changes to Threshold are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project
uses [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Audio output moved to cpal 0.18. The rendered sting is unchanged; the output
  device path was smoke-tested with the demo cards.
- Frontend toolchain: Vite 8, `@vitejs/plugin-react` 6, React 19, TypeScript 7.
- Test environment: jsdom 30 (Node 22.22+, 24.15+ or 26+).
- Rust MSRV is now 1.90, which Tauri 2.12 already required.
- Release workflow uses `tauri-action` v1.

## [0.1.0] — 2026-10-30

First public alpha. Windows is the supported target; macOS and Linux are
compile-checked but not released.

### Added

- Scheduled **cues**: a wall-clock time (`18:00`), a rolling interval
  (`every 25m`), a repeat rule, and a late policy.
- Full-screen **cards** on the monitor under the pointer: one hard cut in, a
  click-through Floor, then interaction dismisses or the Window ends it.
- Procedural **stings** rendered in Rust, pitched per cue, with a volume knob.
- A settings window with the next cue, per-row arm/fire/remove, inline editing,
  and a raw TOML file that stays the source of truth.
- Tray controls: Settings, Fire a test card, Reload config, Pause for an hour,
  Resume, Mute the sting, Start at login, Report a problem, Quit.
- `Ctrl+Alt+Shift+Q` escape hatch: clears the card, returns input, and pauses
  firing for an hour.
- Interval anchors persist in `state.toml` beside the config, so an interval
  chain survives a restart without rewriting the file a human edits.
- Locked-screen fallback: the sting plays and an OS notification carries the
  text.
- Crash reports written as `crash-<timestamp>.log` beside the config.
- `next` CLI: prints every cue's next fire without starting the app.
- `sting` CLI: renders the impact to a WAV file for tuning.

### Fixed before the first tag

- Interval cues no longer disarm themselves after their first card.
- Pause and the escape hatch actually stop the scheduler, and cues due during a
  pause are skipped rather than replayed.
- The settings window round-trips `overlay`, can no longer save an empty cue
  list before loading, and asks before overwriting an external edit.
- A settings window opened at login no longer appears in front of the user.
- A one-shot is pinned to the occurrence it was armed for, so a restart cannot
  re-arm it; re-enabling a spent cue re-arms it.
- Weekday intervals keep their time of day when they skip the weekend.
- A DST spring-forward gap no longer aborts the search for the next fire.
- Durations with multibyte suffixes and chrono's out-of-range values are errors
  instead of panics.
- Text that cannot wrap shrinks below the preferred floor instead of
  overflowing the screen.
- A card is released by a Rust watchdog even if the webview never reports its
  end.
- Writes are uniquely named, flushed, and retried; a transient read error no
  longer moves a valid config aside.
- macOS transparent overlays are enabled (`macOSPrivateApi`).

[Unreleased]: https://github.com/mconcepcionb/threshold/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/mconcepcionb/threshold/releases/tag/v0.1.0
