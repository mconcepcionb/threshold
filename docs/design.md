# Design

Threshold interrupts your day with a full-screen typographic card and one impact
sound, at moments you schedule. Everything here serves that single event.

## Reference

The Card borrows from the location cards in Remedy's *Control* (2019): enormous
white type, all caps, one block, no chrome, over a scene you are already in.

What the reference actually does, from [Fonts In Use](https://fontsinuse.com/uses/48689/control-video-game),
[Kotaku](https://kotaku.com/the-fonts-in-control-make-its-world-feel-way-creepier-1837714580)
and frame-stepping the game:

| | |
|---|---|
| Face | ITC Avant Garde Gothic Bold — wide and geometric, round O/C/G |
| Motion | opacity only. No scale, no tracking, no glitch, no type-on |
| Layout | one block, two lines max, centred, slightly below middle |
| Depth | the type sits *behind* the character; her silhouette cuts through it |
| Sound | a low hit with a long tail, landing with the type, not before it |

The depth occlusion is the least-copied part and the reason the cards feel
physical rather than pasted on. It cannot be reproduced over live windows, so we
substitute a soft horizontal scrim and a wide soft shadow for separation.

## Type

**TeX Gyre Adventor Bold** — a metric-compatible free clone of ITC Avant Garde
Gothic, GUST Font License, bundled at `core/assets/`.

```
fill       #F2F0EA
tracking   -0.02em
line-height 0.86
size       solved, not fixed — 280px maximum at 1920 wide, 110px preferred floor
alignment  centred on both axes
```

### Sizing

Size is a function of the text, not a constant, so a 40-character instruction and
the word `WAREHOUSE` both look deliberate.

1. Uppercase the text. Manual line breaks are honoured.
2. Target line count: `ceil(words / 2)`, clamped to 1–3.
3. Break lines by scoring every partition of the words, minimising the sum of
   squared deviations from the mean line width and discouraging lines narrower
   than 35% of it. One-word last lines are possible; they are penalised, not
   forbidden.
4. Scale the type directly so the widest line fills 84% of the viewport width,
   capped at 280px. The 110px figure is a preferred floor: a single
   unbreakable word shrinks below it rather than overflowing the frame.
5. Centre the measured block on both axes.

Deterministic given `(text, breaks, viewport)`, so the solver is unit-testable in
Rust without a renderer.

### Legibility

The visual decisions were settled against a real white document, and the answer
was not a tight halo. White type with a dark halo survives a dark editor and
fails over white: the rim reads as an outline and the letters go hollow.

The default is therefore a **scrim**: a full-width horizontal band behind the
block, transparent at the top and bottom, at most 55% black at its centre, plus a
soft shadow on the type. Desktop remains visible above and below. It reads as
Ground in the reference — the cards in *Control* always sit on something darker
than pure white — and it costs no assets.

The type never depends on the desktop being dark, which is the only way this
works as a daily tool.

## Timing

```
t = 0        hard cut in. Sting starts on the same frame. Floor begins.
t = 600ms    Floor ends. Input passes through until here, and cannot dismiss.
             From here a keypress or mouse-button press dismisses.
t = window   Card ends (default 4s), hard cut out.
```

**Floor** is global; **Window** is per cue. Both are counts of real time, never
animations — there is no fade in either direction. A card that appears softly is
a notification; this one is an event.

_Consequences_: 4 seconds of white block over a desktop is a legitimate
flashbang, and a hard cut with no sound reads as a rendering bug. The Sting is
not decoration, it is half the effect.

On Windows, a locked screen cannot show a window, so a cue that fires while the
workstation is locked skips the Card entirely: the Sting plays, and a
notification carries the text. Firing the card behind a lock would spend it
unseen. macOS and Linux have no lock detection and will attempt the card.

## Sting

Synthesised in Rust, no assets (see ADR-0003). The chosen voice is `hit = 2`
(`sweep`) at depth 1.8:

```
~6 ms    crack: broadband noise, high-passed, no resonator
22 ms    body dirt: the same noise low-passed at 320 Hz — mass, not hiss
95 ms    thump: sine 190 → 52 Hz over 70 ms, driven through tanh so it has harmonics
~570 ms  chest: 46 Hz, undriven, so it stays felt rather than heard as a note
620 ms   room sub: 31 Hz under everything, the weight you feel but a laptop loses
30 ms    pre-delay into a two-band tail: lows (below 220 Hz) through a reverb
         1.7× longer than the highs
```

A thump is mostly noise; a bell is a few partials. Both failed first attempts were
one of those — sustained noise through a resonator (rattle) and clean partials
with a long clean tail (bell). This one keeps the noise but confines it to the
first few milliseconds, with nothing resonant to ring in.

The two-band tail is what makes it deep rather than just low. One reverb for the
whole hit decays at one rate, so the room sounds like a second copy of the hit;
giving the lows their own, slower reverb puts weight *behind* the impact instead
of beside it. The tail is algorithmic (Freeverb-style combs and allpasses), not
convolution. `--depth` scales the dry chest and room-sub layers and lengthens the
chest decay; the low reverb length comes from `--t60`. Depth defaults to 1.8.

Depth alone is rumble, not a thump: a spectrum of the deep tail showed the attack
was 23–70 Hz and nothing else. The thump the ear wants lives at 100–250 Hz, which
is also the only band a laptop speaker can reproduce. `--hit` selects three
voicings of that:

| | |
|---|---|
| `0` hard | 95 → 48 Hz, body stops in 45 ms, knock 165 / 247 Hz at 1.45 / 0.55 |
| `1` knock | same pitch, longer body (75 ms), knock raised to 1.85 / 0.70 — the box leads |
| `2` sweep | 190 → 52 Hz over 70 ms, knock lower — the classic falling contour |

**Chosen voice: `sweep` on the deep tail** (t60 2.6s, depth 1.8). It is the
default in `app/src-tauri/src/audio.rs`, and the same code is rendered offline by
the `sting` binary:

```
cd app/src-tauri
cargo run --bin sting -- --out sting.wav
```

Per-cue variation is a semitone offset applied to the body, knock, chest and room
sub; the reverb parameters stay fixed, so the character is stable while the pitch
changes. The config and the settings window allow -6..+6 semitones, and the model
rejects anything outside that or non-finite.

## Palette

Concrete grey, off-white, near-black, and one signal red (`#C4262E`) used only
for the settings window's warnings, errors and missed cues. The tray has no
armed-state colour. Nothing else.
