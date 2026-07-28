#!/usr/bin/env node
// Deterministic repro + regression check for the code-editor gutter alignment
// defect (docs/defects/2026-07-27-code-editor-gutter-misaligned.md).
//
// Loads a story the way the capture pipeline does — full page load in headless
// Chrome with forced BeginFrames — and reports whether the line-number gutter
// advances at the same rate as the code lines. Exits non-zero if any run is
// misaligned.
//
// The defect only appears when the editor is BELOW THE FOLD at load, because
// CodeMirror's ViewState.measure() consumes its "re-measure the content" flags
// and then returns early while the editor is outside the window — so the height
// oracle keeps the library default of 14px while the content lays out at 18px.
// The viewport-height argument is therefore the load-bearing one: at the story
// viewport height of 760 the editor sits just above the fold and the bug hides.
//
//   # serve a release story build first, e.g.
//   #   just studio-web-story-build
//   #   (cd target/dx/lpa-studio-web/release/web/public && python3 -m http.server 31447)
//   node gutter-alignment-probe.mjs http://127.0.0.1:31447/ \
//        '#/stories/base/code-editor/overview' 3 1 400   # 3 runs, no throttle, 400px tall
//
// Args: <baseUrl> [storyPath] [runs] [cpuThrottleRate] [viewportHeight]
// Env:  PROBE_EXPR overrides the in-page expression (must be side-effect free —
//       it is re-evaluated while polling for readiness); INJECT_CSS installs a
//       stylesheet before load, for A/B testing candidate fixes.

import { spawn } from "node:child_process";
import { mkdtemp, rm } from "node:fs/promises";
import { once } from "node:events";
import { tmpdir } from "node:os";
import path from "node:path";

const baseUrl = process.argv[2] ?? "http://127.0.0.1:29958/";
const storyPath = process.argv[3] ?? "#/stories/base/code-editor/overview";
const runs = Number(process.argv[4] ?? 5);
// CI runners are slower than a dev Mac and capture several pages at once. The
// defect is a measurement/layout race, so throttle the CPU to widen it.
const cpuThrottle = Number(process.argv[5] ?? 1);
// Viewport height: the defect needs the editor to be BELOW THE FOLD at load,
// which is what makes CodeMirror's inView check fail and discard its measure.
const viewH = Number(process.argv[6] ?? 760);
const chrome =
  process.env.CHROME_BIN ??
  (process.platform === "darwin"
    ? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
    : "google-chrome");

const PROBE = process.env.PROBE_EXPR ?? `
(() => {
  const ed = document.querySelector('.cm-editor');
  if (!ed) return JSON.stringify({ready: false, why: 'no .cm-editor'});
  const content = ed.querySelector('.cm-content');
  if (!content || !content.cmView) return JSON.stringify({ready: false, why: 'no cmView'});
  const view = content.cmView.view;
  const nums = [...ed.querySelectorAll('.cm-lineNumbers .cm-gutterElement')]
    .filter(e => e.getBoundingClientRect().height > 0);
  const lines = [...ed.querySelectorAll('.cm-content .cm-line')];
  if (nums.length < 2 || lines.length < 2) return JSON.stringify({ready: false, why: 'too few rows'});
  const gutterAdvance = Math.round(nums[1].getBoundingClientRect().top - nums[0].getBoundingClientRect().top);
  const lineAdvance = Math.round(lines[1].getBoundingClientRect().top - lines[0].getBoundingClientRect().top);
  // Pair gutter label N with content line N — the alignment the user sees.
  const offsets = nums.map(g => {
    const i = parseInt(g.textContent.trim(), 10);
    if (!i || !lines[i - 1]) return null;
    return Math.round(g.getBoundingClientRect().top - lines[i - 1].getBoundingClientRect().top);
  }).filter(v => v !== null);
  return JSON.stringify({
    ready: true,
    oracleLineHeight: view.viewState.heightOracle.lineHeight,
    gutterAdvance,
    lineAdvance,
    aligned: gutterAdvance === lineAdvance,
    maxLabelOffset: offsets.length ? Math.max(...offsets.map(Math.abs)) : null,
    editorTop: Math.round(ed.getBoundingClientRect().top),
    editorBottom: Math.round(ed.getBoundingClientRect().bottom),
    winH: window.innerHeight,
    inView: view.viewState.inView,
    belowFold: ed.getBoundingClientRect().top >= window.innerHeight,
  });
})()
`;

class Cdp {
  static async open(url) {
    const ws = new WebSocket(url);
    await new Promise((res, rej) => {
      ws.addEventListener("open", res, { once: true });
      ws.addEventListener("error", rej, { once: true });
    });
    return new Cdp(ws);
  }
  constructor(ws) {
    this.ws = ws;
    this.id = 1;
    this.pending = new Map();
    ws.addEventListener("message", (e) => {
      const m = JSON.parse(e.data.toString());
      if (!m.id) return;
      const p = this.pending.get(m.id);
      if (!p) return;
      this.pending.delete(m.id);
      m.error ? p.rej(new Error(m.error.message)) : p.res(m.result ?? {});
    });
  }
  send(method, params = {}, sessionId) {
    const id = this.id++;
    const msg = { id, method, params };
    if (sessionId) msg.sessionId = sessionId;
    return new Promise((res, rej) => {
      const timer = setTimeout(() => {
        if (this.pending.delete(id)) rej(new Error(`${method} timed out`));
      }, 30_000);
      timer.unref?.();
      this.pending.set(id, {
        res: (v) => (clearTimeout(timer), res(v)),
        rej: (e) => (clearTimeout(timer), rej(e)),
      });
      this.ws.send(JSON.stringify(msg));
    });
  }
  close() {
    this.ws.close();
  }
}

async function waitForDevTools(child) {
  return new Promise((resolve, reject) => {
    let buf = "";
    const t = setTimeout(() => reject(new Error("DevTools never started")), 15_000);
    child.stderr.on("data", (c) => {
      buf += c;
      const m = buf.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (m) {
        clearTimeout(t);
        resolve(m[1]);
      }
    });
  });
}

const userDataDir = await mkdtemp(path.join(tmpdir(), "gutter-probe-chrome-"));
const child = spawn(
  chrome,
  [
    "--headless=new",
    "--disable-gpu",
    "--disable-dev-shm-usage",
    "--hide-scrollbars",
    "--no-first-run",
    "--no-default-browser-check",
    "--remote-debugging-port=0",
    "--window-size=1080,760",
    `--user-data-dir=${userDataDir}`,
    "about:blank",
  ],
  { stdio: ["ignore", "ignore", "pipe"] },
);
const exited = once(child, "exit").catch(() => {});
const cdp = await Cdp.open(await waitForDevTools(child));

const results = [];
for (let run = 0; run < runs; run++) {
  const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
  await cdp.send("Page.enable", {}, sessionId);
  await cdp.send("Runtime.enable", {}, sessionId);
  await cdp.send(
    "Emulation.setDeviceMetricsOverride",
    { width: 390, height: viewH, deviceScaleFactor: 1, mobile: false },
    sessionId,
  );
  if (cpuThrottle > 1) {
    await cdp.send("Emulation.setCPUThrottlingRate", { rate: cpuThrottle }, sessionId);
  }
  if (process.env.INJECT_CSS) {
    await cdp.send("Page.addScriptToEvaluateOnNewDocument", {source: `
      document.addEventListener("DOMContentLoaded", () => {
        const st = document.createElement("style");
        st.textContent = ${JSON.stringify(process.env.INJECT_CSS)};
        document.head.appendChild(st);
      });
      const obs = new MutationObserver(() => {
        if (document.head && !document.getElementById("probe-css")) {
          const st = document.createElement("style");
          st.id = "probe-css";
          st.textContent = ${JSON.stringify(process.env.INJECT_CSS)};
          document.head.appendChild(st);
        }
      });
      obs.observe(document.documentElement, {childList: true, subtree: true});
    `}, sessionId);
  }
  // Full page load, exactly like the capture pipeline (not an SPA hash nav).
  await cdp.send("Page.navigate", { url: `${baseUrl}${storyPath}` }, sessionId);

  let probe = { ready: false };
  const started = Date.now();
  while (Date.now() - started < 25_000) {
    const { result } = await cdp.send(
      "Runtime.evaluate",
      { expression: PROBE, returnByValue: true, awaitPromise: true },
      sessionId,
    );
    probe = JSON.parse(result.value);
    if (probe.ready) break;
    // Headless stops producing frames once load-time BeginFrames are spent; a
    // discarded screenshot forces one so rAF-driven work can progress.
    await cdp
      .send(
        "Page.captureScreenshot",
        { format: "png", clip: { x: 0, y: 0, width: 1, height: 1, scale: 1 } },
        sessionId,
      )
      .catch(() => {});
    await new Promise((r) => setTimeout(r, 100));
  }
  if (process.env.SWEEP === "1") {
    const m = JSON.parse((await cdp.send("Runtime.evaluate", {expression:
      `JSON.stringify({h: window.innerHeight, s: Math.max(document.documentElement.scrollHeight, document.body.scrollHeight)})`,
      returnByValue: true}, sessionId)).result.value);
    const step = Math.max(1, Math.floor(m.h * 0.9));
    const stops = Math.min(Math.ceil(m.s / step), 12);
    for (let i = 1; i <= stops; i++) {
      await cdp.send("Runtime.evaluate", {expression: `(window.scrollTo(0, ${i*step}), true)`, returnByValue: true}, sessionId);
      await cdp.send("Page.captureScreenshot", {format:"png", clip:{x:0,y:0,width:1,height:1,scale:1}}, sessionId).catch(()=>{});
    }
    await cdp.send("Runtime.evaluate", {expression: "(window.scrollTo(0,0), true)", returnByValue: true}, sessionId);
    await cdp.send("Page.captureScreenshot", {format:"png", clip:{x:0,y:0,width:1,height:1,scale:1}}, sessionId).catch(()=>{});
  }
  // Let any late measurement settle, forcing frames throughout.
  for (let i = 0; i < 12; i++) {
    await cdp
      .send(
        "Page.captureScreenshot",
        { format: "png", clip: { x: 0, y: 0, width: 1, height: 1, scale: 1 } },
        sessionId,
      )
      .catch(() => {});
    await new Promise((r) => setTimeout(r, 100));
  }
  const { result } = await cdp.send(
    "Runtime.evaluate",
    { expression: PROBE, returnByValue: true, awaitPromise: true },
    sessionId,
  );
  const final = JSON.parse(result.value);
  results.push(final);
  console.log(`run ${run + 1}: ${JSON.stringify(final)}`);
  await cdp.send("Target.closeTarget", { targetId });
}

const aligned = results.filter((r) => r.aligned).length;
console.log(`\n${aligned}/${results.length} runs aligned (gutter advance == line advance)`);

cdp.close();
child.kill();
await exited;
await rm(userDataDir, { recursive: true, force: true });
process.exit(aligned === results.length ? 0 : 1);
