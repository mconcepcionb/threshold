# THRESHOLD

**FEDERAL BUREAU OF CONTROL — ALTERED WORLD EVENT DIVISION**
File THR-001 · Classification: UNRESTRICTED · Status: public alpha
Prepared by ███. ████████, ██████ ████████ · Reviewed by ██████ ████████
Distribution: ████████ ████████, ██████ ██████, and one (1) operator
Portions of this file have been ████████ for your ██████████.

[![ci](https://github.com/mconcepcionb/threshold/actions/workflows/ci/badge.svg)](https://github.com/mconcepcionb/threshold/actions/workflows/ci)

A desktop application that interrupts the working day with a full-screen
typographic card and one impact sound, at moments the operator schedules
personally.

![A card over a live desktop](docs/screenshots/card.jpg)

<div align="center">

**THRESHOLD**<br>
**AN ██████████ OF THE FEDERAL BUREAU OF CONTROL**

**INSPIRED BY**<br>
**CONTROL**<br>
**CONTROL RESONANT**<br>
**REMEDY ENTERTAINMENT**

</div>

*An unofficial ████████. Not affiliated with, endorsed by, or ████████ by Remedy
Entertainment. No ████████ ████████ were consulted. The Bureau depicted here is
████████, its ████████ are ████████, and the ██████ remains ████████.*

> **Field advisement.** This will get annoying fast. It is your computer. A card
> takes the whole screen, swallows one keystroke, and makes a noise. That is the
> product — not an oversight, and not a bug the Bureau is prepared to file. Use
> it to mark the transitions you actually want marked: the end of a focus block,
> "review mail", "go drink water". Not everything. The Bureau has seen ████████
> ████████ when an operator marks ██████████.

## Subject

Not an alarm clock, and not a notification. Nothing about Threshold is polite:
no slide-in, no corner, no little badge asking to be acknowledged. The card
arrives on one frame, with a thump, and is gone after its Window. The working
theory is that a full-screen interruption you scheduled for yourself lands
differently than a banner you dismiss without reading. So far the theory holds,
in the ██████ ██ cases we are permitted to discuss.

Terminology is controlled; please do not improvise your own.

| Term | Bureau definition |
|---|---|
| **Cue** | A scheduled moment: a time or interval, a piece of text, a repeat rule and a late policy. |
| **Card** | What a cue produces: type, centred, over the live desktop. Existential for seconds, then gone. |
| **Sting** | The sound a card lands with. One thump, procedurally generated, pitched per cue. The chosen voice is a 190→52 Hz sweep over a long, low tail. No musical bed. No loop. |
| **Floor** | The guaranteed visible time at the start of a card (600 ms), during which input passes through and cannot dismiss it. |
| **Window** | Total time the card is up, counted from the hard cut. Per cue, default 4 s. |

The full vocabulary and its boundaries live in [GLOSSARY.md](./GLOSSARY.md); the
type system, wrap algorithm and timing are in [docs/design.md](./docs/design.md);
decisions the Bureau would rather not litigate twice are in
[docs/adr](./docs/adr).

## Containment status

Public alpha. Windows is the supported jurisdiction. macOS and Linux compile in
CI and are best-effort; installers are unsigned and there is no auto-update.

The domain crate and the audio renderer are unit-tested, the scheduling decision
is tested as a pure function, and the settings and overlay components have smoke
tests. The Tauri shell itself — tray, window lifecycle, audio device — is covered
by the commands in [CONTRIBUTING.md](./CONTRIBUTING.md), not by an automated UI
driver. The Bureau has ████████ about this, a committee █████ ████████ them, and
no budget to pursue either.

```
app/       the Tauri shell: tray, overlay windows, scheduler thread, sting
core/      the domain: cue model, scheduler plan, wrap solver, config, anchors
examples/  the starter cue file, which is what a fresh install starts from
docs/      design notes and the ████████ worth remembering
```

![The settings window](docs/screenshots/settings.jpg)

## Requisition

Download the installer for your platform from
[Releases](https://github.com/mconcepcionb/threshold/releases). Windows has both
an MSI and an NSIS setup. macOS and Linux builds are provided as best-effort, in
the way that a █████ is provided.

The installers are **not signed**. Windows SmartScreen will warn; macOS Gatekeeper
will require the usual right-click → Open. This is paperwork, not a breach. There
is no updater: to upgrade, download the new build and install it over the old one.

## Local replication

```
cd app
npm ci
npm run tauri dev
```

The app lives in the tray. A normal launch opens the settings window; closing it
hides it rather than quitting, which the Bureau considers a feature and users
consider a mystery. An autostart launch goes straight to the tray. The card is a
transparent, always-on-top layer per monitor, created at that monitor's size. A
second launch raises the running instance instead of starting a second scheduler —
two schedulers in one ████████ is how you get a ██████. The Bureau has a file on
that. The file is ████ pages long.

`THRESHOLD_DEMO=1` fires one card four seconds after boot, so the overlay can be
checked without waiting for a cue. `THRESHOLD_DEMO_NOTIFY=1` posts a notification
three seconds after boot, to exercise the locked-screen path. `/?window=overlay&demo`
renders the card in a plain browser, for screenshots and for convincing the
committee.

## The tray

The tray menu carries **Settings…**, **Fire a test card**, **Reload config**,
**Pause for an hour**, **Resume**, **Mute the sting**, **Start at login**,
**Report a problem**, the **escape hatch** reminder, and **Quit**. It is the only
surface with authority to stop the phenomenon.

**Ctrl+Alt+Shift+Q** clears any card, returns input and focus, and stops the app
firing for an hour, at the OS level, so it works even when the UI is buried under
a stuck layer. This key combination has been ████████. Pause and mute are runtime
state and are not persisted across a restart; a restart is, in effect, a fresh
shift, and the Bureau does not carry over ████████.

While a Windows workstation is locked the card cannot be shown, so a cue that
fires then plays its sting and posts a notification instead. macOS and Linux do
not detect a locked screen, and the Bureau has stopped asking them to.

## The directive

The whole product is one readable TOML file, written only on save (and when a
broken file is replaced at startup).

| Platform | Default path |
|---|---|
| Windows | `%APPDATA%\threshold\cues.toml` |
| macOS | `~/Library/Application Support/threshold/cues.toml` |
| Linux | `$XDG_CONFIG_HOME/threshold/cues.toml` (usually `~/.config/threshold/cues.toml`) |

`examples/cues.toml` is the starter file: twelve cues, half instructions and half
places. A fresh install also appends one `YOU ARE HERE` one-shot due in about
ninety seconds — unless that would cross midnight, at which point the Bureau
declines to create a paradox over a welcome message.

### Top-level fields

```toml
version = 1        # optional; a newer file is rejected with a clear error
floor_ms = 600     # 100..10000
volume = 1.0       # 0..1, the sting level
overlay = "auto"   # auto | permanent | on_demand; omitted when auto
```

### Cue fields

Two required fields, and six optional ones:

```toml
[[cue]]
text = "REVIEW MAIL"          # required, up to 40 characters, drawn uppercase
at = "18:00"                  # required: "18:00" | "every 25m" | "in 25m"
repeat = "weekdays"           # once | daily | weekdays | weekly(mon,wed,fri)
late = "catch_up(20m)"        # exact | catch_up(10m) | next_opportunity
seconds = 6                   # 1..120, default 4
sting = -3                    # finite, -6..6 semitones, default 0
enabled = true                # false disarms without deleting
anchor = "2026-03-10T09:00:00+01:00"  # internal; normally lives in state.toml
```

Durations are `30s`, `25m` or `2h`. `in 25m` is an alias for `every 25m`: an
interval, not a one-shot delay. A one-shot is a wall-clock time with the default
`repeat = "once"`. The naming was reviewed and survived.

### Scheduling doctrine

- **Wall-clock cues** (`at = "18:00"`) repeat according to `repeat`. A one-shot
  is pinned to the next occurrence of its time when it is armed; after it fires
  it has no next occurrence until it is edited or re-armed.
- **Intervals** (`at = "every 25m"`) are anchored to their last fire, so a
  restart continues the chain instead of re-phasing it. A `weekdays` interval
  jumps the weekend and keeps its time of day.
- **Late policies**: `exact` fires only if the tick notices within 2 seconds;
  `catch_up(10m)` fires inside its grace window; `next_opportunity` fires
  however late. Late decisions are made against the due time held in memory, so
  a cue missed while the app was closed is not replayed at the next start. The
  Bureau does not █████ you with ████ ████ schedule.
- **Pause and the escape hatch** advance due cues without firing them. Resuming
  does not replay the hour you were away.
- **Same text and same schedule** in two cues is rejected: the anchor sidecar
  keys a chain by exactly that pair, so two identical cues would collide. One
  of them would win. ██████ wants to find out which.

`state.toml` sits beside the config and holds the interval anchors and one-shot
occurrences. It is written by the app, holds nothing you need to edit, and is
safe to delete: interval chains restart and spent one-shots become armable
again. `cues.toml` is written only when you save (plus the one startup recovery
path in **Problems**). Do not edit `state.toml` by hand. The Bureau ████ ████.

There is a small tool for checking a file without the app:

```
cd core
cargo run --bin next -- ../examples/cues.toml
```

It reads `cues.toml` and the sidecar anchors, and prints each cue's next fire,
its schedule and whether it is disarmed — which is also the answer to "why
didn't it fire?", a question the Bureau receives with ████ ██████████.

### Manifestation modes

The card is drawn by transparent windows — one per monitor, created at that
monitor's size and never resized. Whether they stay on screen is a setting:

- `auto` (default) — permanent layers on a multi-monitor machine, shown on demand
  on a single screen
- `permanent` — the layer is always up; a card is only a DOM change, which is the
  cleanest transition and costs one composition per monitor
- `on_demand` — nothing is on screen between cards; the window is shown per card,
  which is friendlier to screen sharing and games but can flash on the first frame

Monitors added or removed while the app runs are not picked up until restart.
The Bureau acknowledges the monitor count may change. The Bureau has not
██████████ it.

## Amplitude

One number in the settings header: the sting is a single sound, so it gets a
single level. It is saved with the config, not per cue. There is no per-cue
mixer, and there will not be one. ███ ████████ has been ███████.

## Jurisdictions

Windows is the supported target. macOS and Linux compile in CI; always-on-top
over fullscreen apps is not available on Wayland, so the overlay may sit under
other windows there. On macOS the overlay is not placed on every Space, and
focus hand-back is Windows-only. These are known limitations, not anomalies, and
have been filed under ████-███.

## Anomalies

Alpha software. If it breaks, the tray has **Report a problem**. A Rust panic
writes a timestamped `crash-<timestamp>.log` beside your config; the report is
not uploaded, and the tray item can attach the latest crash to a prefilled issue
at [github.com/mconcepcionb/threshold](https://github.com/mconcepcionb/threshold).
Nothing leaves the building without a signature.

If `cues.toml` cannot be parsed, the app keeps it as `cues.toml.broken-<timestamp>`
and starts from the starter set; the settings window shows the parse error and
the path of the kept file. A transient read or permission error does **not** move
the file: the app runs from an in-memory copy and asks you to reload once the
file is readable. Recovery, in this Bureau, means not destroying the evidence.

## Research and development

```
cargo test --manifest-path core/Cargo.toml
cargo test --manifest-path app/src-tauri/Cargo.toml
cd app && npm test && npm run build
```

See [CONTRIBUTING.md](./CONTRIBUTING.md) for prerequisites, formatting, clippy,
the version check and the release process. Changes are recorded in
[CHANGELOG.md](./CHANGELOG.md).

## Clearance

MIT — see [LICENSE](./LICENSE). Bundled type: TeX Gyre Adventor, GUST Font
License — see `core/assets/LICENSE-tex-gyre-adventor.txt`.

Threshold is an unofficial work inspired by the **CONTROL** and **CONTROL
RESONANT** games by Remedy Entertainment. It contains no assets, text or code
from either game, and is not affiliated with or endorsed by Remedy.

## Addendum

████████ ██████ ████████ ████████████ ██████ ████ ████████ ██████ ██████
██████ ██████████ ████ ████████████ ██████ ████ ████████ ██████ ██████████
████. ████████ ██████ ████ ████████, ████████████ ██████ ██████ ████ ████
████████ ██████ ████████.

████ ██████████ ████████ — ██████ ██████ ████ ████████ ██████. ████████
██████ ████████████ ██████ ████ ████████.

*Addendum included for completeness. It is not complete.*

---

**END OF ████████**

*Filed by the Altered World Event Division. Threshold is not, and has never
been, an alarm clock. Please stop writing "alarm clock" on the forms.*
