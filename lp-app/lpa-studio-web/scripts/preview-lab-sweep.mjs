#!/usr/bin/env node
// Preview-lab measurement sweep (PoC A / M1; GPU-vs-CPU tiers added in
// lp-gfx M3). Launches headless Chrome, opens the lab page per
// configuration with autostart, waits for the run to reach "running", then
// samples window.__labStats and records averaged results.
//
// The GPU tier requires WebGPU in headless Chrome: the launcher passes
// --enable-unsafe-webgpu and --use-angle=metal (override/extend with
// CHROME_EXTRA_FLAGS). Runs whose cards were granted a different tier than
// requested are flagged in the output (no-silent-fallback ADR) — never
// treat a cpu-granted "gpu" run as a GPU measurement.

import { spawn } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";

const BASE_URL = process.env.LAB_BASE_URL ?? "http://127.0.0.1:2861/";
const CHROME =
  process.env.CHROME_BIN ?? "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
const OUT_FILE = process.argv[2] ?? "preview-lab-results.json";

const WARMUP_MS = 10_000;
const SAMPLES = 3;
const SAMPLE_GAP_MS = 4_000;
const RUNNING_TIMEOUT_MS = 300_000;

function sweepConfigs() {
  const configs = [];
  // Both tiers over the same m1-shaped matrix so the GPU-vs-CPU comparison
  // is one JSON file. SWEEP_TIERS=gpu (comma list) restricts the matrix —
  // used to resume an interrupted sweep half without redoing the other.
  const tiers = (process.env.SWEEP_TIERS ?? "cpu,gpu").split(",").filter(Boolean);
  for (const tier of tiers) {
    // Scaling: N x workers at 15fps / 128px / basic.
    for (const workers of [1, 2, 4]) {
      for (const cards of [1, 5, 10, 20, 40]) {
        configs.push({ tier, cards, workers, fps: 15, size: 128, project: "basic" });
      }
    }
    // fps / size envelope at N=20, workers=4.
    configs.push({ tier, cards: 20, workers: 4, fps: 10, size: 128, project: "basic" });
    configs.push({ tier, cards: 20, workers: 4, fps: 20, size: 128, project: "basic" });
    configs.push({ tier, cards: 20, workers: 4, fps: 15, size: 96, project: "basic" });
    configs.push({ tier, cards: 20, workers: 4, fps: 15, size: 64, project: "basic" });
    // Project variety at N=5, workers=2.
    for (const project of ["fluid", "events", "fyeah-sign"]) {
      configs.push({ tier, cards: 5, workers: 2, fps: 15, size: 128, project });
    }
  }
  return configs;
}

class Cdp {
  static async open(url) {
    const ws = new WebSocket(url);
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      ws.addEventListener("error", reject, { once: true });
    });
    return new Cdp(ws);
  }
  constructor(ws) {
    this.ws = ws;
    this.nextId = 1;
    this.pending = new Map();
    // A dropped DevTools socket (Chrome crash, OOM, disk-full) must fail
    // the sweep loudly: if pending promises are simply abandoned, node's
    // event loop drains and the process exits 0 mid-sweep looking healthy.
    ws.addEventListener("close", () => {
      for (const pending of this.pending.values()) {
        pending.reject(new Error("CDP socket closed"));
      }
      this.pending.clear();
    });
    ws.addEventListener("message", (event) => {
      const message = JSON.parse(event.data.toString());
      if (!message.id) return;
      const pending = this.pending.get(message.id);
      if (!pending) return;
      this.pending.delete(message.id);
      message.error
        ? pending.reject(new Error(message.error.message))
        : pending.resolve(message.result ?? {});
    });
  }
  send(method, params = {}, sessionId = undefined, timeoutMs = 30_000) {
    const id = this.nextId++;
    const message = { id, method, params };
    if (sessionId) message.sessionId = sessionId;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        if (this.pending.delete(id)) reject(new Error(`CDP ${method} timed out`));
      }, timeoutMs);
      timer.unref?.();
      this.pending.set(id, {
        resolve: (v) => (clearTimeout(timer), resolve(v)),
        reject: (e) => (clearTimeout(timer), reject(e)),
      });
      this.ws.send(JSON.stringify(message));
    });
  }
  close() {
    this.ws.close();
  }
}

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function launchChrome() {
  const userDataDir = await mkdtemp(path.join(tmpdir(), "preview-lab-chrome-"));
  const port = 9260 + Math.floor(Math.random() * 300);
  const extraFlags = (process.env.CHROME_EXTRA_FLAGS ?? "").split(/\s+/).filter(Boolean);
  const child = spawn(
    CHROME,
    [
      "--headless=new",
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${userDataDir}`,
      "--no-first-run",
      "--no-default-browser-check",
      "--disable-background-timer-throttling",
      // WebGPU for the GPU tier. headless=new keeps the GPU process; ANGLE
      // on Metal gives the real adapter on macOS.
      "--enable-unsafe-webgpu",
      "--use-angle=metal",
      "--window-size=1400,1000",
      ...extraFlags,
      "about:blank",
    ],
    { stdio: "ignore" },
  );
  for (let i = 0; i < 100; i++) {
    await delay(200);
    try {
      const res = await fetch(`http://127.0.0.1:${port}/json/version`);
      const info = await res.json();
      return { child, port, userDataDir, wsUrl: info.webSocketDebuggerUrl, version: info.Browser };
    } catch {}
  }
  throw new Error("Chrome DevTools endpoint did not come up");
}

function labUrl(config) {
  const query = `cards=${config.cards}&workers=${config.workers}&fps=${config.fps}&size=${config.size}&project=${config.project}&tier=${config.tier ?? "cpu"}&autostart=1`;
  return `${BASE_URL}?r=${Date.now()}#/preview-lab?${query}`;
}

async function evalStats(cdp, sessionId) {
  const result = await cdp.send(
    "Runtime.evaluate",
    { expression: "window.__labStats || null", returnByValue: true },
    sessionId,
  );
  const raw = result.result?.value;
  return raw ? JSON.parse(raw) : null;
}

async function runConfig(cdp, config) {
  const { targetId } = await cdp.send("Target.createTarget", { url: labUrl(config) });
  const { sessionId } = await cdp.send("Target.attachToTarget", { targetId, flatten: true });
  try {
    const startedAt = Date.now();
    let stats = null;
    while (Date.now() - startedAt < RUNNING_TIMEOUT_MS) {
      await delay(1000);
      stats = await evalStats(cdp, sessionId).catch(() => null);
      if (stats && stats.phase === "running") break;
      if (stats && /failed|no cards/.test(stats.phase)) {
        return { config, error: `setup failed: ${stats.phase}`, notes: stats.notes };
      }
    }
    if (!stats || stats.phase !== "running") {
      return { config, error: `timed out in phase ${stats ? stats.phase : "unknown"}` };
    }
    const deployMs = Date.now() - startedAt;
    await delay(WARMUP_MS);
    const samples = [];
    for (let i = 0; i < SAMPLES; i++) {
      if (i > 0) await delay(SAMPLE_GAP_MS);
      const sample = await evalStats(cdp, sessionId);
      if (sample) samples.push(sample);
    }
    if (samples.length === 0) return { config, error: "no samples" };
    return { config, deployMs, result: summarize(config, samples) };
  } finally {
    await cdp.send("Target.closeTarget", { targetId }).catch(() => {});
  }
}

function summarize(config, samples) {
  const mean = (values) => values.reduce((a, b) => a + b, 0) / values.length;
  const aggKeys = [
    "total_fps",
    "mean_tick_ms",
    "mean_render_ms",
    "mean_transport_ms",
    "mean_present_ms",
    "est_worker_cores",
    "est_present_cores",
  ];
  const aggregate = {};
  for (const key of aggKeys) aggregate[key] = mean(samples.map((s) => s.aggregate[key]));
  const last = samples[samples.length - 1];
  const cardFps = last.cards.map((c) => c.stats.achieved_fps);
  // No-silent-fallback check: every card must run the tier the config asked
  // for (in "both" layouts even indexes are gpu, odd cpu).
  const requestedTier = (index) => {
    const tier = config.tier ?? "cpu";
    if (tier === "both") return index % 2 === 0 ? "gpu" : "cpu";
    return tier;
  };
  const tierMismatches = last.cards
    .filter((c) => c.tier !== requestedTier(c.index))
    .map((c) => [c.index, c.tier, c.tier_reason]);
  return {
    granted_tiers: [...new Set(last.cards.map((c) => c.tier))],
    tier_mismatches: tierMismatches,
    aggregate,
    total_dropped: last.aggregate.total_dropped,
    total_errors: last.aggregate.total_errors,
    per_card_fps_min: Math.min(...cardFps),
    per_card_fps_max: Math.max(...cardFps),
    per_card_fps_mean: mean(cardFps),
    worker_wasm_memory_bytes: last.worker_wasm_memory_bytes,
    js_heap_bytes: last.js_heap_bytes,
    card_errors: last.cards.filter((c) => c.error).map((c) => [c.index, c.error]),
    notes: last.notes,
    elapsed_s: last.elapsed_s,
  };
}

const chrome = await launchChrome();
console.log(`chrome: ${chrome.version} (port ${chrome.port})`);
const cdp = await Cdp.open(chrome.wsUrl);
const results = [];
try {
  // SWEEP_ONLY=<substring> restricts runs by label (resume/one-off).
  const only = process.env.SWEEP_ONLY;
  for (const config of sweepConfigs()) {
    const label = `${config.tier ?? "cpu"} ${config.project} N=${config.cards} W=${config.workers} ${config.fps}fps ${config.size}px`;
    if (only && !label.includes(only)) continue;
    process.stdout.write(`run ${label} ... `);
    const outcome = await runConfig(cdp, config);
    results.push(outcome);
    if (outcome.error) {
      console.log(`ERROR: ${outcome.error}`);
    } else {
      const a = outcome.result.aggregate;
      console.log(
        `fps=${a.total_fps.toFixed(1)} tick=${a.mean_tick_ms.toFixed(2)} render=${a.mean_render_ms.toFixed(2)} xfer=${a.mean_transport_ms.toFixed(2)} present=${a.mean_present_ms.toFixed(3)} cores=${a.est_worker_cores.toFixed(2)} deploy=${(outcome.deployMs / 1000).toFixed(1)}s`,
      );
    }
    await writeFile(OUT_FILE, JSON.stringify({ chrome: chrome.version, results }, null, 2));
  }
} finally {
  cdp.close();
  chrome.child.kill();
  await rm(chrome.userDataDir, { recursive: true, force: true }).catch(() => {});
}
console.log(`wrote ${OUT_FILE}`);
