#!/usr/bin/env bun
// Runs harness.js against a served lab page in headless Chrome and prints
// what it measured, one JSON object per project.
//
//   bun lab/mapping/web/perf/run.mjs http://127.0.0.1:<port>/ [project…]
//
// Headless (`--headless=new`, never a visible window: other agents share the
// desk), 1440 × 900 at device pixel ratio 2, like a laptop screen. Its
// frames are real frames — rAF paced, painted — but rasterized however
// headless Chrome rasterizes, so a desk browser can differ: read the numbers
// as the page's own work, and compare before and after on the same runner.

import { spawn } from "node:child_process";
import { readFileSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

const url = process.argv[2];
const projects = process.argv.slice(3);
if (!url) {
  console.error("usage: run.mjs <page url> [project…]");
  process.exit(2);
}
const chrome =
  process.env.CHROME_BIN ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const profile = mkdtempSync(path.join(tmpdir(), "lab-perf-"));
const proc = spawn(chrome, [
  "--headless=new",
  "--remote-debugging-port=0",
  `--user-data-dir=${profile}`,
  "--window-size=1440,900",
  "--force-device-scale-factor=2",
  "--no-first-run",
  "--no-default-browser-check",
  "about:blank",
]);
const wsUrl = await new Promise((resolve, reject) => {
  let err = "";
  proc.stderr.on("data", (d) => {
    err += d;
    const m = err.match(/DevTools listening on (ws:\/\/\S+)/);
    if (m) resolve(m[1]);
  });
  proc.on("exit", () => reject(new Error(err)));
});

// Talk to the page target.
const version = await (await fetch(wsUrl.replace("ws://", "http://").replace(/\/devtools.*/, "/json/list"))).json();
const page = version.find((t) => t.type === "page");
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r) => ws.addEventListener("open", r, { once: true }));
let id = 0;
const pending = new Map();
ws.addEventListener("message", (e) => {
  const m = JSON.parse(e.data);
  if (m.id && pending.has(m.id)) {
    pending.get(m.id)(m);
    pending.delete(m.id);
  }
});
const call = (method, params = {}) =>
  new Promise((resolve) => {
    const n = ++id;
    pending.set(n, resolve);
    ws.send(JSON.stringify({ id: n, method, params }));
  });
const evaluate = async (expression) => {
  const r = await call("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
  if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails));
  return r.result?.result?.value;
};

await call("Page.enable");
await call("Page.navigate", { url });
// Wait for the page's canvas to exist.
await evaluate(`new Promise((res) => { const t = setInterval(() => { if (document.querySelector('.canvas') && document.querySelector('.fixture-picker select')) { clearInterval(t); res(true); } }, 50); })`);
await evaluate(readFileSync(new URL("./harness.js", import.meta.url), "utf8"));

const wanted = projects.length ? projects : ["playful-choker", "zook-dome", "radiance-dome"];
const repeats = { "radiance-dome": "r3-p01", "zook-dome": "sector" };
for (const p of wanted) {
  const out = await evaluate(
    `labPerf.suite(${JSON.stringify(p)}, { frames: 60, repeat: ${JSON.stringify(repeats[p] ?? null)} })`,
  );
  console.log(JSON.stringify(out));
  if (process.env.SHOTS) {
    const shot = await call("Page.captureScreenshot", { format: "png" });
    writeFileSync(path.join(process.env.SHOTS, `perf-${p}.png`), Buffer.from(shot.result.data, "base64"));
  }
}
ws.close();
proc.kill();
