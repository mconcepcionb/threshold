# Cinematic Alarm

A personal desktop tool that interrupts the day with a full-screen typographic
card and a single impact sound, at moments you schedule yourself.

## Language

**Cue**:
A scheduled moment: a time or interval, a piece of text, a repeat rule, a late
policy, a sting variant. The persistent unit the user authors.
_Avoid_: Alarm, reminder, timer, event

**Card**:
The visual event a Cue produces: full-screen overlay, type only, over the live
desktop. Exists for seconds, then is gone.
_Avoid_: Overlay, popup, notification, title screen

**Sting**:
The single impact sound a Card lands with. No bed, no loop, no escalation.
_Avoid_: Sound, chime, tone, alert

**Floor**:
The guaranteed visible time at the start of a Card, during which input passes
through and cannot dismiss it. Global setting, default 600ms.
_Avoid_: Fade-in, animation delay, minimum duration

**Window**:
A Card's total visible time, counted from the hard cut. The part after the Floor
dismisses on any interaction. Per cue, default 4s.
_Avoid_: Hold, timeout, display time

### Scheduling

**Fire**:
The moment a Cue produces a Card. Not the moment the Cue was configured for —
those differ under a late policy.
_Avoid_: Trigger, ring, go off

**Late policy**:
A Cue's rule for what happens when its moment passes while the app or machine
is not running. `exact` skips, `catch_up` fires inside a grace window,
`next_opportunity` fires at the next start or wake regardless.
_Avoid_: Missed alarm handling, retry

**Anchor**:
The instant an interval Cue's chain counts from. Persisted, so "every 25
minutes" continues after a restart instead of re-phasing.
_Avoid_: Start time, base, epoch

**Disarm**:
Suppressing a Cue without deleting it. Global disarm is the tray's only
scheduling control.
_Avoid_: Disable, mute, snooze

### Surfaces

**Config**:
The persisted Cue list, a TOML file in the platform's user-config directory.
Edited either by the app's settings window or by hand; both are equally
expressive.
_Avoid_: Database, store, settings.json
