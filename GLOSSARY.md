# Threshold

A personal desktop tool that interrupts the day with a full-screen typographic
card and a single impact sound, at moments you schedule yourself.

## Language

**Cue**:
A scheduled moment: a time or interval, a piece of text, a repeat rule, a late
policy, a sting pitch offset. The persistent unit the user authors.
_Avoid_: Alarm, reminder, timer, event

**Card**:
The visual event a Cue produces: full-screen overlay, type only, over the live
desktop. Exists for seconds, then is gone.
_Avoid_: Overlay, popup, notification, title screen

**Sting**:
The single impact sound a Card lands with. No musical bed, no loop, no
escalation.
_Avoid_: Sound, chime, tone, alert

**Floor**:
The guaranteed visible time at the start of a Card, during which input passes
through and cannot dismiss it. Global setting, default 600ms.
_Avoid_: Fade-in, animation delay, minimum duration

**Window**:
A Card's total visible time, counted from the hard cut. The part after the Floor
dismisses on a keypress or mouse-button press. Per cue, default 4s.
_Avoid_: Hold, timeout, display time

### Scheduling

**Fire**:
The moment a Cue produces a Card. Not the moment the Cue was configured for -
those differ under a late policy.
_Avoid_: Trigger, ring, go off

**Late policy**:
A Cue's rule for what to do when its due time passes while the app is still
running. `exact` fires only if noticed within 2 seconds; `catch_up(10m)` fires
inside a grace window; `next_opportunity` fires however late. A cue missed while
the app was closed is not replayed: the next fire is the next future occurrence.
_Avoid_: Missed alarm handling, retry

**Anchor**:
The instant a chain counts from. For an interval Cue it is the last scheduled
due time, so "every 25 minutes" continues after a restart instead of re-phasing.
For a one-shot it is the single occurrence the Cue is armed for, so a restart
cannot re-arm a shot that has passed. Persisted in `state.toml`, keyed by text
plus schedule.
_Avoid_: Start time, base, epoch

**Disarm**:
Suppressing an individual Cue without deleting it, via `enabled = false` or the
armed toggle in the settings window. There is no global disarm; the tray has
Pause for that.
_Avoid_: Disable, mute, snooze

### Surfaces

**Config**:
The persisted Cue list, a TOML file in the platform's user-config directory.
Edited either by the app's settings window or by hand. The file is the more
expressive editor: the window exposes presets for repeat and late policy.
_Avoid_: Database, store, settings.json
