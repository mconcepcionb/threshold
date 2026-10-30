#!/usr/bin/env node
/**
 * Regenerate the README screenshots from the app itself.
 *
 * The overlay and the settings window are loaded over Vite at their own demo
 * URLs; this script only stages the desktop around them (wallpaper, a couple of
 * neighbouring windows, the taskbar) and drives the camera.
 *
 *   cd app
 *   npm run screenshots
 *   npm run screenshots -- --wallpaper <file-or-url> --text "REVIEW|MAIL"
 *
 * Uses the Playwright Chromium if installed, otherwise a local Chrome or Edge.
 * `npx playwright install chromium` fetches the pinned browser.
 */

import { chromium } from "playwright";
import { createServer } from "vite";
import { existsSync } from "node:fs";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, extname, isAbsolute, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const appDir = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repoDir = resolve(appDir, "..");
const cacheDir = join(appDir, ".screenshots-cache");

const WALLPAPERS = {
  mountains: {
    url: "https://images.unsplash.com/photo-1519681393784-d120267933ba?w=2400&q=85",
    credit: "Unsplash, photo-1519681393784-d120267933ba",
  },
  nebula: {
    url: "https://images.unsplash.com/photo-1462331940025-496dfbfc7564?w=2400&q=85",
    credit: "Unsplash, photo-1462331940025-496dfbfc7564",
  },
  forest: {
    url: "https://images.unsplash.com/photo-1441974231531-c6227db76b6e?w=2400&q=85",
    credit: "Unsplash, photo-1441974231531-c6227db76b6e",
  },
};

const SHOTS = [
  { scene: "card", file: "card.jpg", width: 1920, height: 1080, quality: 90 },
  { scene: "settings", file: "settings.jpg", width: 1600, height: 1000, quality: 92 },
];

const argv = process.argv.slice(2);

if (argv.includes("--help") || argv.includes("-h")) {
  console.log(`Threshold screenshots

  npm run screenshots          # the defaults
  node scripts/screenshots.mjs [options]

  --wallpaper <name|file|url>  mountains (default), nebula, forest, or an image
  --text "LINE|LINE"           card text (default "GO DRINK|WATER")
  --out <dir>                  output directory (default docs/screenshots)
  --scene card|settings        render one scene only
  --channel <chrome|msedge>    browser channel instead of bundled Chromium

  Wallpapers downloaded from the network are cached in .screenshots-cache/,
  which is ignored by git. A built-in backdrop is used when nothing loads.`);
  process.exit(0);
}

function option(name, fallback) {
  const at = argv.indexOf(`--${name}`);
  return at >= 0 && argv[at + 1] ? argv[at + 1] : fallback;
}

const wallpaperArg = option("wallpaper", "mountains");
const text = option("text", "GO DRINK|WATER");
const outDir = resolve(repoDir, option("out", "docs/screenshots"));
const channel = option("channel", "");
const only = option("scene", "");

const MIME = { ".jpg": "image/jpeg", ".jpeg": "image/jpeg", ".png": "image/png", ".webp": "image/webp" };

function dataUrl(bytes, mime) {
  return `data:${mime};base64,${Buffer.from(bytes).toString("base64")}`;
}

function fallbackWallpaper() {
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1920" height="1080">
    <defs>
      <linearGradient id="g" x1="0" y1="0" x2="1" y2="1">
        <stop offset="0" stop-color="#14171c"/>
        <stop offset="0.55" stop-color="#1d2128"/>
        <stop offset="1" stop-color="#241c1c"/>
      </linearGradient>
      <radialGradient id="r" cx="28%" cy="18%" r="72%">
        <stop offset="0" stop-color="#3d4653"/>
        <stop offset="1" stop-color="#3d4653" stop-opacity="0"/>
      </radialGradient>
      <radialGradient id="w" cx="82%" cy="76%" r="60%">
        <stop offset="0" stop-color="#4a3232"/>
        <stop offset="1" stop-color="#4a3232" stop-opacity="0"/>
      </radialGradient>
    </defs>
    <rect width="1920" height="1080" fill="url(#g)"/>
    <rect width="1920" height="1080" fill="url(#r)"/>
    <rect width="1920" height="1080" fill="url(#w)"/>
  </svg>`;
  return dataUrl(Buffer.from(svg), "image/svg+xml");
}

async function loadWallpaper(source) {
  if (source.startsWith("data:")) return { dataUrl: source, credit: "inline image" };

  if (source.startsWith("http://") || source.startsWith("https://")) {
    return download(source, source);
  }

  const local = isAbsolute(source) ? source : resolve(process.cwd(), source);
  if (existsSync(local)) {
    const mime = MIME[extname(local).toLowerCase()] ?? "image/jpeg";
    return { dataUrl: dataUrl(await readFile(local), mime), credit: local };
  }

  const preset = WALLPAPERS[source] ?? WALLPAPERS.mountains;
  await mkdir(cacheDir, { recursive: true });
  const cached = join(cacheDir, `${source}.jpg`);
  if (existsSync(cached)) {
    return { dataUrl: dataUrl(await readFile(cached), "image/jpeg"), credit: preset.credit };
  }

  return download(preset.url, preset.credit, cached);
}

async function download(url, credit, cachePath) {
  try {
    const response = await fetch(url, { redirect: "follow" });
    if (!response.ok) throw new Error(`HTTP ${response.status}`);
    const bytes = Buffer.from(await response.arrayBuffer());
    if (cachePath) await writeFile(cachePath, bytes);
    return { dataUrl: dataUrl(bytes, "image/jpeg"), credit };
  } catch (error) {
    console.warn(`! wallpaper unavailable (${error.message}); using the built-in backdrop`);
    return { dataUrl: fallbackWallpaper(), credit: "built-in backdrop" };
  }
}

async function launchBrowser() {
  const attempts = channel
    ? [{ channel }]
    : [{}, { channel: "chrome" }, { channel: "msedge" }];

  let lastError;
  for (const options of attempts) {
    try {
      return await chromium.launch(options);
    } catch (error) {
      lastError = error;
    }
  }

  throw new Error(
    `No browser available. Run "npx playwright install chromium", ` +
      `or pass --channel chrome.\n${lastError?.message ?? ""}`
  );
}

async function main() {
  const wallpaper = await loadWallpaper(wallpaperArg);
  await mkdir(outDir, { recursive: true });
  const server = await createServer({
    root: appDir,
    logLevel: "error",
    server: { host: "127.0.0.1", port: 0, strictPort: false },
  });
  await server.listen();
  const { port } = server.httpServer.address();
  const base = `http://127.0.0.1:${port}`;

  const browser = await launchBrowser();

  try {
    for (const shot of SHOTS) {
      if (only && shot.scene !== only) continue;

      const page = await browser.newPage({
        viewport: { width: shot.width, height: shot.height },
        deviceScaleFactor: 1,
      });

      const problems = [];
      page.on("pageerror", (error) => problems.push(String(error)));
      page.on("console", (message) => {
        if (message.type() === "error") problems.push(message.text());
      });
      await page.route("**/favicon.ico", (route) => route.fulfill({ status: 204 }));

      await page.addInitScript((payload) => {
        window.__SHOT = payload;
      }, { wallpaper: wallpaper.dataUrl, text });

      await page.goto(`${base}/screenshot.html?scene=${shot.scene}`, { waitUntil: "load" });
      await page.waitForFunction(() => window.__STAGE_READY === true, undefined, { timeout: 30_000 });

      const target = join(outDir, shot.file);
      await page.screenshot({ path: target, type: "jpeg", quality: shot.quality });
      await page.close();

      console.log(`  ${shot.file}  ${shot.width}x${shot.height}`);
      for (const problem of problems) console.warn(`  ! ${problem}`);
    }
  } finally {
    await browser.close();
    await server.close();
  }

  console.log(`\nWrote ${outDir}\nWallpaper: ${wallpaper.credit}`);
}

await main();
