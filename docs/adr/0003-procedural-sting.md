# Audio is synthesised in Rust, not played by the webview

Status: accepted

The Sting is generated sample by sample in Rust (click transient, pitch-dropping
body, low noise bed, algorithmic stereo tail) and written to the audio device
directly. There is no audio asset, no decoder, no `Audio` element.

Two reasons. First, the webview is the wrong place for the one sound the product
depends on: the Linux build runs WebKitGTK, which has long-standing trouble
loading media from custom protocols, and both hosts apply autoplay policies and
background timer throttling that we would have to fight per platform. Second, a
synthesised Sting can vary per cue for free — the body shifts a few semitones
while the tail stays fixed — which is how one sound becomes many without a sample
library.

_Consequences_: the Sting is code, so it needs its own tuning loop and a renderer
to a WAV file to hear it without launching the app. If the synthesised body never
sounds good enough, the fallback is a single recorded impact with the procedural
tail kept, which this design already allows.
