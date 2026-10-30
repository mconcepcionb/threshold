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
substitute a tight dark halo for contrast and a wide soft shadow for separation.

## Type

**TeX Gyre Adventor Bold** — a metric-compatible free clone of ITC Avant Garde
Gothic, GUST Font License, bundled at `core/assets/`.

```
fill       #F2F0EA
tracking   -0.02em
line-height 0.86
size       solved, not fixed — 280px maximum at 1920 wide, 110px floor
alignment  centred on both axes
```

### Sizing

Size is a function of the text, not a constant, so a 40-character instruction and
the word `WAREHOUSE` both look deliberate.

1. Uppercase the text. Manual line breaks are honoured.
2. Target line count: `ceil(words / 2)`, clamped to 1–3.
3. Break lines by dynamic programming: minimise the sum of squared slack per
   line, with a heavy penalty for a one-word last line. Greedy wrapping produces
   orphans; this does not.
4. Binary-search the font size so the longest resulting line fills at most 84% of
   the viewport width. Cap at 280px, floor at 110px.
5. Centre the measured block on both axes.

Deterministic given `(text, breaks, viewport)`, so the solver is unit-testable in
Rust without a renderer.

### Legibility

The spike settled this against a real white document, and the answer was not the
halo. White type with a dark halo survives a dark editor and fails over white: the
rim reads as an outline and the letters go hollow.

The default is therefore a **scrim**: a soft elliptical dark patch behind the
block only, ~62% black at centre fading out by ~72%, desktop still visible at the
edges, plus a soft shadow on the type. It reads as Ground in the reference — the
cards in *Control* always sit on something darker than pure white — and it costs
no assets. A tight halo remains a second treatment behind the same interface, and
`difference` (`mix-blend-mode: difference`) is kept as a curiosity: always legible,
but it inverts colour over the desktop, so it is not the reference.

The type never depends on the desktop being dark, which is the only way this
works as a daily tool.

## Timing

```
t = 0        hard cut in. Sting starts on the same frame. Floor begins.
t = 600ms    Floor ends. Input passes through until here, and cannot dismiss.
             From here any interaction dismisses.
t = window   Card ends (default 4s), hard cut out.
```

**Floor** is global; **Window** is per cue. Both are counts of real time, never
animations — there is no fade in either direction. A card that appears softly is
a notification; this one is an event.

_Consequences_: 4 seconds of white block over a desktop is a legitimate
flashbang, and a hard cut with no sound reads as a rendering bug. The Sting is
not decoration, it is half the effect.

A locked screen cannot show a window, so a cue that fires while the workstation is
locked skips the Card entirely: the Sting plays, and a notification carries the
text. Firing the card behind a lock would spend it unseen.

## Sting

Synthesised in Rust, no assets (see ADR-0003):

```
6 ms     crack: broadband noise, high-passed, no resonator
30 ms    body dirt: the same noise low-passed at 240 Hz — mass, not hiss
75 ms    thump: sine 95 → 48 Hz in 35 ms, driven through tanh so it has harmonics
280 ms   chest: 46 Hz, undriven, so it stays felt rather than heard as a note
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
of beside it. `--depth` scales the sub layers and lengthens the low tail
(1.0 default, 1.8 in the `deep` variant).

Depth alone is rumble, not a thump: a spectrum of the deep tail showed the attack
was 23–70 Hz and nothing else. The thump the ear wants lives at 100–250 Hz, which
is also the only band a laptop speaker can reproduce. `--hit` selects three
voicings of that:

| | |
|---|---|
| `0` hard | 95 → 48 Hz, body stops in 45 ms, knock 165 / 247 Hz at 1.45 / 0.55 |
| `1` knock | same body, knock raised to 1.85 / 0.70 — the box leads |
| `2` sweep | 190 → 52 Hz over 70 ms, knock lower — the classic falling contour |

**Chosen voice: `sweep` on the `deep` tail** (t60 2.6s, depth 1.8), as of the
spike. It is close rather than final — the default in `spike/audio` and in
`Params::default` so the code and the page agree.

Per-cue variation is a semitone offset applied to the **body only** (±3 st); the
tail is identical every time, so the character stays stable while the pitch
changes.

## Palette

Concrete grey, off-white, near-black, and one signal red (`#C4262E`) used only for
the tray state when a cue is armed. Nothing else.
