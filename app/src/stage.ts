/**
 * Screenshot stage: draws a believable desktop behind the real app.
 *
 * The app is loaded in an iframe at its own URL, so the card and the settings
 * window in the published screenshots are the product's own markup and CSS.
 * Everything outside the iframe (wallpaper, neighbouring windows, taskbar,
 * window frame) exists only for the photograph and never ships.
 *
 * Query: ?scene=card | ?scene=settings
 * Input: window.__SHOT = { wallpaper, text } set by scripts/screenshots.mjs
 */

import iconUrl from "../src-tauri/icons/32x32.png";

type Scene = "card" | "settings";

declare global {
  interface Window {
    __SHOT?: { wallpaper?: string; text?: string };
    __STAGE_READY?: boolean;
  }
}

const stage = document.getElementById("stage")!;
const shot = window.__SHOT ?? {};
const query = new URLSearchParams(window.location.search);
const scene: Scene = query.get("scene") === "settings" ? "settings" : "card";
const lines = (shot.text ?? "GO DRINK|WATER")
  .split("|")
  .map((line) => line.trim())
  .filter(Boolean);

stage.classList.add(scene === "settings" ? "scene-settings" : "scene-card");

/* ------------------------------- wallpaper -------------------------------- */

const wallpaper = document.createElement("div");
wallpaper.className = "wallpaper";
if (shot.wallpaper) wallpaper.style.backgroundImage = `url("${shot.wallpaper}")`;
stage.append(wallpaper);

/* --------------------------------- helpers -------------------------------- */

function html(markup: string): HTMLElement {
  const template = document.createElement("template");
  template.innerHTML = markup.trim();
  return template.content.firstElementChild as HTMLElement;
}

const icons = {
  start: `<svg viewBox="0 0 16 16" fill="currentColor" aria-hidden="true">
    <rect x="1" y="1" width="6.2" height="6.2" rx="0.6"/>
    <rect x="8.8" y="1" width="6.2" height="6.2" rx="0.6"/>
    <rect x="1" y="8.8" width="6.2" height="6.2" rx="0.6"/>
    <rect x="8.8" y="8.8" width="6.2" height="6.2" rx="0.6"/>
  </svg>`,
  search: `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.4" aria-hidden="true">
    <circle cx="7" cy="7" r="4.6"/><path d="m10.6 10.6 3.6 3.6" stroke-linecap="round"/>
  </svg>`,
  files: `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" aria-hidden="true">
    <path d="M1.5 4.2h4.4l1.5 1.7h7.1v7.4H1.5z" stroke-linejoin="round"/>
  </svg>`,
  terminal: `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" aria-hidden="true">
    <rect x="1.4" y="2.2" width="13.2" height="11.6" rx="1.6"/>
    <path d="m4.4 6.2 2.6 2-2.6 2M8.8 10.2h3" stroke-linecap="round" stroke-linejoin="round"/>
  </svg>`,
  browser: `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" aria-hidden="true">
    <circle cx="8" cy="8" r="6.3"/><path d="M2.2 6.2h11.6M2.4 9.9h11.2"/>
    <path d="M8 1.7c1.7 1.7 2.5 3.8 2.5 6.3S9.7 12.6 8 14.3C6.3 12.6 5.5 10.5 5.5 8S6.3 3.4 8 1.7z"/>
  </svg>`,
  settings: `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" aria-hidden="true">
    <circle cx="8" cy="8" r="2.4"/>
    <path d="M8 1.2v1.9M8 12.9v1.9M1.2 8h1.9M12.9 8h1.9M3.2 3.2l1.3 1.3M11.5 11.5l1.3 1.3M12.8 3.2l-1.3 1.3M4.5 11.5l-1.3 1.3"/>
  </svg>`,
  wifi: `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" aria-hidden="true">
    <path d="M1.8 6.2a9.5 9.5 0 0 1 12.4 0M4.1 8.8a6 6 0 0 1 7.8 0M6.4 11.3a2.6 2.6 0 0 1 3.2 0" stroke-linecap="round"/>
    <circle cx="8" cy="13.4" r="0.7" fill="currentColor" stroke="none"/>
  </svg>`,
  volume: `<svg viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.3" aria-hidden="true">
    <path d="M2 6h2.5L8 3.2v9.6L4.5 10H2z" stroke-linejoin="round"/>
    <path d="M10.6 5.6a3.4 3.4 0 0 1 0 4.8M12.6 3.8a6 6 0 0 1 0 8.4" stroke-linecap="round"/>
  </svg>`,
};

function taskbar(): HTMLElement {
  const now = new Date();
  const time = now.toLocaleTimeString("en-GB", { hour: "2-digit", minute: "2-digit" });
  const date = now.toLocaleDateString("en-GB", { day: "2-digit", month: "2-digit", year: "numeric" });

  return html(`
    <div class="taskbar">
      <div class="tb-apps">
        <span class="tb-start">${icons.start}</span>
        <span class="tb-app">${icons.search}</span>
        <span class="tb-sep"></span>
        <span class="tb-app">${icons.files}</span>
        <span class="tb-app">${icons.terminal}</span>
        <span class="tb-app">${icons.browser}</span>
        <span class="tb-app">${icons.settings}</span>
      </div>
      <div class="tb-tray">
        <span class="glyphs">${icons.wifi}${icons.volume}</span>
        <span>${time}<br>${date}</span>
      </div>
    </div>
  `);
}

/* ------------------------------- card scene ------------------------------- */

function cardScene(): HTMLElement {
  return html(`
    <div class="desktop">
      <div class="win editor" style="left:5.4vw;top:8.5vh;width:47vw;height:56vh">
        <div class="titlebar">
          <span class="mark"></span>
          <span class="title">scheduler.rs — threshold</span>
          <span class="controls"><i class="min"></i><i class="max"></i><i class="close"></i></span>
        </div>
        <div class="tabs">
          <span class="tab on">scheduler.rs</span>
          <span class="tab">cue.rs</span>
          <span class="tab">wrap.rs</span>
        </div>
        <pre class="code"><span class="l"><span class="com">// a cue fires when the plan says so, not when the clock ticks</span></span><span class="l"></span><span class="l"><span class="kw">use</span> std::time::{Duration, Instant};</span><span class="l"><span class="kw">use</span> crate::cue::{Cue, Late};</span><span class="l"></span><span class="l"><span class="com">/// The next time this cue is allowed to ring.</span></span><span class="l"><span class="kw">pub fn</span> <span class="fn">due</span>(cue: <span class="ty">&amp;Cue</span>, now: Instant) -&gt; <span class="ty">Option&lt;Instant&gt;</span> {</span><span class="l">    <span class="kw">let</span> next = cue.schedule.next_after(now)?;</span><span class="l"></span><span class="l">    <span class="kw">match</span> cue.late {</span><span class="l">        <span class="ty">Late::Exact</span> =&gt; {</span><span class="l">            <span class="kw">let</span> early = next.saturating_duration_since(now);</span><span class="l">            (early &lt;= Duration::from_secs(<span class="num">2</span>)).then_some(next)</span><span class="l">        }</span><span class="l">        <span class="ty">Late::CatchUp</span>(grace) =&gt; {</span><span class="l">            <span class="kw">let</span> late = now.saturating_duration_since(next);</span><span class="l">            (late &lt;= grace).then_some(next)</span><span class="l">        }</span><span class="l">        <span class="ty">Late::NextOpportunity</span> =&gt; <span class="ty">Some</span>(next),</span><span class="l">    }</span><span class="l">}</span><span class="l"></span><span class="l"><span class="com">// never replay what the operator slept through</span></span></pre>
        <div class="status"><span>main*</span><span>Rust</span><span>Ln 12, Col 9</span><span class="push">UTF-8</span></div>
      </div>
    </div>
  `);
}

/* ----------------------------- settings scene ----------------------------- */

function settingsScene(): HTMLElement {
  return html(`
    <div class="settings-frame">
      <div class="titlebar">
        <img class="app-icon" src="${iconUrl}" alt="" />
        <span class="title">Threshold</span>
        <span class="controls"><i class="min"></i><i class="max"></i><i class="close"></i></span>
      </div>
      <iframe src="/?window=settings&amp;demo" title="Threshold settings"></iframe>
    </div>
  `);
}

/* -------------------------------- assemble -------------------------------- */

let frame: HTMLIFrameElement | null = null;

if (scene === "card") {
  stage.append(cardScene());
  stage.append(taskbar());

  const card = document.createElement("iframe");
  card.className = "layer";
  card.title = "Threshold card";
  const params = new URLSearchParams({ window: "overlay", demo: "", bare: "" });
  if (lines.length) params.set("text", lines.join("|"));
  card.src = `/?${params.toString()}`;
  frame = card;
  stage.append(card);
} else {
  stage.append(settingsScene());
  stage.append(taskbar());
  frame = stage.querySelector(".settings-frame iframe");
}

/* --------------------------------- ready ---------------------------------- */

function settle() {
  requestAnimationFrame(() =>
    requestAnimationFrame(() => window.setTimeout(() => (window.__STAGE_READY = true), 120))
  );
}

async function waitForApp(target: HTMLIFrameElement) {
  const doc = target.contentDocument;
  if (!doc) return settle();

  const selector = scene === "card" ? ".card .line" : ".cues li";
  const deadline = Date.now() + 10_000;

  while (Date.now() < deadline) {
    if (doc.querySelector(selector)) {
      await doc.fonts.ready;
      settle();
      return;
    }
    await new Promise((resolve) => window.setTimeout(resolve, 60));
  }

  // Nothing rendered: still take the shot so the failure is visible.
  console.warn("screenshot stage: app did not render", selector);
  settle();
}

if (frame) {
  if (frame.contentDocument?.readyState === "complete" && frame.contentDocument.querySelector("*")) {
    void waitForApp(frame);
  } else {
    frame.addEventListener("load", () => void waitForApp(frame), { once: true });
  }
} else {
  settle();
}
