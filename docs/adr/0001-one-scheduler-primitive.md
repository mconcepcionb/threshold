# One scheduler primitive: the Cue

Status: accepted

The app needs pomodoro intervals and fixed-time reminders. These could be two
engines (a session state machine with pause/skip/cycles, plus a cron-like
reminder list), but they overlap badly: a pomodoro's end and a reminder can land
on the same second, and each engine would need its own config surface, persistence
and tests.

Instead there is one primitive, the **Cue**, and one scheduler that fires it. A
pomodoro is not a feature; it is a way of creating a Cue (`at = "every 25m"`).
Interval cues anchor to their last fire and persist that anchor, so "every 25
minutes" survives a restart and resumes its chain after a wake.

_Considered_: a session engine with explicit pause/resume. Rejected because pause
is the only state it adds, and the tray's pause already covers it.
