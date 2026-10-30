# The Card is a hard cut, with a Floor and a Window

Status: accepted

Every instinct from motion design says to fade the card in over 300ms. We do the
opposite: the Card appears on one frame and disappears on one frame. The point of
the app is the interruption, and a fade is a polite request for attention while a
cut is an event. Control's own location cards, which this borrows from, also do
not scale, track out or type on — the reference is simpler than its imitators.

That hardness needs a contract for dismissal, so a Card has two phases:

- **Floor** (global, 600ms): guaranteed visible. Input passes through the overlay
  and cannot dismiss it. This guarantees the punch is never swallowed by a
  keystroke already in flight.
- **Window** (per cue, default 4s): counted from the cut. After the Floor any
  interaction dismisses immediately; if nothing happens, the Card leaves at the
  end of the Window.

_Consequences_: a Card is a full-screen frame with no transition, so the audio
carries the entrance; a Card with a bad sting reads as a rendering glitch. The
overlay must be click-through during the Floor and interactive after it, which is
a per-platform window flag (`WS_EX_TRANSPARENT`, `ignoresMouseEvents`).
