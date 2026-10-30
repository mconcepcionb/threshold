# One overlay window per monitor, never resized

Status: accepted

The card was a single transparent window, created at the primary monitor's size and
moved and resized to whichever monitor the cursor was on when a cue fired. On a
two-monitor setup that resize is visible: the webview reflows and stretches while
it happens, so the first card — the one that has to move from one display to
another — looks like it scales into place. The card is a hard cut; anything that
looks like an animation is a bug.

So each monitor gets its own transparent overlay, created at that monitor's
position and size and never touched again. Firing a cue picks the layer under the
pointer and shows it; any other visible layer is hidden first, so a card is never
half-drawn across two displays.

_Consequences_: one extra webview per monitor, which is tens of megabytes and only
paid on multi-display machines. Monitors added or removed while the app runs are
not picked up until restart. Settings, the tray and the scheduler do not care.

Every layer is also shown once, empty, a second and a half after boot. The first
show of a webview paints its surface for the first time; doing that in front of a
cue is what makes the first card differ from the rest.
