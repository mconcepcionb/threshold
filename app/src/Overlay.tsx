import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { invoke } from "@tauri-apps/api/core";

type CardPayload = {
  lines: string[];
  sizePx: number;
  seconds: number;
  floorMs: number;
};

/**
 * The Card. Hard cut in, Floor, Window, hard cut out — no transitions anywhere.
 *
 * Rust does the work: it decides the breaks and the size against the real font,
 * places this window on the monitor under the cursor, and holds input back for
 * the Floor. This component only draws, then gets out of the way.
 */
export default function Overlay() {
  const [card, setCard] = useState<CardPayload | null>(null);
  const [interactive, setInteractive] = useState(false);
  const [demo, setDemo] = useState(false);
  const timers = useRef<number[]>([]);
  const inputListener = useRef<(() => void) | null>(null);

  useEffect(() => {
    function clearTimers() {
      timers.current.forEach(window.clearTimeout);
      timers.current = [];
    }

    // Listeners are per-card. Leaving them attached would let a card dismissed
    // thirty seconds ago dismiss the one on screen before its Floor is over.
    function detachInput() {
      if (!inputListener.current) return;
      window.removeEventListener("keydown", inputListener.current);
      window.removeEventListener("mousedown", inputListener.current);
      inputListener.current = null;
    }

    function dismiss() {
      clearTimers();
      detachInput();
      setCard(null);
      setInteractive(false);
      // the window never hides: clearing the card is the whole dismissal
      invoke("card_end", { label: getCurrentWindow().label }).catch(() => {});
    }

    function show(payload: CardPayload) {
      clearTimers();
      detachInput();
      setCard(payload);
      setInteractive(false);

      const label = getCurrentWindow().label;
      const startedAt = window.performance.now();
      timers.current.push(
        window.setTimeout(() => {
          setInteractive(true);
          invoke("card_floor", { label }).catch(() => {});
        }, payload.floorMs)
      );
      timers.current.push(window.setTimeout(dismiss, payload.seconds * 1000));

      // any interaction after the Floor dismisses; before it, nothing does
      const onInput = () => {
        if (window.performance.now() - startedAt >= payload.floorMs) dismiss();
      };
      inputListener.current = onInput;
      window.addEventListener("keydown", onInput);
      window.addEventListener("mousedown", onInput);
    }

    const unlisten = listen<CardPayload>("card", (event) => show(event.payload));

    // viewable in a plain browser: /index.html?window=overlay&demo
    // (the layout normally comes from Rust, so this is a hardcoded card)
    const demoMode = new URLSearchParams(window.location.search).has("demo");
    if (demoMode && !("__TAURI_INTERNALS__" in window)) {
      setDemo(true);
      const sizePx = Math.min(280, Math.round(window.innerWidth * 0.1458));
      show({ lines: ["GO DRINK", "WATER"], sizePx, seconds: 40, floorMs: 600 });
    }

    return () => {
      clearTimers();
      detachInput();
      unlisten.then((f) => f());
    };
  }, []);

  if (!card) return null;

  return (
    <>
      {demo && (
        <div className="demo-desktop" aria-hidden="true">
          <div className="editor" />
          <div className="doc" />
        </div>
      )}
      <div className={`card ${interactive ? "interactive" : ""}`}>
        <div className="block">
          <div className="lines">
            {card.lines.map((line, i) => (
              <div className="line" key={i} style={{ fontSize: `${card.sizePx}px` }}>
                {line}
              </div>
            ))}
          </div>
        </div>
      </div>
    </>
  );
}
