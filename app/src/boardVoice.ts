/**
 * The Board's voice. One source path: the oscillator is gated for consonants,
 * the noise burst excites the same formants, then the vowel opens. No TTS, no
 * samples, no second bus.
 *
 * The screen text is the translation. The voice speaks a deterministic
 * syllabary; the same line always produces the same syllables.
 */

const HARD = ["k", "t", "g", "q", "kh"];
const SOFT = ["s", "sh", "m", "n", "r", "l", "d", "b"];
const VOWELS = ["a", "e", "i", "o", "u", "ə"];

const FORMANTS: Record<string, [number, number, number]> = {
  a: [730, 1090, 2440],
  e: [530, 1840, 2480],
  i: [270, 2290, 3010],
  o: [570, 840, 2410],
  u: [300, 870, 2240],
  "ə": [500, 1500, 2500],
};

const CONS: Record<
  string,
  {
    freq: number;
    gain: number;
    len: number;
    stop?: boolean;
    fric?: boolean;
    nasal?: boolean;
    liquid?: boolean;
    locus: [number, number, number];
  }
> = {
  k: { freq: 2600, gain: 0.55, len: 0.024, stop: true, locus: [300, 1800, 2600] },
  t: { freq: 3600, gain: 0.55, len: 0.022, stop: true, locus: [300, 1800, 3000] },
  g: { freq: 900, gain: 0.4, len: 0.03, stop: true, locus: [300, 1200, 2400] },
  q: { freq: 850, gain: 0.5, len: 0.035, stop: true, locus: [400, 900, 2000] },
  kh: { freq: 1300, gain: 0.3, len: 0.07, fric: true, locus: [500, 1000, 2200] },
  s: { freq: 5200, gain: 0.24, len: 0.07, fric: true, locus: [300, 1800, 2600] },
  sh: { freq: 3000, gain: 0.26, len: 0.08, fric: true, locus: [300, 1600, 2400] },
  m: { freq: 300, gain: 0, len: 0.04, nasal: true, locus: [300, 1000, 2200] },
  n: { freq: 300, gain: 0, len: 0.04, nasal: true, locus: [300, 1500, 2400] },
  r: { freq: 500, gain: 0, len: 0.04, liquid: true, locus: [400, 1200, 2200] },
  l: { freq: 400, gain: 0, len: 0.04, liquid: true, locus: [400, 1000, 2200] },
  d: { freq: 3000, gain: 0.4, len: 0.026, stop: true, locus: [300, 1600, 2600] },
  b: { freq: 800, gain: 0.4, len: 0.03, stop: true, locus: [300, 900, 2200] },
};

const DEFAULT_CONS = CONS.k;

export type BoardVoiceHandle = {
  duration: number;
  stop: () => void;
};

type Word = { text: string; at: number; dur: number; last: boolean };
type Break = { at: number; dur: number };
type Script = { words: Word[]; breaks: Break[]; total: number };

type Scheduled = {
  duration: number;
  master: GainNode;
  sources: Array<OscillatorNode | AudioBufferSourceNode>;
};

function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619);
  return h >>> 0;
}

function noise01(s: string): number {
  return (hash(`j${s}`) % 1000) / 1000;
}

function syllabify(word: string): string[] {
  const w = word.replace(/[^a-zA-Z]/g, "").toLowerCase();
  if (!w) return [];
  const n = Math.max(1, Math.min(3, Math.round(w.length / 2.2)));
  const h = hash(w);
  const out: string[] = [];
  for (let i = 0; i < n; i++) {
    const useHard = i === 0 && (h >>> 3) % 3 !== 0;
    const pool = useHard ? HARD : [...HARD, ...SOFT];
    const c = pool[(h >>> (i * 5)) % pool.length];
    const v = VOWELS[(h >>> (i * 3 + 7)) % VOWELS.length];
    out.push(c + v);
  }
  return out;
}

function onsetOf(s: string): string {
  return s.startsWith("kh") || s.startsWith("sh") ? s.slice(0, 2) : s.slice(0, 1);
}

function vowelOf(s: string): string {
  return s.slice(onsetOf(s).length, onsetOf(s).length + 1);
}

/** Deliberate tempo and seeded duration jitter, matching the visual test. */
function scriptFor(text: string): Script {
  const speed = 0.55;
  const jitter = 0.08;
  const scale = 1 / speed;
  const clean = text.replace(/[<>]/g, " ").trim();
  const words: Word[] = [];
  const breaks: Break[] = [];
  let t = 0;
  const parts = clean.split(/(\/)/);
  for (const part of parts) {
    const p = part.trim();
    if (!p) continue;
    if (p === "/") {
      const pause = 0.18 * scale * (1 + (noise01(`s${t}`) - 0.5) * 2 * jitter * 2);
      breaks.push({ at: t, dur: pause });
      t += pause;
    } else {
      const list = p.split(/\s+/).filter(Boolean);
      for (const word of list) {
        const dur =
          Math.min(1.4, (0.16 + word.length * 0.05) * 1.15 * scale) *
          (1 + (noise01(`w${word}`) - 0.5) * 2 * jitter);
        words.push({ text: word, at: t, dur, last: false });
        t += dur;
      }
    }
  }
  if (words.length) words[words.length - 1].last = true;
  const tail = 0.9;
  breaks.push({ at: t, dur: Math.min(0.6, 0.3 * scale) });
  return { words, breaks, total: t + tail + 0.3 * scale };
}

function scheduleBoardVoice(
  ctx: BaseAudioContext,
  text: string,
  destination: AudioNode
): Scheduled {
  const script = scriptFor(text);
  const now = ctx.currentTime + 0.08;
  const endTime = now + script.total;

  const master = ctx.createGain();
  master.gain.setValueAtTime(0, now);
  master.gain.linearRampToValueAtTime(0.82, now + 0.08);

  const comp = ctx.createDynamicsCompressor();
  comp.threshold.value = -20;
  comp.knee.value = 12;
  comp.ratio.value = 4;
  comp.attack.value = 0.003;
  comp.release.value = 0.25;
  master.connect(comp);
  comp.connect(destination);

  const oscMix = ctx.createGain();
  oscMix.gain.value = 0.35;
  const osc1 = ctx.createOscillator();
  osc1.type = "sawtooth";
  osc1.frequency.setValueAtTime(48, now);
  const osc2 = ctx.createOscillator();
  osc2.type = "square";
  osc2.frequency.setValueAtTime(51, now);
  osc2.detune.value = -7;
  osc1.connect(oscMix);
  osc2.connect(oscMix);

  const oscGate = ctx.createGain();
  oscGate.gain.value = 1;
  oscMix.connect(oscGate);

  const noise = ctx.createBufferSource();
  const noiseBuffer = ctx.createBuffer(1, Math.floor(ctx.sampleRate * 2), ctx.sampleRate);
  const nd = noiseBuffer.getChannelData(0);
  for (let i = 0; i < nd.length; i++) nd[i] = Math.random() * 2 - 1;
  noise.buffer = noiseBuffer;
  noise.loop = true;
  const nBand = ctx.createBiquadFilter();
  nBand.type = "bandpass";
  nBand.frequency.value = 2100;
  nBand.Q.value = 0.8;
  const nGain = ctx.createGain();
  nGain.gain.value = 0;
  noise.connect(nBand);
  nBand.connect(nGain);

  const input = ctx.createGain();
  input.gain.value = 1;
  oscGate.connect(input);
  nGain.connect(input);

  const f1 = ctx.createBiquadFilter();
  const f2 = ctx.createBiquadFilter();
  const f3 = ctx.createBiquadFilter();
  for (const [f, q] of [
    [f1, 9],
    [f2, 12],
    [f3, 15],
  ] as const) {
    f.type = "bandpass";
    f.frequency.value = 600;
    f.Q.value = q;
  }
  const fg1 = ctx.createGain(); fg1.gain.value = 0.95;
  const fg2 = ctx.createGain(); fg2.gain.value = 0.7;
  const fg3 = ctx.createGain(); fg3.gain.value = 0.45;
  const fsum = ctx.createGain(); fsum.gain.value = 0.9;
  input.connect(f1); f1.connect(fg1); fg1.connect(fsum);
  input.connect(f2); f2.connect(fg2); fg2.connect(fsum);
  input.connect(f3); f3.connect(fg3); fg3.connect(fsum);

  const shaper = ctx.createWaveShaper();
  const curve = new Float32Array(1024);
  for (let i = 0; i < curve.length; i++) {
    const x = (i * 2) / (curve.length - 1) - 1;
    curve[i] = Math.tanh(x * 2.2) / Math.tanh(2.2);
  }
  shaper.curve = curve;
  shaper.oversample = "none";

  const hp = ctx.createBiquadFilter();
  hp.type = "highpass";
  hp.frequency.value = 280;
  const lp = ctx.createBiquadFilter();
  lp.type = "lowpass";
  lp.frequency.value = 3800;
  const lowShelf = ctx.createBiquadFilter();
  lowShelf.type = "lowshelf";
  lowShelf.frequency.value = 90;
  lowShelf.gain.value = 3;
  const air = ctx.createBiquadFilter();
  air.type = "highshelf";
  air.frequency.value = 9000;
  air.gain.value = 3;

  fsum.connect(shaper);
  shaper.connect(hp);
  hp.connect(lp);
  lp.connect(lowShelf);
  lowShelf.connect(air);
  air.connect(master);

  const verb = ctx.createConvolver();
  const impulse = ctx.createBuffer(2, Math.floor(ctx.sampleRate * 1.6), ctx.sampleRate);
  for (let c = 0; c < 2; c++) {
    const d = impulse.getChannelData(c);
    for (let i = 0; i < d.length; i++) {
      d[i] = (Math.random() * 2 - 1) * Math.pow(1 - i / d.length, 2.2);
    }
  }
  verb.buffer = impulse;
  const verbGain = ctx.createGain();
  verbGain.gain.value = 0.24;
  master.connect(verb);
  verb.connect(verbGain);
  verbGain.connect(comp);

  const setVowel = (v: [number, number, number], t: number) => {
    f1.frequency.setTargetAtTime(v[0] * 0.84, t, 0.02);
    f2.frequency.setTargetAtTime(v[1] * 0.84, t, 0.02);
    f3.frequency.setTargetAtTime(v[2] * 0.84, t, 0.02);
  };

  const setPitch = (target: number, t: number, dur: number, last: boolean) => {
    osc1.frequency.setTargetAtTime(target, t, 0.03);
    osc2.frequency.setTargetAtTime(target * 1.06, t, 0.03);
    if (last) {
      const fall = target * Math.pow(2, -1 / 12);
      osc1.frequency.linearRampToValueAtTime(fall, t + dur);
      osc2.frequency.linearRampToValueAtTime(fall * 1.06, t + dur);
    }
  };

  const sources: Array<OscillatorNode | AudioBufferSourceNode> = [osc1, osc2, noise];

  for (const word of script.words) {
    const syllables = syllabify(word.text);
    const count = syllables.length || 1;
    const weights = syllables.map((s) => 0.75 + (hash(s) % 7) * 0.08);
    const sum = weights.reduce((a, b) => a + b, 0) || count;
    let cursor = word.at;

    for (let i = 0; i < count; i++) {
      const syl = syllables[i] || "ka";
      const onset = onsetOf(syl);
      const vowelKey = vowelOf(syl);
      const info = CONS[onset] ?? DEFAULT_CONS;
      const sd = (word.dur * weights[i]) / sum;
      const start = now + 0.08 + cursor;
      const cDur = info.nasal || info.liquid
        ? Math.min(0.045, sd * 0.22)
        : Math.min(0.085, sd * 0.3);
      const vStart = start + cDur;
      const vDur = Math.max(0.07, sd - cDur);
      cursor += sd;

      nBand.frequency.setTargetAtTime(info.freq, start, 0.004);
      nGain.gain.setTargetAtTime(0, start, 0.002);
      if (info.stop || info.fric) {
        nGain.gain.setTargetAtTime(info.gain, start + 0.004, 0.003);
        nGain.gain.setTargetAtTime(0, start + Math.min(info.len, cDur), 0.012);
      }

      if (info.stop) {
        oscGate.gain.setTargetAtTime(0, start, 0.004);
        oscGate.gain.setTargetAtTime(1, vStart, 0.012);
      } else if (info.fric) {
        oscGate.gain.setTargetAtTime(0.08, start, 0.004);
        oscGate.gain.setTargetAtTime(1, vStart, 0.012);
      } else if (info.nasal || info.liquid) {
        oscGate.gain.setTargetAtTime(0.45, start, 0.01);
        oscGate.gain.setTargetAtTime(1, vStart, 0.02);
      } else {
        oscGate.gain.setTargetAtTime(1, start, 0.01);
      }

      f1.frequency.setTargetAtTime(info.locus[0], start, 0.008);
      f2.frequency.setTargetAtTime(info.locus[1], start, 0.008);
      f3.frequency.setTargetAtTime(info.locus[2], start, 0.008);

      const v = FORMANTS[vowelKey] ?? FORMANTS.a;
      setVowel(v, vStart);
      setPitch(48, vStart, vDur, word.last && i === count - 1);
    }
  }

  for (const pause of script.breaks) {
    const t = now + 0.08 + pause.at;
    master.gain.setTargetAtTime(0.22, t, 0.008);
    master.gain.setTargetAtTime(0.82, t + pause.dur * 0.55, 0.02);
    oscGate.gain.setTargetAtTime(0, t, 0.004);
    oscGate.gain.setTargetAtTime(1, t + pause.dur, 0.02);
  }

  master.gain.setTargetAtTime(0.82, endTime - 0.6, 0.02);
  master.gain.linearRampToValueAtTime(0, endTime);

  sources.forEach((source) => source.start(now));
  sources.forEach((source) => source.stop(endTime + 0.05));

  return { duration: endTime - now, master, sources };
}

export async function speakBoardLine(text: string): Promise<BoardVoiceHandle> {
  const ctx = new AudioContext();
  await ctx.resume();
  const scheduled = scheduleBoardVoice(ctx, text, ctx.destination);
  return {
    duration: scheduled.duration,
    stop: () => {
      const t = ctx.currentTime;
      scheduled.master.gain.cancelScheduledValues(t);
      scheduled.master.gain.setTargetAtTime(0, t, 0.02);
      for (const source of scheduled.sources) {
        try {
          source.stop(t + 0.1);
        } catch {
          // already stopped
        }
      }
      window.setTimeout(() => {
        void ctx.close();
      }, 220);
    },
  };
}

/** Offline render for the video / comparison WAV. */
export async function renderBoardVoiceWav(text: string): Promise<string> {
  const sampleRate = 48000;
  const length = Math.ceil(sampleRate * (scriptFor(text).total + 0.6));
  const ctx = new OfflineAudioContext(2, length, sampleRate);
  scheduleBoardVoice(ctx, text, ctx.destination);
  const buffer = await ctx.startRendering();
  return wavBase64(buffer);
}

function wavBase64(buffer: AudioBuffer): string {
  const channels = buffer.numberOfChannels;
  const sampleRate = buffer.sampleRate;
  const dataLength = buffer.length * channels * 2;
  const view = new DataView(new ArrayBuffer(44 + dataLength));
  const write = (offset: number, text: string) => {
    for (let i = 0; i < text.length; i++) view.setUint8(offset + i, text.charCodeAt(i));
  };
  write(0, "RIFF");
  view.setUint32(4, 36 + dataLength, true);
  write(8, "WAVE");
  write(12, "fmt ");
  view.setUint32(16, 16, true);
  view.setUint16(20, 1, true);
  view.setUint16(22, channels, true);
  view.setUint32(24, sampleRate, true);
  view.setUint32(28, sampleRate * channels * 2, true);
  view.setUint16(32, channels * 2, true);
  view.setUint16(34, 16, true);
  write(36, "data");
  view.setUint32(40, dataLength, true);

  const data = Array.from({ length: channels }, (_, c) => buffer.getChannelData(c));
  let offset = 44;
  for (let i = 0; i < buffer.length; i++) {
    for (let c = 0; c < channels; c++) {
      const sample = Math.max(-1, Math.min(1, data[c][i]));
      view.setInt16(offset, sample < 0 ? sample * 0x8000 : sample * 0x7fff, true);
      offset += 2;
    }
  }

  const bytes = new Uint8Array(view.buffer);
  let binary = "";
  for (let i = 0; i < bytes.length; i++) binary += String.fromCharCode(bytes[i]);
  return btoa(binary);
}
