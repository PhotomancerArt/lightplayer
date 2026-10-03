#!/usr/bin/env node
// Agent-indicator spike: frame strips + stills + motion clips of the
// `agent-spike-*` stories, at 2x device scale, in headless Chrome.
//
//   node spikes/agent-indicator/capture.mjs <out-dir> [control ...]
//
// Serves the story build (target/dx/lpa-studio-web/release/web/public) on an
// OS-assigned port, opens each spike story, and for every `[data-spike-cell]`
// crops the lit control at each frame time. Animations are paused and SEEKED
// (Web Animations API), so every frame is exact, not a race. The still is a
// reload under `prefers-reduced-motion: reduce`. Then it lays out one sheet
// per control (rows = looks, columns = frames + still) and one motion clip
// (all looks in sync, 15 fps) through ffmpeg.

import { spawn } from "node:child_process";
import { createReadStream } from "node:fs";
import { mkdir, mkdtemp, stat, writeFile, rm } from "node:fs/promises";
import { createServer } from "node:http";
import { tmpdir } from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, "../..");
const site = path.join(repo, "target/dx/lpa-studio-web/release/web/public");
const argv = process.argv.slice(2);
const cssAt = argv.indexOf("--css");
const overrideCss = cssAt >= 0 ? await (await import("node:fs/promises")).readFile(argv.splice(cssAt, 2)[1], "utf8") : "";
const cellsAt = argv.indexOf("--cells");
const cellMap = cellsAt >= 0 ? JSON.parse(await (await import("node:fs/promises")).readFile(argv.splice(cellsAt, 2)[1], "utf8")) : {};
const outDir = path.resolve(argv[0] ?? "spike-out");
const only = argv.slice(1);

const STRIP_TIMES = [0.12, 0.32, 0.9, 2.0, 3.1, 3.7];
const CLIP_FPS = 15;
const CLIP_SECS = 4.4;

// target: the lit element inside each cell; pad: CSS px around it.
const CONTROLS = [
  {
    key: "save",
    title: "Header Save",
    story: "studio/layout/site-chrome/agent-spike-save",
    target: '[data-offer-path="project/save"] > *',
    pad: { left: 150, right: 60, top: 18, bottom: 18 },
  },
  {
    key: "node-verb",
    title: "Node card verb (Remove)",
    story: "studio/node/node/agent-spike-remove",
    target: '[data-offer-path$="/remove"] > *',
    pad: { left: 220, right: 24, top: 16, bottom: 16 },
  },
  {
    key: "device-button",
    title: "Device card button (Remove)",
    story: "studio/home/home-gallery/devices-card-agent-spike",
    target: '[data-offer-path$="/remove-project"] > *',
    pad: { left: 40, right: 40, top: 40, bottom: 30 },
  },
  {
    key: "edited-node",
    title: "Edited node (assistant set hue)",
    story: "studio/node/shader-face/agent-spike-edited",
    target: ".ux-agent-mark > *",
    // The whole card: header to the knob row (crop height set below).
    pad: { left: 14, right: 14, top: 14, bottom: 14 },
    cardTo: true,
    // Strips and the clip crop to the knob row (the chip shows in the stills).
    strip: '[id^="ux-panel-control-"]',
    gridColumns: 4,
  },
].filter((c) => only.length === 0 || only.includes(c.key));

const chrome = process.env.CHROME_BIN ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";

// ---------------------------------------------------------------- server
const types = { ".html": "text/html", ".js": "text/javascript", ".wasm": "application/wasm", ".css": "text/css", ".png": "image/png", ".woff2": "font/woff2", ".svg": "image/svg+xml", ".json": "application/json" };
const server = createServer(async (req, res) => {
  const url = new URL(req.url, "http://x");
  let file = path.join(site, decodeURIComponent(url.pathname));
  if (url.pathname.startsWith("/__out/")) file = path.join(outDir, decodeURIComponent(url.pathname.slice(7)));
  try {
    const s = await stat(file);
    if (s.isDirectory()) file = path.join(file, "index.html");
    await stat(file);
  } catch {
    file = path.join(site, "index.html");
  }
  res.writeHead(200, { "content-type": types[path.extname(file)] ?? "application/octet-stream" });
  createReadStream(file).pipe(res);
});
await new Promise((r) => server.listen(0, "127.0.0.1", r));
const base = `http://127.0.0.1:${server.address().port}/`;

// ---------------------------------------------------------------- chrome
const profile = await mkdtemp(path.join(tmpdir(), "agent-spike-chrome-"));
const proc = spawn(chrome, ["--headless=new", "--disable-gpu", "--hide-scrollbars", "--no-first-run", "--remote-debugging-port=0", `--user-data-dir=${profile}`, "about:blank"], { stdio: ["ignore", "ignore", "pipe"] });
const wsUrl = await new Promise((resolve, reject) => {
  let buf = "";
  proc.stderr.on("data", (d) => {
    buf += d;
    const m = buf.match(/DevTools listening on (ws:\/\/\S+)/);
    if (m) resolve(m[1]);
  });
  proc.on("exit", () => reject(new Error("chrome exited")));
});
const ws = new WebSocket(wsUrl);
await new Promise((r) => ws.addEventListener("open", r));
let nextId = 1;
const pending = new Map();
ws.addEventListener("message", (e) => {
  const msg = JSON.parse(e.data);
  if (msg.id && pending.has(msg.id)) {
    const { resolve, reject } = pending.get(msg.id);
    pending.delete(msg.id);
    msg.error ? reject(new Error(JSON.stringify(msg.error))) : resolve(msg.result);
  }
});
const send = (method, params = {}, sessionId) =>
  new Promise((resolve, reject) => {
    const id = nextId++;
    pending.set(id, { resolve, reject });
    ws.send(JSON.stringify({ id, method, params, sessionId }));
  });
const { targetId } = await send("Target.createTarget", { url: "about:blank" });
const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
const cdp = (m, p) => send(m, p, sessionId);
await cdp("Page.enable");
await cdp("Runtime.enable");
if (overrideCss) {
  await cdp("Page.addScriptToEvaluateOnNewDocument", {
    source: `document.addEventListener("DOMContentLoaded", () => { const s = document.createElement("style"); s.id = "spike-override"; s.textContent = ${JSON.stringify(overrideCss)}; document.head.appendChild(s); });`,
  });
}
const evaluate = async (expression) => {
  const r = await cdp("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.exceptionDetails) throw new Error(JSON.stringify(r.exceptionDetails));
  return r.result.value;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function setViewport(width, height, scale) {
  await cdp("Emulation.setDeviceMetricsOverride", { width, height, deviceScaleFactor: scale, mobile: false });
}

async function openStory(story, reduced) {
  await cdp("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: reduced ? "reduce" : "no-preference" }] });
  await cdp("Page.navigate", { url: `${base}stories/${story}?story-png=1&viewport=lg` });
  for (let i = 0; i < 300; i++) {
    const ready = await evaluate(`(() => {
      const box = document.querySelector('[data-story-capture="1"]');
      if (!box || !box.querySelector('[data-spike-cell]')) return false;
      if (document.fonts && document.fonts.status !== 'loaded') return false;
      return true;
    })()`).catch(() => false);
    if (ready) break;
    await sleep(100);
    if (i === 299) {
      console.error("not ready:", await evaluate(`({ href: location.href, cap: !!document.querySelector('[data-story-capture]'), capId: document.querySelector('[data-story-capture]')?.getAttribute('data-story-id'), text: document.body.innerText.slice(0, 300) })`));
    }
  }
  if (cellMap[currentKey]) {
    await evaluate(`(() => {
      const map = ${JSON.stringify(cellMap[currentKey])};
      [...document.querySelectorAll('[data-spike-cell]')].forEach((cell, i) => {
        const m = map[i];
        if (!m) { cell.style.display = 'none'; return; }
        cell.setAttribute('data-spike-cell', m.label);
        cell.className = 'tw:grid tw:min-w-0 tw:content-start tw:gap-2 tw:p-4 ' + m.class;
      });
    })()`);
  }
  await evaluate(`(() => {
    const s = document.createElement("style");
    s.textContent = ${JSON.stringify("[data-spike-cell] > span:first-child { visibility: hidden; }\n")} + ${JSON.stringify(overrideCss)};
    document.body.appendChild(s);
  })()`);
  await sleep(400);
}

async function pauseAt(t) {
  await evaluate(`(async () => {
    for (const a of document.getAnimations()) { a.pause(); a.currentTime = ${t * 1000}; }
    await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
  })()`);
}

async function cellRects(control) {
  return evaluate(`(() => {
    const pad = ${JSON.stringify(control.pad)};
    return [...document.querySelectorAll('[data-spike-cell]')].filter((c) => c.style.display !== 'none').map((cell) => {
      const t = cell.querySelector(${JSON.stringify(control.target)});
      const r = t.getBoundingClientRect();
      let bottom = r.bottom;
      ${control.cardTo ? `
      const knobs = cell.querySelector('.ux-agent-slot') ?? cell.querySelector('[id^="ux-panel-control-"]');
      if (knobs) bottom = knobs.getBoundingClientRect().bottom + 6;` : ""}
      return {
        label: cell.getAttribute('data-spike-cell'),
        x: Math.max(0, r.left - pad.left) + scrollX,
        y: Math.max(0, r.top - pad.top) + scrollY,
        width: r.width + pad.left + pad.right,
        height: (bottom - r.top) + pad.top + pad.bottom,
      };
    });
  })()`);
}

async function stripRects(control) {
  return evaluate(`(() => [...document.querySelectorAll('[data-spike-cell]')].filter((c) => c.style.display !== 'none').map((cell) => {
    const els = [...cell.querySelectorAll(${JSON.stringify(control.strip)})].map((e) => e.getBoundingClientRect());
    const l = Math.min(...els.map((r) => r.left)), r = Math.max(...els.map((r) => r.right));
    const t = Math.min(...els.map((r) => r.top)), b = Math.max(...els.map((r) => r.bottom));
    return { x: l - 22 + scrollX, y: t - 18 + scrollY, width: r - l + 44, height: b - t + 36 };
  }))()`);
}

async function shoot(rect, file) {
  const { data } = await cdp("Page.captureScreenshot", { format: "png", clip: { ...rect, scale: 1 }, captureBeyondViewport: true });
  await writeFile(file, Buffer.from(data, "base64"));
}

const slug = (s) => s.replace(/[^a-z0-9]+/gi, "-").replace(/^-|-$/g, "").toLowerCase();
const manifest = {};

let currentKey = null;
for (const control of CONTROLS) {
  currentKey = control.key;
  const dir = path.join(outDir, control.key);
  await rm(dir, { recursive: true, force: true });
  await mkdir(dir, { recursive: true });
  await setViewport(1200, 1000, 2);
  // Motion.
  await openStory(control.story, false);
  if (process.env.SPIKE_EVAL) { console.log(JSON.stringify(await evaluate(process.env.SPIKE_EVAL), null, 1)); process.exit(0); }
  const rects = control.strip ? await stripRects(control) : await cellRects(control);
  const labels = (await cellRects(control)).map((r) => r.label);
  for (const [i, r] of rects.entries()) r.label = labels[i];
  const rows = rects.map((r) => ({ label: r.label, slug: slug(r.label), strip: [], clip: [] }));
  for (const t of STRIP_TIMES) {
    await pauseAt(t);
    for (const [i, r] of rects.entries()) {
      const f = `${rows[i].slug}-t${Math.round(t * 100)}.png`;
      await shoot(r, path.join(dir, f));
      rows[i].strip.push(f);
    }
  }
  const nClip = Math.round(CLIP_SECS * CLIP_FPS);
  for (let k = 0; k < nClip; k++) {
    await pauseAt(k / CLIP_FPS);
    for (const [i, r] of rects.entries()) {
      const f = `clip/${rows[i].slug}-${String(k).padStart(3, "0")}.png`;
      await mkdir(path.join(dir, "clip"), { recursive: true });
      await shoot(r, path.join(dir, f));
      rows[i].clip.push(f);
    }
  }
  // Still (reduced motion).
  await openStory(control.story, true);
  const stillRects = await cellRects(control);
  for (const [i, r] of stillRects.entries()) {
    const f = `${rows[i].slug}-still.png`;
    await shoot(r, path.join(dir, f));
    rows[i].still = f;
  }
  if (control.strip) {
    for (const [i, r] of (await stripRects(control)).entries()) {
      const f = `${rows[i].slug}-still-strip.png`;
      await shoot(r, path.join(dir, f));
      rows[i].stillStrip = f;
    }
  }
  manifest[control.key] = { title: control.title, rows, width: rects[0].width, height: rects[0].height, still: stillRects[0], gridColumns: control.gridColumns };
  console.log(`captured ${control.key}: ${rows.length} looks`);
}

// ---------------------------------------------------------------- sheets
// Images are 2x pixels; the sheet shows them at natural pixel size (CSS
// width = pixel width at scale 1), so the sheet itself IS the 2x zoom.
function sheetHtml(key, m, mode, frame) {
  const w = Math.round(m.width * 2);
  const h = Math.round(m.height * 2);
  const head = mode === "sheet"
    ? `<div class="row head"><div class="lab"></div>${STRIP_TIMES.map((t) => `<div class="cap" style="width:${w}px">t = ${t.toFixed(2)} s</div>`).join("")}<div class="cap" style="width:${w}px">still (reduced motion)</div></div>`
    : "";
  const body = m.rows.map((r) => {
    const imgs = mode === "sheet"
      ? [...r.strip, r.stillStrip ?? r.still].map((f) => `<img src="/__out/${key}/${f}" width="${w}" height="${h}">`).join("")
      : `<img src="/__out/${key}/${r.clip[frame]}" width="${w}" height="${h}">`;
    return `<div class="row"><div class="lab">${r.label}</div>${imgs}</div>`;
  }).join("");
  const sub = mode === "sheet" ? "4 s light, frames seeked exactly; 2× zoom" : `t = ${(frame / CLIP_FPS).toFixed(2)} s · 2× zoom`;
  return `<!doctype html><meta charset=utf-8><style>
    body{margin:0;background:#07070b;color:#e9e9ef;font:600 22px/1.3 Inter,system-ui,sans-serif;padding:24px;display:inline-block}
    h1{font-size:30px;margin:0 0 4px} p{margin:0 0 18px;color:#9c9cab;font-weight:500}
    .row{display:flex;gap:12px;align-items:center;margin-bottom:12px}
    .lab{width:300px;flex:none;font-family:'JetBrains Mono',monospace;font-size:22px}
    .cap{color:#9c9cab;font-size:20px;text-align:center}
    img{display:block;border:1px solid #2b2b39;border-radius:6px;background:#0d0d13}
  </style><h1>Agent light — ${m.title}</h1><p>${sub}</p>${head}${body}`;
}

function gridHtml(key, m) {
  const w = Math.round(m.still.width * 2);
  const h = Math.round(m.still.height * 2);
  const cells = m.rows.map((r) => `<figure><figcaption>${r.label}</figcaption><img src="/__out/${key}/${r.still}" width="${w}" height="${h}"></figure>`).join("");
  return `<!doctype html><meta charset=utf-8><style>
    body{margin:0;background:#07070b;color:#e9e9ef;font:600 22px/1.3 Inter,system-ui,sans-serif;padding:24px;display:inline-block}
    h1{font-size:30px;margin:0 0 4px} p{margin:0 0 18px;color:#9c9cab;font-weight:500}
    .grid{display:grid;grid-template-columns:repeat(${m.gridColumns},${w}px);gap:16px}
    figure{margin:0} figcaption{font-family:'JetBrains Mono',monospace;font-size:24px;margin-bottom:6px}
    img{display:block;border:1px solid #2b2b39;border-radius:6px;background:#0d0d13}
  </style><h1>Agent light — ${m.title}</h1><p>Each look at its full light (the still / reduced-motion form) · 2× zoom</p><div class="grid">${cells}</div>`;
}

async function renderHtml(html, file) {
  await setViewport(1200, 800, 1);
  await cdp("Emulation.setEmulatedMedia", { features: [] });
  const htmlFile = `${file}.html`;
  await writeFile(htmlFile, html);
  await cdp("Page.navigate", { url: `${base}__out/${path.relative(outDir, htmlFile)}` });
  await sleep(150);
  await evaluate(`Promise.all([...document.images].map((i) => i.complete ? 0 : new Promise((r) => { i.onload = i.onerror = r; })))`);
  const size = await evaluate(`({ w: Math.ceil(document.body.getBoundingClientRect().width), h: Math.ceil(document.body.scrollHeight) })`);
  await setViewport(size.w, size.h, 1);
  await sleep(50);
  const { data } = await cdp("Page.captureScreenshot", { format: "png", clip: { x: 0, y: 0, width: size.w, height: size.h, scale: 1 } });
  await writeFile(file, Buffer.from(data, "base64"));
  await rm(htmlFile);
}

for (const [key, m] of Object.entries(manifest)) {
  await renderHtml(sheetHtml(key, m, "sheet"), path.join(outDir, `${key}-${m.gridColumns ? "strip" : "sheet"}.png`));
  if (m.gridColumns) await renderHtml(gridHtml(key, m), path.join(outDir, `${key}-sheet.png`));
  const frames = path.join(outDir, key, "frames");
  await mkdir(frames, { recursive: true });
  for (let k = 0; k < m.rows[0].clip.length; k++) {
    await renderHtml(sheetHtml(key, m, "clip", k), path.join(frames, `${String(k).padStart(3, "0")}.png`));
  }
  await new Promise((resolve, reject) => {
    const ff = spawn("ffmpeg", ["-y", "-loglevel", "error", "-framerate", String(CLIP_FPS), "-i", path.join(frames, "%03d.png"), "-vf", "pad=ceil(iw/2)*2:ceil(ih/2)*2,tpad=stop_mode=clone:stop_duration=1", "-pix_fmt", "yuv420p", "-c:v", "libx264", "-crf", "18", path.join(outDir, `${key}-motion.mp4`)], { stdio: "inherit" });
    ff.on("exit", (c) => (c === 0 ? resolve() : reject(new Error(`ffmpeg ${c}`))));
  });
  console.log(`wrote ${key}-sheet.png and ${key}-motion.mp4`);
}

await send("Browser.close").catch(() => {});
server.close();
await rm(profile, { recursive: true, force: true }).catch(() => {});
