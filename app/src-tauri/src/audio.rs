//! The Threshold sting.
//!
//! A thump: 2 ms noise click, a pitch-dropping sine body, a low noise bed under
//! it, then a stereo Schroeder tail. No samples, no decoder, no assets.
//!
//! The point of the spike is to tune this by ear and eye (the WAV) before any of
//! it moves into the app. Run with no arguments to write `sting.wav` next to the
//! binary and play it.

use std::f32::consts::TAU;
use std::io::Write;

// ---------------------------------------------------------------------------
// dsp primitives
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Rng(u32);

impl Rng {
    fn new(seed: u32) -> Self {
        Rng(seed | 1)
    }
    fn next(&mut self) -> f32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        (x as f32 / u32::MAX as f32) * 2.0 - 1.0
    }
}

/// One-pole low-pass, used for the noise bed and comb damping.
struct Lowpass {
    z: f32,
    a: f32,
}

impl Lowpass {
    fn new(freq: f32, sr: f32) -> Self {
        let a = (-TAU * freq / sr).exp();
        Lowpass { z: 0.0, a }
    }
    fn process(&mut self, x: f32) -> f32 {
        self.z = x * (1.0 - self.a) + self.z * self.a;
        self.z
    }
}

struct Comb {
    buf: Vec<f32>,
    i: usize,
    feedback: f32,
    damp: f32,
    store: f32,
}

impl Comb {
    fn new(delay: usize, feedback: f32, damp: f32) -> Self {
        Comb {
            buf: vec![0.0; delay],
            i: 0,
            feedback,
            damp,
            store: 0.0,
        }
    }
    fn process(&mut self, x: f32) -> f32 {
        let out = self.buf[self.i];
        self.store = out * (1.0 - self.damp) + self.store * self.damp;
        self.buf[self.i] = x + self.store * self.feedback;
        self.i = (self.i + 1) % self.buf.len();
        out
    }
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
    g: f32,
}

impl Allpass {
    fn new(delay: usize, g: f32) -> Self {
        Allpass {
            buf: vec![0.0; delay.max(1)],
            i: 0,
            g,
        }
    }
    fn process(&mut self, x: f32) -> f32 {
        let buffered = self.buf[self.i];
        let out = -x + buffered;
        self.buf[self.i] = x + buffered * self.g;
        self.i = (self.i + 1) % self.buf.len();
        out
    }
}

/// Freeverb-style stereo tail: eight damped combs into two allpasses per side.
/// Four combs ring metallically; eight with damping reads as a room.
struct Reverb {
    combs_l: Vec<Comb>,
    combs_r: Vec<Comb>,
    ap_l: Vec<Allpass>,
    ap_r: Vec<Allpass>,
}

impl Reverb {
    fn new(t60: f32, sr: f32) -> Self {
        const DELAYS: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
        const APS: [usize; 2] = [225, 556];
        let scale = sr / 44100.0;
        let mk = |offset: usize| -> (Vec<Comb>, Vec<Allpass>) {
            let combs = DELAYS
                .iter()
                .map(|&d| {
                    let d = ((d + offset) as f32 * scale) as usize;
                    let feedback = 10f32.powf(-3.0 * d as f32 / (t60 * sr));
                    Comb::new(d.max(1), feedback.min(0.985), 0.42)
                })
                .collect();
            let aps = APS
                .iter()
                .map(|&d| Allpass::new(((d + offset) as f32 * scale) as usize, 0.5))
                .collect();
            (combs, aps)
        };
        let (combs_l, ap_l) = mk(0);
        let (combs_r, ap_r) = mk(23);
        Reverb {
            combs_l,
            combs_r,
            ap_l,
            ap_r,
        }
    }

    fn process(&mut self, x: f32) -> (f32, f32) {
        let l: f32 = self.combs_l.iter_mut().map(|c| c.process(x)).sum::<f32>() * 0.125;
        let r: f32 = self.combs_r.iter_mut().map(|c| c.process(x)).sum::<f32>() * 0.125;
        let l = self.ap_l.iter_mut().fold(l, |acc, a| a.process(acc));
        let r = self.ap_r.iter_mut().fold(r, |acc, a| a.process(acc));
        (l, r)
    }
}

// ---------------------------------------------------------------------------
// the sting
// ---------------------------------------------------------------------------

pub struct Params {
    pub sr: f32,
    /// semitone offset on the body only; the tail never changes character
    pub semitones: f32,
    /// reverb decay time in seconds
    pub t60: f32,
    /// seconds of tail kept after the dry body
    pub tail: f32,
    /// weight below the body: 1 = default, 0 = thin, 2 = felt in the chest
    pub depth: f32,
    /// hit voicing: 0 hard, 1 knock-led, 2 sweep
    pub hit: u8,
}

impl Default for Params {
    fn default() -> Self {
        Params {
            sr: 48_000.0,
            semitones: 0.0,
            t60: 2.6,
            tail: 3.2,
            depth: 1.8,
            hit: 2,
        }
    }
}

#[allow(dead_code)] // peak is kept for offline level checks
pub struct Sting {
    pub stereo: Vec<(f32, f32)>,
    pub peak: f32,
}

#[allow(clippy::needless_range_loop)] // the index is the sample clock, not a slice cursor
pub fn render(p: &Params) -> Sting {
    // The tuner and the config are allowed to hand this bad numbers; audio must
    // never emit NaN, which can reach the device as full-scale noise.
    let sr = if p.sr.is_finite() && (8_000.0..=192_000.0).contains(&p.sr) {
        p.sr
    } else {
        48_000.0
    };
    let semitones = if p.semitones.is_finite() {
        p.semitones.clamp(-24.0, 24.0)
    } else {
        0.0
    };
    let depth = if p.depth.is_finite() {
        p.depth.clamp(0.0, 4.0)
    } else {
        1.8
    };
    let t60 = if p.t60.is_finite() {
        p.t60.clamp(0.1, 20.0)
    } else {
        2.6
    };
    let tail = if p.tail.is_finite() {
        p.tail.clamp(0.1, 20.0)
    } else {
        3.2
    };
    let body_len = (0.9 * sr) as usize;
    let total = body_len + (tail * sr) as usize;
    let mut dry = vec![0f32; total];

    let bend = 2f32.powf(semitones / 12.0);
    let mut rng = Rng::new(0x5EED_1234);

    // A thump is mostly noise; a bell is a few partials. The two failed versions
    // were each one of those: sustained noise through a resonator (rattle) and
    // clean partials with a long decay (bell). This one keeps the noise but
    // confines it to the first few milliseconds, with no resonance to ring in.

    // --- crack: broadband noise, ~6 ms, no resonator --------------------------
    {
        let mut lp = Lowpass::new(1500.0, sr);
        for i in 0..(0.02 * sr) as usize {
            let t = i as f32 / sr;
            let n = rng.next();
            let crack = n - lp.process(n); // high-passed remainder: bright, uncoloured
            dry[i] += crack * (-t / 0.0055).exp() * 0.55;
        }
    }

    // --- body dirt: the same noise low-passed, 30 ms. Reads as mass, not hiss --
    {
        let mut lp = Lowpass::new(320.0, sr);
        for i in 0..(0.12 * sr) as usize {
            let t = i as f32 / sr;
            let n = rng.next();
            dry[i] += lp.process(n) * (-t / 0.022).exp() * 1.15;
        }
    }

    // --- thump: a fast downward sweep, driven so it has harmonics. Three
    // voicings, because "it lacks thump" is easier to answer by ear than by
    // spectrum, and the difference is where the sweep starts and how long it
    // takes to fall.
    let (f_hi, f_lo, sweep_tau, body_tau, knock) = match p.hit {
        // knock-led: the sub is there, but the ear hears the 165 Hz box
        1 => (95.0, 48.0, 0.035, 0.075, (1.85, 0.70, 0.070, 0.034)),
        // sweep: starts high and falls, the classic "thump" contour
        2 => (190.0, 52.0, 0.070, 0.095, (0.75, 0.30, 0.050, 0.026)),
        // hard: same pitch, but the body stops in 45 ms
        _ => (95.0, 48.0, 0.030, 0.045, (1.45, 0.55, 0.045, 0.024)),
    };
    {
        let f_hi = f_hi * bend;
        let f_lo = f_lo * bend;
        let mut phase = 0f32;
        for i in 0..body_len {
            let t = i as f32 / sr;
            let f = f_lo + (f_hi - f_lo) * (-t / sweep_tau).exp();
            phase += TAU * f / sr;
            if phase > TAU {
                phase -= TAU;
            }
            let env = (t / 0.0015).min(1.0) * (-t / body_tau).exp();
            dry[i] += (phase.sin() * 2.4).tanh() * env * 0.9;
        }
    }

    // --- knock: damped partials around 165 / 247 Hz. This is the "thump" the
    // ear hears on a laptop, where nothing below 60 Hz survives. Short enough
    // that they hit and stop rather than ring.
    for (freq, amp, tau) in [(165.0f32, knock.0, knock.2), (247.0, knock.1, knock.3)] {
        let mut phase = 0f32;
        let n = (0.35 * sr) as usize;
        for i in 0..n {
            let t = i as f32 / sr;
            phase += TAU * freq * bend / sr;
            if phase > TAU {
                phase -= TAU;
            }
            let env = (t / 0.0012).min(1.0) * (-t / tau).exp();
            dry[i] += (phase.sin() * 1.6).tanh() * env * amp / 1.6f32.tanh();
        }
    }

    // --- chest: 46 Hz, slower, undriven so it stays felt rather than heard ----
    {
        let mut phase = 0f32;
        for i in 0..body_len {
            let t = i as f32 / sr;
            phase += TAU * 46.0 * bend / sr;
            if phase > TAU {
                phase -= TAU;
            }
            let env = (t / 0.010).min(1.0) * (-t / (0.28 + 0.16 * depth)).exp();
            dry[i] += phase.sin() * env * 0.5 * depth;
        }
    }

    // --- room sub: the weight underneath everything, 31 Hz, slow. This is the
    // part that is felt in a room and lost on a laptop, which is why it is
    // layered under the body rather than replacing it ------------------------
    {
        let mut phase = 0f32;
        for i in 0..body_len {
            let t = i as f32 / sr;
            phase += TAU * 31.0 * bend / sr;
            if phase > TAU {
                phase -= TAU;
            }
            let env = (t / 0.060).min(1.0) * (-t / 0.62).exp();
            dry[i] += phase.sin() * env * 0.44 * depth;
        }
    }

    // --- trim, then one soft-clip stage. The layers sum hot; the tanh shapes
    // the peak instead of squaring everything off ------------------------------
    for s in dry.iter_mut() {
        *s = (*s * 0.55 * 1.3).tanh() / 1.3f32.tanh();
    }

    // --- tail: 30 ms behind the hit, dark, short. Long clean tails read as
    // pitched, which is the bell we just came back from ------------------------
    let pre = (0.030 * sr) as usize;
    // Two bands, because lows decay slower in a real room. Sending everything
    // through one reverb t60 makes the tail share the body's character; a longer
    // low band is what puts weight *behind* the hit instead of beside it.
    let low_verb_t60 = t60 * 1.7;
    let mut verb_low = Reverb::new(low_verb_t60, sr);
    let mut verb_high = Reverb::new(t60, sr);
    let in_cut_freq = (1500.0 - 350.0 * (depth - 1.0).max(0.0)).max(650.0);
    let mut in_cut = Lowpass::new(in_cut_freq, sr);
    let mut low_split = Lowpass::new(220.0, sr);
    let mut wet_cut_l = Lowpass::new(2400.0, sr);
    let mut wet_cut_r = Lowpass::new(2400.0, sr);
    let low_gain = (0.42 + 0.22 * (depth - 1.0).max(0.0)).min(0.9);
    let high_gain = 0.34;
    let mut stereo = Vec::with_capacity(total);
    let mut peak = 0f32;

    for i in 0..total {
        let sent = if i >= pre { dry[i - pre] } else { 0.0 };
        let low = low_split.process(sent);
        let high = in_cut.process(sent - low);

        let (ll, lr) = verb_low.process(low);
        let (hl, hr) = verb_high.process(high);
        let wl = wet_cut_l.process(ll * low_gain + hl * high_gain);
        let wr = wet_cut_r.process(lr * low_gain + hr * high_gain);

        let l = dry[i] * 0.95 + wl;
        let r = dry[i] * 0.95 + wr;
        peak = peak.max(l.abs()).max(r.abs());
        stereo.push((l, r));
    }

    // --- tail release. Reverb is still decaying when the buffer ends, so a hard
    // stop here is an audible click on the last sample. A raised-cosine fade over
    // the final quarter second hands the tail to silence instead of cutting it.
    let fade = (0.25 * sr) as usize;
    let start = total.saturating_sub(fade);
    for (i, (l, r)) in stereo.iter_mut().enumerate().skip(start) {
        let t = (i - start) as f32 / fade.max(1) as f32;
        let gain = 0.5 * (1.0 + (std::f32::consts::PI * t).cos());
        *l *= gain;
        *r *= gain;
    }

    // normalise to -1 dBFS: an impact should be loud, not clipped
    let target = 0.891;
    if peak > 0.0 {
        let g = target / peak;
        for (l, r) in stereo.iter_mut() {
            *l *= g;
            *r *= g;
        }
        peak = target;
    }

    Sting { stereo, peak }
}

// ---------------------------------------------------------------------------
// wav + playback
// ---------------------------------------------------------------------------

#[allow(dead_code)] // handy for offline renders, used by the spike
pub fn write_wav(path: &str, frames: &[(f32, f32)], sr: f32) -> std::io::Result<()> {
    let mut f = std::fs::File::create(path)?;
    let data_len = (frames.len() * 4) as u32;
    let byte_rate = sr as u32 * 4;

    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&2u16.to_le_bytes())?; // stereo
    f.write_all(&(sr as u32).to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&4u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for (l, r) in frames {
        let q = |x: f32| (x.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
        f.write_all(&q(*l).to_le_bytes())?;
        f.write_all(&q(*r).to_le_bytes())?;
    }
    Ok(())
}

pub fn play(frames: Vec<(f32, f32)>, sr: f32) -> Result<(), Box<dyn std::error::Error>> {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    let host = cpal::default_host();
    let device = host.default_output_device().ok_or("no output device")?;
    let config = device.default_output_config()?;
    let channels = config.channels() as usize;
    let out_rate = config.sample_rate() as f32;

    // resample by linear interpolation if the device disagrees with the render
    let src_len = frames.len();
    let src = if (out_rate - sr).abs() < 1.0 {
        frames
    } else {
        let ratio = sr / out_rate;
        let n = (frames.len() as f32 / ratio) as usize;
        (0..n)
            .map(|i| {
                let x = i as f32 * ratio;
                let a = frames[(x as usize).min(frames.len() - 1)];
                let b = frames[(x as usize + 1).min(frames.len() - 1)];
                let t = x.fract();
                (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)
            })
            .collect()
    };

    let mut i = 0usize;
    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device.build_output_stream(
            config.into(),
            move |data: &mut [f32], _| {
                for frame in data.chunks_mut(channels) {
                    let (l, r) = src.get(i).copied().unwrap_or((0.0, 0.0));
                    if channels >= 2 {
                        frame[0] = l;
                        frame[1] = r;
                    } else if channels == 1 {
                        frame[0] = (l + r) * 0.5;
                    }
                    i += 1;
                }
            },
            |e| eprintln!("stream error: {e}"),
            None,
        )?,
        cpal::SampleFormat::I16 => device.build_output_stream(
            config.into(),
            {
                let src = src.clone();
                let mut i = 0usize;
                move |data: &mut [i16], _| {
                    for frame in data.chunks_mut(channels) {
                        let (l, r) = src.get(i).copied().unwrap_or((0.0, 0.0));
                        if channels >= 2 {
                            frame[0] = (l * i16::MAX as f32) as i16;
                            frame[1] = (r * i16::MAX as f32) as i16;
                        } else if channels == 1 {
                            frame[0] = ((l + r) * 0.5 * i16::MAX as f32) as i16;
                        }
                        i += 1;
                    }
                }
            },
            |e| eprintln!("stream error: {e}"),
            None,
        )?,
        other => return Err(format!("unsupported sample format {other:?}").into()),
    };

    stream.play()?;
    let secs = src_len as f32 / sr;
    std::thread::sleep(std::time::Duration::from_secs_f32(secs + 0.05));
    Ok(())
}

impl Params {
    /// The voice settled on in the spike: the sweep hit over the deep tail.
    pub fn chosen() -> Self {
        Params {
            sr: 48_000.0,
            semitones: 0.0,
            t60: 2.6,
            tail: 3.2,
            depth: 1.8,
            hit: 2,
        }
    }

    pub fn at_semitone(mut self, semitones: f32) -> Self {
        self.semitones = semitones;
        self
    }
}

/// Render and play in the background, so the card is drawn on the same frame
/// the sound starts.
pub fn fire(semitones: f32, volume: f32) {
    let params = Params::chosen().at_semitone(semitones);
    let gain = if volume.is_finite() {
        volume.clamp(0.0, 1.0)
    } else {
        1.0
    };
    std::thread::spawn(move || {
        let sting = render(&params);
        let scaled: Vec<(f32, f32)> = sting
            .stereo
            .iter()
            .map(|(l, r)| (l * gain, r * gain))
            .collect();
        if let Err(e) = play(scaled, params.sr) {
            eprintln!("threshold: could not play the sting: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(frames: &[(f32, f32)]) -> f32 {
        frames
            .iter()
            .fold(0f32, |m, (l, r)| m.max(l.abs()).max(r.abs()))
    }

    fn rms(frames: &[(f32, f32)]) -> f32 {
        if frames.is_empty() {
            return 0.0;
        }
        let sum: f32 = frames.iter().map(|(l, r)| l * l + r * r).sum();
        (sum / (frames.len() * 2) as f32).sqrt()
    }

    #[test]
    fn the_tail_ends_in_silence_rather_than_on_a_cut() {
        let sting = render(&Params::chosen());
        let last_ms = &sting.stereo[sting.stereo.len() - 48..];
        assert!(
            peak(last_ms) < 1e-4,
            "the last millisecond is still at {:.4} — that is the click",
            peak(last_ms)
        );
    }

    #[test]
    fn non_finite_parameters_never_reach_the_device_as_nan() {
        let mut params = Params::chosen();
        params.semitones = f32::NAN;
        params.depth = f32::INFINITY;
        params.t60 = f32::NAN;
        params.tail = f32::NEG_INFINITY;
        let sting = render(&params);
        assert!(!sting.stereo.is_empty());
        assert!(
            sting
                .stereo
                .iter()
                .all(|(l, r)| l.is_finite() && r.is_finite()),
            "a bad parameter produced a non-finite sample"
        );
        assert!(sting.peak.is_finite() && sting.peak > 0.0);
    }

    #[test]
    fn the_fade_starts_from_a_decayed_tail() {
        let params = Params::chosen();
        let sting = render(&params);
        let n = sting.stereo.len();
        let quarter = (0.25 * params.sr) as usize;
        let tenth = (0.1 * params.sr) as usize;
        let just_before = rms(&sting.stereo[n - quarter - tenth..n - quarter]);
        let overall = peak(&sting.stereo);
        assert!(
            just_before < overall * 0.02,
            "the fade starts at {just_before:.4} against a peak of {overall:.4}: the release would be audible"
        );
    }
}
