import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";

type CueDraft = {
  text: string;
  at: string;
  repeat: string;
  late: string;
  seconds: number;
  sting: number;
  enabled: boolean;
};

type StateDto = {
  configPath: string;
  changedOnDisk: boolean;
  floorMs: number;
  volume: number;
  overlay: string;
  cues: CueDraft[];
  next: (string | null)[];
  trouble: string | null;
};

const REPEATS = ["", "daily", "weekdays", "weekly(mon,wed,fri)", "weekly(mon)"];
const LATES = ["", "catch_up(10m)", "catch_up(30m)", "next_opportunity"];

const blank = (): CueDraft => ({
  text: "NEW CUE",
  at: "18:00",
  repeat: "",
  late: "",
  seconds: 4,
  sting: 0,
  enabled: true,
});

/** "in 2d 4h", "in 25m" — the only number that matters on this screen. */
function until(iso: string | null | undefined): string {
  if (!iso) return "";
  const at = new Date(iso).getTime();
  if (Number.isNaN(at)) return "";
  const mins = Math.round((at - Date.now()) / 60000);
  if (mins <= 0) return "due";
  if (mins < 60) return `in ${mins}m`;
  if (mins < 60 * 24) return `in ${Math.floor(mins / 60)}h ${mins % 60}m`;
  return `in ${Math.floor(mins / (60 * 24))}d ${Math.floor((mins % (60 * 24)) / 60)}h`;
}

function clock(iso: string | null | undefined): string {
  if (!iso) return "";
  return new Date(iso).toLocaleString(undefined, {
    weekday: "short",
    hour: "2-digit",
    minute: "2-digit",
  });
}

const pad = (n: number) => String(n).padStart(3, "0");

export default function Settings() {
  const [state, setState] = useState<StateDto | null>(null);
  const [cues, setCues] = useState<CueDraft[]>([]);
  const [floor, setFloor] = useState(600);
  const [volume, setVolume] = useState(1);
  const [overlay, setOverlay] = useState("auto");
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);

  // viewable in a plain browser: /?window=settings&demo
  const demoMode =
    new URLSearchParams(window.location.search).has("demo") &&
    !("__TAURI_INTERNALS__" in window);

  // The window can exist before the Rust side has managed its state, so a
  // "starting" answer means "ask again", not "this is broken".
  const load = useCallback(async (attempt = 0): Promise<void> => {
    try {
      const s = await invoke<StateDto>("get_state");
      setState(s);
      setCues(s.cues);
      setFloor(s.floorMs);
      setVolume(s.volume);
      setOverlay(s.overlay);
      setError(null);
    } catch (e) {
      const message = String(e);
      if (message.includes("starting") && attempt < 25) {
        window.setTimeout(() => void load(attempt + 1), 120);
        return;
      }
      setError(message);
    }
  }, []);

  useEffect(() => {
    if (demoMode) {
      setState(DEMO);
      setCues(DEMO.cues);
      setFloor(DEMO.floorMs);
      setVolume(DEMO.volume);
      setOverlay(DEMO.overlay);
      return;
    }
    load();
  }, [load, demoMode]);

  const patch = (i: number, next: Partial<CueDraft>) =>
    setCues((all) => all.map((c, j) => (j === i ? { ...c, ...next } : c)));

  const save = async () => {
    if (demoMode) return;
    try {
      await invoke("save_state", { floorMs: floor, volume, overlay, cues });
      setSaved(true);
      setError(null);
      window.setTimeout(() => setSaved(false), 1400);
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const reload = async () => {
    try {
      await invoke("reload_config");
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const addExamples = async () => {
    if (demoMode) return;
    try {
      await invoke<number>("add_examples");
      await load();
    } catch (e) {
      setError(String(e));
    }
  };

  const preview = async (cue: CueDraft) => {
    if (demoMode) return;
    try {
      await invoke("preview_cue", { cue });
    } catch (e) {
      setError(String(e));
    }
  };

  // the next interruption, which is the one thing worth showing big
  const nextIndex = (state?.next ?? []).reduce<number>((best, iso, i) => {
    if (!iso) return best;
    if (best === -1) return i;
    const a = new Date(iso).getTime();
    const b = new Date(state!.next[best]!).getTime();
    return a < b ? i : best;
  }, -1);
  const nextCue = nextIndex >= 0 ? cues[nextIndex] : undefined;
  const armed = cues.filter((c) => c.enabled);

  return (
    <main className={`settings ${demoMode ? "demo" : ""}`}>
      <header className="masthead">
        <div className="brand">
          <h1>THRESHOLD</h1>
          <p className="kicker">CUE REGISTRY</p>
        </div>
        <div className="meta">
          <label className="floor">
            floor
            <input
              type="number"
              min={100}
              max={10000}
              step={50}
              value={floor}
              onChange={(e) => setFloor(Number(e.target.value))}
            />
            ms
          </label>
          <label className="vol">
            volume
            <input
              type="range"
              min={0}
              max={1}
              step={0.02}
              value={volume}
              onChange={(e) => setVolume(Number(e.target.value))}
            />
            {Math.round(volume * 100)}%
          </label>
          <label className="vol">
            overlay
            <select value={overlay} onChange={(e) => setOverlay(e.target.value)}>
              <option value="auto">auto</option>
              <option value="permanent">permanent</option>
              <option value="on_demand">on demand</option>
            </select>
          </label>
          <span>{cues.length} cues</span>
          <span className={state?.changedOnDisk ? "drift" : ""}>
            {state?.changedOnDisk ? "file changed" : "in sync"}
          </span>
        </div>
      </header>

      <section className={`next ${nextCue ? "" : "empty"}`}>
        <div className="kicker">
          <span className={`dot ${nextCue ? "live" : "warn"}`} />
          next
        </div>
        <div className="next-text">{nextCue ? nextCue.text : armed.length ? "NOTHING DUE" : "NOTHING ARMED"}</div>
        <div className="next-meta">
          {nextCue ? (
            <>
              <span>{until(state?.next[nextIndex])}</span>
              <span>{clock(state?.next[nextIndex])}</span>
              <span>{nextCue.repeat || "once"}</span>
              <button className="fire" onClick={() => preview(nextCue)}>
                fire now
              </button>
            </>
          ) : armed.length ? (
            <>
              <span className="warn">{armed.length} armed</span>
              <span>every one has already passed — a one-shot only fires on its day</span>
            </>
          ) : (
            <span>arm a cue below</span>
          )}
        </div>
      </section>

      {state?.trouble && <div className="banner error">{state.trouble}</div>}

      {state?.changedOnDisk && (
        <div className="banner">
          this file changed on disk
          <button onClick={reload}>reload from disk</button>
        </div>
      )}

      {error && <div className="banner error">{error}</div>}

      <ol className="cues">
        {cues.map((cue, i) => (
          <li key={i} className={cue.enabled ? "" : "off"}>
            <div className="line-1">
              <span className="index">{pad(i + 1)}</span>
              <input
                className="text"
                value={cue.text}
                maxLength={40}
                spellCheck={false}
                onChange={(e) => patch(i, { text: e.target.value })}
              />
              <input
                className="at"
                value={cue.at}
                placeholder="18:00"
                spellCheck={false}
                onChange={(e) => patch(i, { at: e.target.value })}
              />
              <span
                className={`when ${cue.enabled && !state?.next[i] ? "missed" : ""}`}
                title={
                  cue.enabled && !state?.next[i]
                    ? "this cue has already passed and has no repeat, so it will not fire again — give it a repeat to re-arm it"
                    : clock(state?.next[i])
                }
              >
                {cue.enabled && !state?.next[i] ? "missed" : until(state?.next[i])}
              </span>
              <button
                className={`arm ${cue.enabled ? "on" : ""}`}
                title={cue.enabled ? "armed — click to disarm" : "disarmed — click to arm"}
                aria-pressed={cue.enabled}
                onClick={() => patch(i, { enabled: !cue.enabled })}
              >
                {cue.enabled ? "armed" : "off"}
              </button>
              <button className="ghost" title="fire this cue now" onClick={() => preview(cue)}>
                ▶
              </button>
              <button
                className="ghost remove"
                title="remove"
                onClick={() => setCues((all) => all.filter((_, j) => j !== i))}
              >
                ×
              </button>
            </div>

            <div className="line-2">
              <span className="index" />
              <label>
                repeat
                <select value={cue.repeat} onChange={(e) => patch(i, { repeat: e.target.value })}>
                  {REPEATS.map((r) => (
                    <option key={r} value={r}>
                      {r || "once"}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                if late
                <select value={cue.late} onChange={(e) => patch(i, { late: e.target.value })}>
                  {LATES.map((l) => (
                    <option key={l} value={l}>
                      {l || "skip"}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                on screen
                <input
                  className="num"
                  type="number"
                  min={1}
                  max={120}
                  value={cue.seconds}
                  onChange={(e) => patch(i, { seconds: Number(e.target.value) })}
                />
                s
              </label>
              <label>
                pitch
                <input
                  className="num"
                  type="number"
                  min={-6}
                  max={6}
                  step={1}
                  value={cue.sting}
                  onChange={(e) => patch(i, { sting: Number(e.target.value) })}
                />
                st
              </label>
            </div>
          </li>
        ))}
      </ol>

      <footer>
        <button onClick={() => setCues((all) => [...all, blank()])}>add cue</button>
        <button onClick={addExamples} title="append the twelve starter cues that are not already here">
          examples
        </button>
        <button className="primary" onClick={save}>
          {saved ? "saved" : "save"}
        </button>
        <span className="path" title={state?.configPath}>
          {state?.configPath}
        </span>
      </footer>
    </main>
  );
}

const DEMO: StateDto = {
  configPath: "…\\threshold\\config\\cues.toml",
  changedOnDisk: false,
  floorMs: 600,
  volume: 0.8,
  overlay: "permanent",
  cues: [
    { text: "GO DRINK WATER", at: "every 50m", repeat: "", late: "", seconds: 4, sting: 0, enabled: true },
    { text: "REVIEW MAIL", at: "18:00", repeat: "weekdays", late: "catch_up(20m)", seconds: 6, sting: 0, enabled: true },
    { text: "THE LONG HALL", at: "22:30", repeat: "daily", late: "", seconds: 6, sting: -3, enabled: false },
    { text: "LLAMA A JORGE", at: "16:26", repeat: "", late: "", seconds: 4, sting: 0, enabled: true },
  ],
  next: [
    new Date(Date.now() + 12 * 60000).toISOString(),
    new Date(Date.now() + 3 * 3600000).toISOString(),
    null,
    null,
  ],
  trouble: null,
};
