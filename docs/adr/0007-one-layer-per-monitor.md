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
pointer and emits the card only to that layer; every layer hears the event and
checks its own label. A layer that was showing the previous card is cleared
first, so two displays never draw two cards at once.

Whether a layer stays on screen is the `overlay` setting. `permanent` (and
`auto` on a multi-monitor machine) creates every layer visible and click-through;
a card is then only a DOM change. `on_demand` hides the layer between cards,
which is friendlier to screen sharing and games but can show a first-frame flash.
The escape hatch clears every layer and returns input.

_Consequences_: one extra webview per monitor, which is tens of megabytes and only
paid on multi-display machines. Monitors added or removed while the app runs are
not picked up until restart. Settings, the tray and the scheduler do not care.

There is no separate warm-up show: a permanent layer is composed at boot, and an
on-demand layer is shown when a card needs it. If a card is on screen for longer
than its Window plus the Floor and a two-second margin, a watchdog in Rust ends it
regardless of what the page does.
