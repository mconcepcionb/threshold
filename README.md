# Threshold

A desktop app that interrupts your day with a full-screen typographic card and
one impact sound, at moments you schedule yourself.

<!-- GIF of a card firing over a real desktop goes here -->

> **This will get annoying fast, it's your computer.** A card takes the whole
> screen, swallows one keystroke, and makes a noise. That is the product. Schedule
> it for the transitions you actually want marked — the end of a focus block,
> "review mail", "go drink water" — not for everything.

## What it is

Not an alarm clock, and not a notification. Nothing about Threshold tries to be
polite: the card appears on one frame, with a thump, and is gone four seconds
later. The bet is that a full-screen interruption you scheduled for yourself lands
differently than a banner you dismiss without reading.

- **Cues** are scheduled moments: a time or interval, a piece of text, a repeat
  rule and a late policy.
- **Cards** are what a cue produces: type, centred, over your live desktop.
- **Stings** are the sound a card lands with. One thump, procedurally generated,
  pitched per cue. The chosen voice is a 190→52 Hz sweep over a long, low tail.

Vocabulary and domain boundaries live in [GLOSSARY.md](./GLOSSARY.md); the type
system, wrap algorithm and timing are in [docs/design.md](./docs/design.md);
decisions worth remembering are in [docs/adr](./docs/adr).

## Status

It runs, it fires, it thumps, and the parts that can be wrong are tested.

```
app/       the Tauri shell: tray, overlay window, scheduler thread, sting
core/      the domain: cue model, scheduler, wrap solver, config, anchors
examples/  the starter cue file, which is also what a fresh install starts from
docs/      design notes and the decisions worth remembering
```

## Running it

```
cd app
npm install
npm run tauri dev
```

The app lives in the tray. It opens the settings window on first run; closing
that window hides it rather than quitting. The card is a transparent, always-on-top
layer per monitor, created at that monitor's size and shown for the length of one
interruption. A second launch raises the running instance instead of starting a
second scheduler.

`THRESHOLD_DEMO=1` fires one card four seconds after boot, so the overlay can be
checked without waiting for a cue. `THRESHOLD_DEMO_NOTIFY=1` posts a notification
three seconds after boot, to exercise the locked-screen path. `/?window=overlay&demo`
renders the card in a plain browser, for screenshots.

The tray menu carries **Pause for an hour**, **Mute the sting**, **Start at
login**, **Report a problem**, and the reminder that **Ctrl+Alt+Shift+Q** is the
escape hatch: it clears any card and stops the app firing for an hour, at the OS
level, so it works even when the UI is buried under a stuck layer. — the first two are the emergency stop, and neither is persisted across a
restart. While the screen is locked the card cannot be shown, so a cue that fires
then plays its sting and posts a notification instead.

## Build order

1. ~~**Spike**~~ — decided the type, the scrim and the sting, then deleted.
2. ~~**Domain**~~ — cue model, scheduler, late policies, wrap solver, config.
3. ~~**Shell**~~ — Tauri app, tray, overlay, settings, single instance.
4. **Portability** — Linux and macOS builds, CI matrix.
5. **Packaging** — signed installers, start at login.

## Overlay modes

The card is drawn by transparent windows — one per monitor, created at that
monitor's size and never resized. Whether they stay on screen is a setting:

- `auto` (default) — permanent layers on a multi-monitor machine, shown on demand
  on a single screen
- `permanent` — the layer is always up; a card is only a DOM change, which is the
  cleanest transition and costs one composition per monitor
- `on_demand` — nothing is on screen between cards; the window is shown per card,
  which is friendlier to screen sharing and games but can flash on the first frame

## Volume

One number in the settings header: the sting is a single sound, so it gets a
single level. It is saved with the config, not per cue.

[`examples/cues.toml`](./examples/cues.toml) is the starter file: twelve cues,
half instructions and half places, with the field reference in the comments. The
format is two required fields per cue and five optional ones with defaults, so a
cue you write by hand is two lines.

There is a small tool for checking a file without the app:

```
cd core
cargo run --bin next -- ../examples/cues.toml
```

It prints each cue's next fire, its schedule and whether it is disarmed — which is
also the answer to "why didn't it fire?".

Interval cues remember when they last fired in `state.toml`, beside the config. It
is written only by the app, holds nothing you need to edit, and is safe to delete:
the chains simply restart. `cues.toml` is written only when you save.

## Platforms

Windows is the supported target. macOS and Linux build and run; always-on-top
over fullscreen apps is not available on Wayland, so the overlay may sit under
other windows there.

## Problems

Alpha software. If it breaks, the tray has **Report a problem**, and a crash
writes `crash.log` beside your config and opens a prefilled issue at
[github.com/mconcepcionb/threshold](https://github.com/mconcepcionb/threshold).

## Licence

MIT. Bundled type: TeX Gyre Adventor, GUST Font License — see
`core/assets/LICENSE-tex-gyre-adventor.txt`.
