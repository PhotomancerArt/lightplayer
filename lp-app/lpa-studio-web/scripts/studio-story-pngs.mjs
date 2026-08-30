#!/usr/bin/env node

import {
  mkdir,
  mkdtemp,
  readdir,
  readFile,
  rename,
  rm,
  stat,
  unlink,
  writeFile,
} from "node:fs/promises";
import { spawn, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import { once } from "node:events";
import { createReadStream } from "node:fs";
import { createServer as createHttpServer } from "node:http";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";
import path from "node:path";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");
const publicDir = path.resolve(
  repoRoot,
  process.env.STUDIO_STORY_SITE_DIR ?? "target/dx/lpa-studio-web/debug/web/public",
);
const storyRoot = path.join(repoRoot, "lp-app/lpa-studio-web");
const { mode, storyFilters } = parseCliArgs(process.argv.slice(2));
// Default to an OS-assigned free port so parallel runs (multiple agents, each in
// its own git worktree — which isolates files but NOT ports) never fight over a
// fixed port. Set STUDIO_STORY_PNGS_PORT to pin one (e.g. for debugging).
const port = process.env.STUDIO_STORY_PNGS_PORT ?? String(await findFreePort());
const requestedCaptureConcurrency = parseCaptureConcurrency();
const captureTimeoutMs = parsePositiveIntegerEnv("STUDIO_STORY_CAPTURE_TIMEOUT_MS", 10_000);
// Hard ceiling on any single Chrome DevTools call, so a wedged renderer fails
// fast (and gets retried) instead of blocking the run indefinitely.
const cdpCallTimeoutMs = parsePositiveIntegerEnv("STUDIO_STORY_CDP_TIMEOUT_MS", 30_000);
// A capture pass can still die to a wedged Chrome (CDP navigation timeouts under
// parallel wasm loads) even after the per-target fresh-page retry. A re-run
// resumes from the already-captured files and has always completed in practice,
// so retry the pass in-process instead of taxing callers with a manual re-run.
const captureAttempts = parsePositiveIntegerEnv("STUDIO_STORY_CAPTURE_ATTEMPTS", 2);
// Fresh browser every N captures: a long-lived capture browser degrades into
// permanent navigation stalls after a few hundred same-process navigations of
// the release wasm bundle (CI runners wedge around ~200; a CPU-limited local
// container around ~580). Restarts cost ~1-2s and resume from disk, so a low
// ceiling is cheap insurance everywhere, including local runs.
const browserRestartEvery = parsePositiveIntegerEnv("STUDIO_STORY_BROWSER_RESTART_EVERY", 120);
// Hard ceiling on any child process this script waits on (`runProcess`).
// Load-bearing: the two 2026-08-05 CI wedges (5h08m and 3h35m of runner time,
// both cancelled by hand) were the story-DISCOVERY Chrome — a `--dump-dom`
// run that never exited, awaited by a bare `once(child, "exit")` with no
// bound. Every existing timeout in this file guards the CDP capture path,
// which discovery never reaches, so the run went silent between "story build"
// and the first capture line and stayed there until GitHub's 6-hour default.
// A timeout on the process wait is the structural fix; the Chrome-side reason
// it hangs is unidentified and does not need to be, to bound it.
const subprocessTimeoutMs = parsePositiveIntegerEnv("STUDIO_STORY_SUBPROCESS_TIMEOUT_MS", 180_000);
// Discovery gets its own bound plus retries: it is a single short Chrome run,
// and a killed attempt costs nothing to redo, so retrying keeps a transient
// browser hang from turning the whole job red. Sized off a MEASURED happy
// path, not off `--virtual-time-budget=5000`: a cold Chrome takes ~21s here
// (launch and profile setup dominate the virtual-time budget), so the default
// is ~6x that rather than the "few seconds" the flag suggests. Err generous —
// the failure mode being fixed is a 5-hour hang, not a slow discovery.
const discoveryTimeoutMs = parsePositiveIntegerEnv("STUDIO_STORY_DISCOVERY_TIMEOUT_MS", 120_000);
const discoveryAttempts = parsePositiveIntegerEnv("STUDIO_STORY_DISCOVERY_ATTEMPTS", 3);
// Backstop deadline for the ENTIRE run, enforced by a watchdog timer rather
// than by racing individual awaits — so it covers phases nobody has thought to
// bound yet, which is precisely the class the 2026-08-05 wedges belonged to.
// Sized above the slowest healthy CI run (p95 23.7 min, max 24.4) with room to
// spare; the `validate-stories` job timeout (45 min) is the outer backstop, and
// this fires first so the failure carries a diagnostic instead of a bare
// "operation was canceled".
const runDeadlineMs = parsePositiveIntegerEnv("STUDIO_STORY_RUN_DEADLINE_MS", 40 * 60_000);
// Heartbeat cadence. The point is not the capture phase (every capture already
// logs a `wrote` line) — it is the SILENT phases: a wedge must say where it
// stopped instead of emitting nothing for hours.
const progressIntervalMs = parsePositiveIntegerEnv("STUDIO_STORY_PROGRESS_INTERVAL_MS", 30_000);
// Marker file (inside the capture dir) recording the build a partial capture
// belongs to, so a re-run can resume it only when the build is unchanged.
const CAPTURE_BUILD_FILE = ".capture-build";
// HARNESS SEAM — second half of the contract documented at
// `component_overview_id` in src/stories/story_book.rs: the story book
// synthesizes one `<family>/[<category>/]<component>/overview` page per
// component that stacks every story of that component, and no `#[story]`
// function can claim that id (pinned by a unit test there).
// Declared with the other module constants, ABOVE the top-level await that
// drives the run — a `const` further down the file is in its temporal dead
// zone by the time `discoverStoryIds()` reads it, which `node --check` cannot
// see because it only parses.
const OVERVIEW_COMPOSITE_SUFFIX = "/overview";
// Written beside `.check-complete` by a complete `check`: the baseline files
// that actually need replacing/removing. See the write site for why consumers
// must not just swap the whole set.
const REFRESH_MANIFEST_FILE = ".refresh-manifest.json";
// Captures of the same build still differ in a few pixels from anti-aliasing and
// sub-pixel text layout jitter (high per-channel delta, but only along glyph edges).
// So `check` counts pixels whose per-channel delta exceeds a significance threshold
// and fails only when that count is more than a small fraction of the image —
// pixelmatch-style — rather than gating on the single worst pixel. This has a noise
// floor: changes below the ratio don't fail the check (reviewers still see the
// baseline image diff in the PR).
// Ceiling for the grow-to-fit viewport (see `fitViewportToStory`). Tall enough
// for every current story sheet, low enough that a runaway one can't ask Chrome
// for an enormous surface.
const storyViewportMaxHeight = parsePositiveIntegerEnv("STUDIO_STORY_MAX_VIEWPORT_HEIGHT", 8000);
const significanceDelta = parsePositiveIntegerEnv("STUDIO_STORY_MAX_CHANNEL_DELTA", 64);
const maxSignificantPixelRatio = parseRatioEnv("STUDIO_STORY_MAX_DIFF_PIXEL_RATIO", 0.0005);
const baseUrl = `http://127.0.0.1:${port}/`;
const chrome = process.env.CHROME_BIN ?? findChrome();
const baselineDir = path.resolve(repoRoot, baselineDirFromEnv());
const outputDir = path.resolve(repoRoot, outputDirForMode(mode));
const captureDir = mode === "baselines" ? path.join(baselineDir, ".new") : outputDir;
const STORY_VIEWPORTS = [
  { id: "sm", width: 390, height: 760 },
  { id: "md", width: 720, height: 760 },
  { id: "lg", width: 1080, height: 760 },
];
// Stories marked `#[story(screenshot)]` are published images (README heroes,
// docs figures), not the three-size design record: they capture at one size
// only, so the unused sizes cannot churn baselines. Populated by discovery.
const SCREENSHOT_VIEWPORT_ID = "lg";
const SCREENSHOT_STORY_IDS = new Set();

class CdpConnection {
  static async open(url) {
    const ws = new WebSocket(url);
    await new Promise((resolve, reject) => {
      ws.addEventListener("open", resolve, { once: true });
      ws.addEventListener("error", reject, { once: true });
    });
    return new CdpConnection(ws);
  }

  constructor(ws) {
    this.nextId = 1;
    this.pending = new Map();
    this.ws = ws;
    this.ws.addEventListener("message", (event) => this.onMessage(event));
    this.ws.addEventListener("close", () => this.rejectAll(new Error("Chrome DevTools closed")));
    this.ws.addEventListener("error", () => {
      this.rejectAll(new Error("Chrome DevTools connection failed"));
    });
  }

  send(method, params = {}, sessionId = undefined, timeoutMs = cdpCallTimeoutMs) {
    // A send after Chrome died would be silently discarded by the WebSocket and
    // sit out the full call timeout; fail it immediately instead.
    if (this.ws.readyState !== WebSocket.OPEN) {
      return Promise.reject(new Error(`Chrome DevTools connection is closed (${method})`));
    }
    const id = this.nextId;
    this.nextId += 1;
    const message = { id, method, params };
    if (sessionId) {
      message.sessionId = sessionId;
    }

    return new Promise((resolve, reject) => {
      // A wedged renderer can make Runtime.evaluate never respond, which would
      // otherwise hang the whole run forever (the ready-state loop's own timeout
      // never gets to re-check). Bounding every call turns that into a failed
      // capture the worker can retry on a fresh page.
      const timer = setTimeout(() => {
        if (this.pending.delete(id)) {
          reject(new Error(`CDP ${method} timed out after ${timeoutMs}ms`));
        }
      }, timeoutMs);
      timer.unref?.();
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer);
          resolve(value);
        },
        reject: (error) => {
          clearTimeout(timer);
          reject(error);
        },
      });
      this.ws.send(JSON.stringify(message));
    });
  }

  close() {
    this.ws.close();
  }

  onMessage(event) {
    const message = JSON.parse(event.data.toString());
    if (!message.id) {
      return;
    }
    const pending = this.pending.get(message.id);
    if (!pending) {
      return;
    }
    this.pending.delete(message.id);
    if (message.error) {
      pending.reject(new Error(`${message.error.message}: ${message.error.data ?? ""}`));
    } else {
      pending.resolve(message.result ?? {});
    }
  }

  rejectAll(error) {
    for (const pending of this.pending.values()) {
      pending.reject(error);
    }
    this.pending.clear();
  }
}

if (!chrome) {
  console.error(
    "Could not find Google Chrome. Set CHROME_BIN=/path/to/chrome to generate story PNGs.",
  );
  process.exit(1);
}

// Where the run currently is, and how far the capture has got. Both exist for
// one reason: so a hang is DIAGNOSABLE. The heartbeat prints them periodically
// and the watchdog prints them on the way out, which is the difference between
// "the job produced no output for five hours" and "it stopped in
// `discovering stories`".
const runStartedAt = Date.now();
let currentPhase = "starting up";
const progress = { captured: 0, total: 0 };

function setPhase(phase) {
  currentPhase = phase;
}

function elapsedLabel() {
  const seconds = Math.round((Date.now() - runStartedAt) / 1000);
  return `${String(Math.floor(seconds / 60)).padStart(2, "0")}:${String(seconds % 60).padStart(2, "0")}`;
}

function progressLine() {
  const captured =
    progress.total > 0 ? ` — ${progress.captured}/${progress.total} captured` : "";
  return `[+${elapsedLabel()}] ${currentPhase}${captured}`;
}

const heartbeat = setInterval(() => {
  console.log(progressLine());
}, progressIntervalMs);
// unref: a heartbeat must never be the reason the process stays alive. During a
// real wedge the event loop is held open by the wedged handle itself (a child
// process, a socket), so the timer still fires.
heartbeat.unref();

const watchdog = setTimeout(() => {
  console.error(
    `\nDEADLINE: story capture exceeded ${runDeadlineMs} ms without finishing.\n` +
      `  ${progressLine()}\n` +
      "  Raise STUDIO_STORY_RUN_DEADLINE_MS if this machine is genuinely that slow;\n" +
      "  otherwise the phase named above is where to look.",
  );
  // Exit code 3: distinct from drift (1) and usage errors (2), so a wedge is
  // never mistaken for a baseline difference.
  process.exit(3);
}, runDeadlineMs);
watchdog.unref();

// Resume support: fingerprint the built app (the wasm bytes → identical render
// output) and stash it in the capture dir. A re-run against the *same* build
// keeps whatever was already captured and only fills in the rest; any change to
// the build invalidates the cache and starts clean. This makes a killed or
// wedged run cheap to finish instead of redoing all ~660 captures.
const buildFingerprint = await computeBuildFingerprint();
const resuming = buildFingerprint !== null && (await readCaptureBuildId(captureDir)) === buildFingerprint;
if (resuming) {
  await mkdir(captureDir, { recursive: true });
  console.log(
    `Resuming capture for unchanged build ${buildFingerprint.slice(0, 12)} — already-captured viewports are skipped.`,
  );
} else {
  await rm(captureDir, { recursive: true, force: true });
  await mkdir(captureDir, { recursive: true });
  if (buildFingerprint !== null) {
    await writeFile(path.join(captureDir, CAPTURE_BUILD_FILE), buildFingerprint);
  }
}

// In-process static server. This replaced `python3 -m http.server`, which
// WEDGES under capture load: when a CDP timeout kills a Chrome page
// mid-download, the serving thread blocks forever in a kernel socket send
// (observed in sock_alloc_send_pskb) and the whole server stops answering —
// turning one transiently slow story into a permanent all-pages navigation
// wedge (the debt entry's long-mysterious "heavy end-of-queue sheets" class).
// Node destroys dead sockets instead of blocking on them.
const server = createStaticServer(publicDir);
await new Promise((resolve, reject) => {
  server.once("error", reject);
  server.listen(Number(port), "127.0.0.1", resolve);
});

try {
  setPhase("waiting for the static server");
  await waitForServer(baseUrl);
  setPhase("discovering stories (dump-dom Chrome)");
  console.log(`[+${elapsedLabel()}] Discovering stories...`);
  const discoveredStoryIds = await discoverStoryIds();
  if (discoveredStoryIds.length === 0) {
    throw new Error("No story links were discovered from the storybook page.");
  }
  const storyIds = filterStoryIds(discoveredStoryIds, storyFilters);
  console.log(`[+${elapsedLabel()}] Discovered ${storyIds.length} stories.`);

  // Clear any sentinel from a previous completed check so a crashed capture
  // can't inherit it (the sentinel is rewritten after a complete comparison).
  if (mode === "check") {
    await rm(path.join(captureDir, ".check-complete"), { force: true });
  }
  setPhase("capturing");
  const files = await captureStoriesWithRetry(storyIds, captureDir);
  setPhase("optimizing PNGs (oxipng)");
  console.log(`[+${elapsedLabel()}] Optimizing ${files.length} PNGs...`);
  await optimizePngs(files, { required: mode !== "pngs" });
  setPhase("comparing baselines");

  if (mode === "baselines") {
    await replaceBaselineImages(captureDir, outputDir);
    console.log(`Story baselines: ${path.relative(repoRoot, outputDir)}`);
  } else if (mode === "check") {
    const comparison = await compareBaselines(storyIds, baselineDir, outputDir);
    const ok = comparison.ok;
    // Sentinel: the comparison ran over a COMPLETE capture. Consumers of the
    // fresh-capture set (the CI artifact and `story-pull`) require this so a
    // crashed partial capture can't masquerade as story drift — staging a
    // partial set would delete every baseline it didn't reach.
    if (storyFilters.length === 0) {
      await writeFile(path.join(outputDir, ".check-complete"), `${new Date().toISOString()}\n`);
      // Refresh manifest: exactly which baselines the comparison judged stale.
      // Consumers must replace only these instead of swapping the whole set —
      // a wholesale copy also drags in the files this comparison deliberately
      // TOLERATED (sub-threshold AA noise), which is how run-to-run raster
      // jitter became committed baseline churn.
      await writeFile(
        path.join(outputDir, REFRESH_MANIFEST_FILE),
        `${JSON.stringify(
          {
            replace: comparison.replace,
            remove: comparison.remove,
            tolerated: comparison.tolerated,
          },
          null,
          2,
        )}\n`,
      );
    }
    if (!ok) {
      console.error("\nStory baselines differ. Run `just studio-story-baselines` to update them.");
      process.exitCode = 1;
    }
  } else {
    console.log(`Story PNGs: ${path.relative(repoRoot, outputDir)}`);
  }
} finally {
  clearInterval(heartbeat);
  clearTimeout(watchdog);
  server.closeAllConnections();
  await new Promise((resolve) => server.close(resolve));
}

// Minimal static file server over publicDir. MIME types cover what the story
// build actually serves; `application/wasm` is load-bearing (streaming
// compile refuses other types).
function createStaticServer(rootDir) {
  const mimeTypes = {
    ".css": "text/css",
    ".html": "text/html; charset=utf-8",
    ".ico": "image/x-icon",
    ".js": "text/javascript",
    ".json": "application/json",
    ".mjs": "text/javascript",
    ".png": "image/png",
    ".svg": "image/svg+xml",
    ".wasm": "application/wasm",
    ".woff2": "font/woff2",
  };
  return createHttpServer((request, response) => {
    const fatal = (status) => {
      response.writeHead(status);
      response.end();
    };
    let pathname;
    try {
      pathname = decodeURIComponent(new URL(request.url, "http://localhost/").pathname);
    } catch {
      return fatal(400);
    }
    if (pathname.endsWith("/")) {
      pathname += "index.html";
    }
    const filePath = path.join(rootDir, pathname);
    // path.join normalizes ".." segments; anything escaping rootDir is refused.
    if (!filePath.startsWith(rootDir + path.sep)) {
      return fatal(403);
    }
    const stream = createReadStream(filePath);
    stream.once("error", () => fatal(404));
    stream.once("open", () => {
      response.writeHead(200, {
        "content-type": mimeTypes[path.extname(filePath)] ?? "application/octet-stream",
        "cache-control": "no-store",
      });
      stream.pipe(response);
    });
    // A page killed mid-download (CDP timeout recycling) must tear down the
    // stream, never block the server.
    response.once("close", () => stream.destroy());
  });
}

function parseCliArgs(args) {
  const modes = ["pngs", "baselines", "check"];
  let mode = "pngs";
  let storyFilters = args;
  if (args.length > 0 && modes.includes(args[0])) {
    mode = args[0];
    storyFilters = args.slice(1);
  }
  if (storyFilters.some((term) => term.startsWith("-"))) {
    console.error("Usage: studio-story-pngs.mjs [pngs|baselines|check] [story-filter...]");
    process.exit(2);
  }
  // Baselines are always the full story set: the committed set is replaced
  // wholesale (replaceBaselineImages), and canonical captures come from CI —
  // a partial local regeneration would silently delete every other baseline.
  if (mode === "baselines" && storyFilters.length > 0) {
    console.error(
      "Story filters are not supported for baselines: the committed set is always " +
        "regenerated in full (and canonically on CI). Use `pngs` or `check` with filters.",
    );
    process.exit(2);
  }
  return { mode, storyFilters };
}

// Case-insensitive substring OR-match over story ids. An empty filter list
// keeps every story.
function filterStoryIds(storyIds, filters) {
  if (filters.length === 0) {
    return storyIds;
  }
  const needles = filters.map((term) => term.toLowerCase());
  const matched = storyIds.filter((storyId) => {
    const haystack = storyId.toLowerCase();
    return needles.some((needle) => haystack.includes(needle));
  });
  if (matched.length === 0) {
    console.error(
      `No stories match filter(s): ${filters.join(", ")} (${storyIds.length} stories discovered).`,
    );
    process.exit(2);
  }
  console.log(`Filter matched ${matched.length}/${storyIds.length} stories.`);
  return matched;
}

// Ask the OS for a free TCP port by binding to 0, then release it and hand the
// number back to the static server. The window between close and re-bind is
// sub-millisecond, so a clash is astronomically less likely than the old fixed
// port; if it ever does, the server fails fast and the run can be retried.
function findFreePort() {
  return new Promise((resolve, reject) => {
    const probe = createServer();
    probe.once("error", reject);
    probe.listen(0, "127.0.0.1", () => {
      const { port: assigned } = probe.address();
      probe.close((closeError) => (closeError ? reject(closeError) : resolve(assigned)));
    });
  });
}

// Fingerprint the built artifact so resume only reuses captures from the exact
// same build. dx content-hashes the bundle filenames into index.html and emits
// the app wasm as assets/<name>-<hash>.wasm, so hashing both pins the rendered
// output. Returns null when nothing is found, which disables resume (a clean
// capture every time).
async function computeBuildFingerprint() {
  const hash = createHash("sha256");
  let found = false;
  try {
    hash.update(await readFile(path.join(publicDir, "index.html")));
    found = true;
  } catch {
    // No index.html — fall through to the wasm.
  }
  try {
    const assetsDir = path.join(publicDir, "assets");
    for (const name of (await readdir(assetsDir)).sort()) {
      if (name.endsWith(".wasm")) {
        hash.update(await readFile(path.join(assetsDir, name)));
        found = true;
      }
    }
  } catch {
    // No assets dir — the index.html hash alone still fingerprints the build.
  }
  return found ? hash.digest("hex") : null;
}

async function readCaptureBuildId(dir) {
  try {
    return (await readFile(path.join(dir, CAPTURE_BUILD_FILE), "utf8")).trim();
  } catch {
    return null;
  }
}

async function isNonEmptyFile(filePath) {
  try {
    return (await stat(filePath)).size > 0;
  } catch {
    return false;
  }
}

function outputDirForMode(currentMode) {
  if (currentMode === "baselines") {
    return baselineDirFromEnv();
  }
  if (currentMode === "check") {
    return (
      process.env.STUDIO_STORY_NEW_DIR ??
      process.env.STUDIO_STORY_PNGS_DIR ??
      "lp-app/lpa-studio-web/story-images/.new"
    );
  }
  return (
    process.env.STUDIO_STORY_SCRATCH_DIR ??
    process.env.STUDIO_STORY_PNGS_DIR ??
    "lp-app/lpa-studio-web/story-images/.scratch"
  );
}

function baselineDirFromEnv() {
  return (
    process.env.STUDIO_STORY_IMAGES_DIR ??
    process.env.STUDIO_STORY_BASELINES_DIR ??
    "lp-app/lpa-studio-web/story-images"
  );
}

function parseCaptureConcurrency() {
  // Each worker drives its own Chrome page; captures are independent, so this
  // scales the (dominant) capture phase near-linearly. Kept modest by default to
  // bound memory on CI runners; override with STUDIO_STORY_PNGS_CONCURRENCY.
  return parsePositiveIntegerEnv("STUDIO_STORY_PNGS_CONCURRENCY", 4);
}

function parseRatioEnv(name, defaultValue) {
  const value = process.env[name];
  if (value === undefined) {
    return defaultValue;
  }
  const parsed = Number.parseFloat(value);
  if (!Number.isFinite(parsed) || parsed < 0 || parsed > 1) {
    console.error(`${name} must be a number between 0 and 1.`);
    process.exit(2);
  }
  return parsed;
}

function parsePositiveIntegerEnv(name, defaultValue) {
  const value = process.env[name] ?? defaultValue.toString();
  const parsed = Number.parseInt(value, 10);
  if (!Number.isSafeInteger(parsed) || parsed < 1 || parsed.toString() !== value) {
    console.error(`${name} must be a positive integer.`);
    process.exit(2);
  }
  return parsed;
}

// Discovery is one bounded Chrome run, retried: on 2026-08-05 this exact
// invocation hung twice in CI (see `subprocessTimeoutMs`). Each attempt now
// dies at `discoveryTimeoutMs` and the next one gets a fresh browser, so a
// transient hang costs a minute instead of the whole job — and a persistent
// one fails loudly with a message naming the phase.
async function discoverStoryIds() {
  let lastError;
  for (let attempt = 1; attempt <= discoveryAttempts; attempt += 1) {
    try {
      return await discoverStoryIdsOnce();
    } catch (error) {
      lastError = error;
      if (attempt < discoveryAttempts) {
        console.warn(
          `Story discovery attempt ${attempt}/${discoveryAttempts} failed: ${error.message}\n` +
            "Retrying with a fresh Chrome...",
        );
      }
    }
  }
  throw lastError;
}

async function discoverStoryIdsOnce() {
  const html = await runChrome(
    [
      "--headless=new",
      "--disable-gpu",
      "--disable-application-cache",
      "--disk-cache-size=0",
      "--virtual-time-budget=5000",
      "--dump-dom",
      `${baseUrl}?story-discovery=${Date.now()}#/stories`,
    ],
    { timeoutMs: discoveryTimeoutMs, label: "story-discovery Chrome (--dump-dom)" },
  );
  const storyIds = [];
  // Story links became real paths when the router moved off the hash
  // (P09); the legacy `#/stories/…` form is still accepted so this
  // scraper works against either build during a bisect.
  for (const anchor of html.matchAll(/<a\b[^>]*href="(?:#\/|\/)stories\/([^"]+)"[^>]*>/g)) {
    const storyId = decodeURIComponent(anchor[1]).split(/[?#]/, 1)[0];
    // `#[story(screenshot)]` rides the discovery link (see story_book.rs).
    if (/data-story-screenshot="1"/.test(anchor[0])) {
      SCREENSHOT_STORY_IDS.add(storyId);
    }
    storyIds.push(storyId);
  }
  return storyIds
    .filter((value, index, values) => values.indexOf(value) === index)
    // Generated `overview` composites are NOT pixel baselines. They stack a
    // whole component's stories on one page — 10k-25k px tall against a 760px
    // viewport, where every non-composite story fits in 3400 — and
    // `captureBeyondViewport` does not reliably paint composited effects that
    // far below the fold: in the flip that filed
    // docs/defects/2026-07-28-overview-composite-capture-races.md, the device
    // card's `backdrop-filter` overlays kept their blur but lost their own
    // background and children, at every overlay story in the page and nowhere
    // above y=5658. Both terminals survive the stable pair, so each capture
    // committed a coin flip and every auto-refresh commit retriggered CI.
    // They also carried no coverage the per-story captures don't: every state
    // in a composite is captured on its own page too. Browse them in the story
    // book; do not baseline them.
    .filter((storyId) => !storyId.endsWith(OVERVIEW_COMPOSITE_SUFFIX))
    .sort();
}

// Run the capture pass, retrying with a fresh Chrome when it fails. Completed
// captures persist on disk and are skipped by the next attempt, so a retry only
// redoes the targets the failed pass didn't finish.
async function captureStoriesWithRetry(storyIds, directory) {
  let lastError;
  for (let attempt = 1; attempt <= captureAttempts; attempt += 1) {
    try {
      return await captureStories(storyIds, directory);
    } catch (error) {
      lastError = error;
      if (attempt < captureAttempts) {
        console.warn(
          `Capture pass ${attempt}/${captureAttempts} failed: ${error.message}\n` +
            "Retrying with a fresh Chrome — already-captured viewports are kept.",
        );
      }
    }
  }
  throw lastError;
}

async function captureStories(storyIds, directory) {
  const targets = storyTargets(storyIds);
  const files = new Array(targets.length);

  // Resume: viewports already captured for this build are kept as-is (the
  // capture dir is wiped at startup whenever the build changed). Filtering
  // them out here keeps the browser-restart budget below proportional to
  // real capture work, so resumed runs don't pay restarts for skipped files.
  const pending = [];
  for (let targetIndex = 0; targetIndex < targets.length; targetIndex += 1) {
    const target = targets[targetIndex];
    const file = path.join(directory, storyFileName(target.storyId, target.viewport));
    if (await isNonEmptyFile(file)) {
      files[targetIndex] = file;
    } else {
      pending.push({ target, targetIndex });
    }
  }
  if (pending.length === 0) {
    return files;
  }

  const concurrency = Math.min(requestedCaptureConcurrency, pending.length);
  progress.total = pending.length;
  progress.captured = 0;
  console.log(
    `Capturing ${pending.length}/${targets.length} story viewports (${storyIds.length} stories, up to ${STORY_VIEWPORTS.length} sizes each) with ${concurrency} Chrome pages...`,
  );

  // Defense in depth: capture in chunks with a fresh browser per chunk
  // (~1-2s each, resume from disk). The historical navigation wedges were
  // ultimately the static server's fault (see createStaticServer), but a
  // bounded browser lifetime keeps any future renderer-side degradation
  // from ever wedging a whole run. See docs/debt/story-capture-pipeline.md.
  for (let start = 0; start < pending.length; start += browserRestartEvery) {
    const chunk = pending.slice(start, start + browserRestartEvery);
    const chunkConcurrency = Math.min(concurrency, chunk.length);
    if (start > 0) {
      console.log(
        `Restarting capture browser (${start}/${pending.length} captured this pass)...`,
      );
    }
    const browser = await launchCaptureBrowser(chunkConcurrency);
    let nextChunkIndex = 0;
    try {
      await Promise.all(
        Array.from({ length: chunkConcurrency }, (_, pageIndex) =>
          captureStoryWorker({
            browser,
            directory,
            files,
            nextPending: () => chunk[nextChunkIndex++],
            pageIndex,
          }),
        ),
      );
    } finally {
      await browser.close();
    }
  }
  return files;
}

async function captureStoryWorker({ browser, directory, files, nextPending, pageIndex }) {
  while (true) {
    const entry = nextPending();
    if (entry === undefined) {
      return;
    }

    const { target, targetIndex } = entry;
    const file = path.join(directory, storyFileName(target.storyId, target.viewport));
    // Already-captured check stays as a safety even though captureStories
    // pre-filters: the in-process retry pass re-enters with fresh pending
    // lists built from the same on-disk state.
    if (await isNonEmptyFile(file)) {
      files[targetIndex] = file;
      progress.captured += 1;
      continue;
    }
    const url = storyPngUrl(target.storyId, target.viewport);
    try {
      await browser.capture(pageIndex, url, target.storyId, target.viewport, file);
    } catch (error) {
      // A wedged or crashed renderer poisons its page; swap in a fresh one and
      // retry the target once before letting the failure propagate.
      console.warn(
        `retrying ${target.storyId} (${target.viewport.id}) on a fresh page: ${error.message}`,
      );
      await browser.recycle(pageIndex);
      await browser.capture(pageIndex, url, target.storyId, target.viewport, file);
    }
    progress.captured += 1;
    console.log(`wrote ${path.relative(repoRoot, file)}`);
    files[targetIndex] = file;
  }
}

async function launchCaptureBrowser(pageCount) {
  const userDataDir = await mkdtemp(path.join(tmpdir(), "lp-studio-story-chrome-"));
  const child = spawn(
    chrome,
    [
      "--headless=new",
      "--disable-gpu",
      // Shared-memory transport can exhaust /dev/shm on Linux CI runners;
      // falling back to /tmp is the standard headless-CI setting and is
      // harmless elsewhere.
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
  const childExited = once(child, "exit").catch(() => {});
  const wsUrl = await waitForDevTools(child);
  const cdp = await CdpConnection.open(wsUrl);
  const pages = await Promise.all(
    Array.from({ length: pageCount }, () => createCapturePage(cdp)),
  );

  return {
    async capture(pageIndex, url, storyId, viewport, file) {
      await pages[pageIndex].capture(url, storyId, viewport, file);
    },

    // Replace a poisoned page (wedged/crashed renderer) with a fresh target so
    // one bad story can't take down the rest of the run.
    async recycle(pageIndex) {
      try {
        await pages[pageIndex].close();
      } catch {
        // The old target may already be gone; a fresh one is all we need.
      }
      pages[pageIndex] = await createCapturePage(cdp);
    },

    async close() {
      try {
        await cdp.send("Browser.close");
      } catch {
        cdp.close();
      }
      if (child.exitCode === null) {
        child.kill("SIGTERM");
      }
      await Promise.race([childExited, delay(1_000)]);
      // Chrome may still be flushing profile writes when it goes down (the
      // 1s exit race above can elapse first), so an eager recursive removal
      // races those writes — CI died with ENOTEMPTY here AFTER every PNG was
      // captured. Retry briefly, and never let cleanup fail the run: a
      // leftover temp profile dir is harmless.
      try {
        await rm(userDataDir, {
          recursive: true,
          force: true,
          maxRetries: 5,
          retryDelay: 200,
        });
      } catch (error) {
        console.warn(`leaving Chrome profile dir ${userDataDir}: ${error.message}`);
      }
    },
  };
}

async function createCapturePage(cdp) {
  const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await cdp.send("Target.attachToTarget", {
    targetId,
    flatten: true,
  });
  await cdp.send("Page.enable", {}, sessionId);
  await cdp.send("Runtime.enable", {}, sessionId);
  // CSS transitions/animations race the capture and land at a different phase
  // each run, so freeze them before the app mounts. Captures always show the
  // settled end state.
  await cdp.send(
    "Page.addScriptToEvaluateOnNewDocument",
    {
      source: `
        document.addEventListener("DOMContentLoaded", () => {
          const style = document.createElement("style");
          style.textContent =
            "*, *::before, *::after {" +
            " transition: none !important;" +
            " animation: none !important;" +
            " caret-color: transparent !important;" +
            " scroll-behavior: auto !important;" +
            " }";
          document.head.appendChild(style);
        });
      `,
    },
    sessionId,
  );

  return {
    async capture(url, storyId, viewport, file) {
      await cdp.send(
        "Emulation.setDeviceMetricsOverride",
        {
          width: viewport.width,
          height: viewport.height,
          deviceScaleFactor: 1,
          // Never emulate a mobile device, even at phone widths: sm means
          // "narrow desktop window", and Chrome's mobile emulation state leaks
          // asymmetrically across captures on a reused page (a later md/lg
          // capture renders number-input text shifted), which made runs
          // nondeterministic. With mobile off, captures are byte-identical.
          mobile: false,
        },
        sessionId,
      );
      await cdp.send("Page.navigate", { url }, sessionId);
      await waitForCaptureBox(cdp, sessionId, storyId);
      await waitForStoryReady(cdp, sessionId, storyId);
      // <select> widgets paint their label at first layout and do not reliably
      // repaint when a webfont lands afterwards, so a select that painted
      // before its font decoded keeps fallback-metric text for the life of the
      // page — bistable run-to-run depending on who won the race (the last
      // churner standing after the font gate above: subpixel-shifted select
      // text, ~Δ100). Force each select through a display toggle after fonts
      // are ready so its label re-renders with the real font.
      await evaluate(
        cdp,
        sessionId,
        `
        (() => {
          document.querySelectorAll('select').forEach((s) => {
            const d = s.style.display;
            s.style.display = 'none';
            void s.offsetWidth;
            s.style.display = d;
          });
          return true;
        })()
      `,
      );
      await fitViewportToStory(cdp, sessionId, viewport, storyId);
      await settleFocus(cdp, sessionId, storyId);
      const box = await waitForCaptureBox(cdp, sessionId, storyId);
      const clip = captureClip(box);
      // Chromium silently drops `backdrop-filter` from beyond-viewport
      // captures — even when the clip is entirely on screen — so glass
      // surfaces bake into baselines without their blur (see
      // docs/defects/story-capture-drops-backdrop-filter.md). Ask for a
      // beyond-viewport capture only when the clip actually overflows the
      // viewport: `fitViewportToStory` restores the base height before the
      // shot, so tall stories still need it (flipping unconditionally
      // truncates them at the fold).
      const clipFitsViewport =
        clip.x + clip.width <= viewport.width && clip.y + clip.height <= viewport.height;
      const shoot = async () => {
        const { data } = await cdp.send(
          "Page.captureScreenshot",
          {
            format: "png",
            captureBeyondViewport: !clipFitsViewport,
            fromSurface: true,
            clip,
          },
          sessionId,
        );
        return Buffer.from(data, "base64");
      };
      // Stable-pair capture: accept only two consecutive identical shots.
      // The story-ready wait proves the page settled ENOUGH to paint, but
      // bistable late settling still churned a known story set — a font-swap
      // repaint landing after first paint (select text ghosting) and an
      // autofocus ring appearing between shots. Both converge to a terminal
      // state, so requiring shot N == shot N-1 captures that steady state
      // deterministically. Genuinely non-settling stories exhaust the
      // attempts; keep the last shot and warn so they surface as churn, not
      // as a capture failure.
      let shot = await shoot();
      let stable = false;
      for (let attempt = 0; attempt < 5; attempt++) {
        await new Promise((resolve) => setTimeout(resolve, 150));
        const next = await shoot();
        if (next.equals(shot)) {
          stable = true;
          break;
        }
        shot = next;
      }
      if (!stable) {
        console.warn(`unstable render (kept last shot): ${storyId} (${viewport.name})`);
      }
      await writeFile(file, shot);
    },

    async close() {
      await cdp.send("Target.closeTarget", { targetId });
    },
  };
}

async function waitForDevTools(child) {
  return new Promise((resolve, reject) => {
    let stderr = "";
    const timeout = setTimeout(() => {
      cleanup();
      reject(new Error(`Timed out waiting for Chrome DevTools. ${stderr.trim()}`));
    }, 10_000);

    const onData = (chunk) => {
      stderr += chunk;
      const match = stderr.match(/DevTools listening on (ws:\/\/[^\s]+)/);
      if (match) {
        cleanup();
        resolve(match[1]);
      }
    };
    const onExit = (code) => {
      cleanup();
      reject(new Error(`Chrome exited before DevTools started (${code}). ${stderr.trim()}`));
    };
    const onError = (error) => {
      cleanup();
      reject(error);
    };
    const cleanup = () => {
      clearTimeout(timeout);
      child.stderr.off("data", onData);
      child.off("exit", onExit);
      child.off("error", onError);
    };

    child.stderr.on("data", onData);
    child.once("exit", onExit);
    child.once("error", onError);
  });
}

async function waitForCaptureBox(cdp, sessionId, storyId) {
  const expression = `
    (() => {
      const el = document.querySelector('[data-story-capture="1"]');
      if (!el || el.getAttribute('data-story-id') !== ${JSON.stringify(storyId)}) {
        return null;
      }
      const rect = el.getBoundingClientRect();
      if (rect.width < 1 || rect.height < 1) {
        return null;
      }
      return {
        x: rect.x,
        y: rect.y,
        width: rect.width,
        height: rect.height
      };
    })()
  `;
  const started = Date.now();
  while (Date.now() - started < captureTimeoutMs) {
    const box = await evaluate(cdp, sessionId, expression);
    if (box) {
      return box;
    }
    await delay(100);
  }
  throw new Error(`Timed out waiting for story capture target: ${storyId}`);
}

// Stories that mount an [autofocus] element (e.g. the device-card name sheet)
// flickered run-to-run: focus lands a beat after first paint, autofocus
// scrolls the element into view (shifting the capture box), and whether the
// focus ring survives later re-renders is itself racy — so neither "ring" nor
// "no ring" is a stable terminal state of the page. Make captures
// deterministic at this layer: wait for the autofocus candidate to take focus
// (so a late-landing focus can't race the blur), then blur it and reset
// scroll. Baselines therefore always show the unfocused state. Bounded and
// non-fatal: an autofocus element that can never take focus (hidden in a
// closed sheet) costs this story the timeout, not the run.
async function settleFocus(cdp, sessionId, storyId) {
  const focusedExpression = `
    (() => {
      const af = document.querySelector('[autofocus]');
      return !af || document.activeElement === af;
    })()
  `;
  const started = Date.now();
  let settled = false;
  while (Date.now() - started < 3_000) {
    if (await evaluate(cdp, sessionId, focusedExpression)) {
      settled = true;
      break;
    }
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
  if (!settled) {
    console.warn(`autofocus never settled (capturing anyway): ${storyId}`);
  }
  await evaluate(
    cdp,
    sessionId,
    `
    (() => {
      if (document.activeElement && document.activeElement !== document.body) {
        document.activeElement.blur();
      }
      window.scrollTo(0, 0);
      return true;
    })()
  `,
  );
}

// Grow the viewport so the whole story box is ON SCREEN before capturing.
//
// `captureBeyondViewport` photographs content that was never actually in view,
// and widgets that only measure themselves once they become visible get frozen
// in a pre-measurement state by that. CodeMirror is the case that forced this:
// `ViewState.measure()` consumes its "re-measure the content" flags and then
// returns early while the editor is outside the window, so an editor below the
// fold never measures and keeps the library default 14px line height while its
// content lays out at 18px — the line-number gutter then advances 14px against
// 18px lines and the numbers walk out of alignment with the code they label.
// Coming into view is the only thing that recovers it, and a story page does
// not scroll, so without this the state is permanent. Whether a given editor
// straddled the fold moved with a few pixels of layout, which is what flipped
// the baseline run to run.
//
// Widening the viewport instead of scrolling is what actually works here: the
// story page is not a scroll container, so `window.scrollTo` moves nothing.
// Width is never touched — sm/md/lg mean width, and the responsive layout must
// not change. The viewport is restored to its normal height before the capture
// (see below), so the grown height is a measurement window, not the height any
// story is photographed at.
// See docs/defects/2026-07-27-code-editor-gutter-misaligned.md
async function fitViewportToStory(cdp, sessionId, viewport, storyId) {
  const needed = await evaluate(
    cdp,
    sessionId,
    `
    (() => {
      const el = document.querySelector('[data-story-capture="1"]');
      if (!el) return null;
      const rect = el.getBoundingClientRect();
      const doc = document.documentElement;
      // Everything the capture could photograph: the story box in page
      // coordinates, plus whatever else the document reports.
      return Math.ceil(Math.max(
        rect.bottom + window.scrollY,
        doc.scrollHeight,
        document.body ? document.body.scrollHeight : 0,
      ));
    })()
  `,
  );
  if (!needed) {
    return;
  }
  // Cap it: a runaway story must not turn into a gigantic surface allocation.
  // Anything past the cap keeps the old below-the-fold behaviour rather than
  // failing the capture.
  const height = Math.min(Math.max(needed, viewport.height), storyViewportMaxHeight);
  if (height <= viewport.height) {
    return;
  }
  await cdp.send(
    "Emulation.setDeviceMetricsOverride",
    { width: viewport.width, height, deviceScaleFactor: 1, mobile: false },
    sessionId,
  );
  // Measurement runs on requestAnimationFrame and a headless page stops
  // producing frames on its own, so force one and let the story settle at the
  // new height before anything reads geometry again.
  await forceFrame(cdp, sessionId);
  await waitForStoryReady(cdp, sessionId, storyId);
  // Then put the viewport back. The point of growing it was to let widgets take
  // a measurement they refuse to take off screen, and those measurements stick
  // — CodeMirror's height oracle keeps the corrected line height once it has
  // one. Capturing at the grown height would instead bake the taller viewport
  // into every tall story: layout that keys on viewport height (a pane sized to
  // fill the window) expands, and the story box grows by that much empty space.
  // Measured on studio-shell/simulator-ready at sm, the grown capture was 91px
  // taller with pixel-identical content — a baseline change that carries no
  // information. Restoring first keeps the fix and leaves those baselines alone.
  await cdp.send(
    "Emulation.setDeviceMetricsOverride",
    {
      width: viewport.width,
      height: viewport.height,
      deviceScaleFactor: 1,
      mobile: false,
    },
    sessionId,
  );
  await forceFrame(cdp, sessionId);
  await waitForStoryReady(cdp, sessionId, storyId);
}

// A discarded 1x1 screenshot forces a BeginFrame, so rAF-driven work (layout
// measurement, popover positioning) can make progress in a headless page.
async function forceFrame(cdp, sessionId) {
  await cdp
    .send(
      "Page.captureScreenshot",
      { format: "png", fromSurface: true, clip: { x: 0, y: 0, width: 1, height: 1, scale: 1 } },
      sessionId,
    )
    .catch(() => {});
}

async function waitForStoryReady(cdp, sessionId, storyId) {
  const expression = `
    (() => {
      const el = document.querySelector('[data-story-capture="1"]');
      if (!el || el.getAttribute('data-story-id') !== ${JSON.stringify(storyId)}) {
        return false;
      }
      // Webfont fallback metrics shift text; a capture that races font
      // decoding diffs nondeterministically run-to-run — from scattered text
      // pixels up to whole-page layout shifts when line wrapping changes.
      // document.fonts.status alone is a trap: it reports 'loaded' whenever
      // no loads are PENDING, which is trivially true before the first
      // element requests a face. Demand every bundled face explicitly,
      // kicking off its load so the check converges even for faces the
      // current story hasn't touched yet.
      if (document.fonts) {
        const faces = [
          '400 1em Inter', '500 1em Inter', '600 1em Inter',
          '700 1em Inter', '800 1em Inter',
          "400 1em 'JetBrains Mono'", "600 1em 'JetBrains Mono'",
          "700 1em 'JetBrains Mono'",
        ];
        const missing = faces.filter((f) => !document.fonts.check(f));
        if (missing.length > 0) {
          missing.forEach((f) => document.fonts.load(f));
          return false;
        }
        if (document.fonts.status !== 'loaded') {
          return false;
        }
      }
      // Preview canvases paint via putImageData from an async task after
      // mount, so "mounted but not yet painted" is a real page state. Both
      // painted and unpainted survive a stable pair, so without this gate a
      // baseline could freeze either one — bistable run-to-run. The app
      // stamps data-preview-painted on each canvas after its first blit;
      // demand it on every preview canvas in the story.
      const unpainted = el.querySelectorAll(
        'canvas.ux-produced-product-pixel-canvas:not([data-preview-painted]),' +
          'canvas.ux-produced-product-lamp-canvas:not([data-preview-painted])',
      );
      if (unpainted.length > 0) {
        return false;
      }
      // A canvas that sizes its backing store from layout (the clock face's
      // phasor traces) paints a bitmap for the box it measured. Measure that
      // box before the app's stylesheet lands and the bitmap is drawn for the
      // unstyled 300x150 intrinsic size, then squeezed into the real box —
      // a second stable terminal that alternated baselines run to run
      // (docs/defects/2026-08-05-clock-face-baselines-oscillate.md). The app
      // repaints on resize; this refuses to shoot until it has.
      const dpr = window.devicePixelRatio || 1;
      const mismatched = [...el.querySelectorAll('canvas.ux-box-sized-canvas')].filter((c) => {
        const rect = c.getBoundingClientRect();
        return (
          c.width !== Math.max(1, Math.round(rect.width * dpr)) ||
          c.height !== Math.max(1, Math.round(rect.height * dpr))
        );
      });
      if (mismatched.length > 0) {
        return false;
      }
      // The mapping canvas fits its camera to a measured viewport, and the
      // first measurement races container layout settling — a fit frozen on
      // a pre-settle size renders the same story at a different zoom run to
      // run (workbench-mapping-view oscillated 82% vs 157%). The app now
      // re-fits until the measurement settles and stamps data-fit-viewport
      // with the size the camera was last reconciled against; refuse to
      // shoot while any visible canvas's real box disagrees ("" = measured
      // but never reconciled). Hidden mounts (the mobile fold's replaced
      // center) are exempt — they have no box to disagree with.
      const unreconciled = [...el.querySelectorAll('[data-fit-viewport]')].filter((wrap) => {
        const rect = wrap.getBoundingClientRect();
        if (rect.width < 1 || rect.height < 1) {
          return false;
        }
        const svg = wrap.querySelector('svg');
        if (!svg) {
          return false;
        }
        const box = svg.getBoundingClientRect();
        if (box.width < 1 || box.height < 1) {
          return false;
        }
        const stamp = wrap.getAttribute('data-fit-viewport');
        if (!stamp) {
          return true;
        }
        const [w, h] = stamp.split('x').map(Number);
        return Math.abs(w - box.width) > 2 || Math.abs(h - box.height) > 2;
      });
      if (unreconciled.length > 0) {
        return false;
      }
      return !document.querySelector('[data-story-wait="1"]');
    })()
  `;
  const started = Date.now();
  // Generous cap: readiness polls every 50ms so fast stories pay nothing,
  // and the forced-BeginFrame kick below unwedges rAF-driven stories — the
  // 30s ceiling is margin for release-WASM popover stories on a loaded
  // machine (they used to kill whole baseline runs at 10s).
  let lastForcedFrame = 0;
  while (Date.now() - started < 30_000) {
    const ready = await evaluate(cdp, sessionId, expression);
    if (ready) {
      return;
    }
    // Headless pages stop producing frames once the load-time BeginFrames are
    // spent, but the app flushes DOM updates (e.g. popover positioning) on
    // requestAnimationFrame — a story that finishes mounting after the last
    // organic frame would wait here forever. A discarded 1x1 screenshot
    // forces a BeginFrame so rAF-driven work can make progress; throttled
    // (only after the story dawdles, at most every 250ms) so parallel pages
    // don't wedge Chrome with screenshot traffic.
    const now = Date.now();
    if (now - started > 500 && now - lastForcedFrame > 250) {
      lastForcedFrame = now;
      await cdp
        .send(
          "Page.captureScreenshot",
          {
            format: "png",
            fromSurface: true,
            clip: { x: 0, y: 0, width: 1, height: 1, scale: 1 },
          },
          sessionId,
        )
        .catch(() => {});
    }
    await delay(50);
  }
  throw new Error(`Timed out waiting for story ready state: ${storyId}`);
}

async function evaluate(cdp, sessionId, expression) {
  const response = await cdp.send(
    "Runtime.evaluate",
    {
      expression,
      awaitPromise: true,
      returnByValue: true,
    },
    sessionId,
  );
  if (response.exceptionDetails) {
    throw new Error(`Chrome evaluation failed: ${JSON.stringify(response.exceptionDetails)}`);
  }
  return response.result.value;
}

function captureClip(box) {
  const x = Math.max(0, Math.floor(box.x));
  const y = Math.max(0, Math.floor(box.y));
  return {
    x,
    y,
    width: Math.ceil(box.width + box.x - x),
    height: Math.ceil(box.height + box.y - y),
    scale: 1,
  };
}

async function optimizePngs(files, { required }) {
  const oxipng = findCommand("oxipng");
  if (!oxipng) {
    if (required) {
      throw new Error(
        "oxipng is required for story baselines and checks. Install with `cargo install oxipng` or `brew install oxipng`.",
      );
    }
    console.warn("oxipng not found; PNGs were not losslessly optimized.");
    return;
  }
  await runProcess(oxipng, ["-o", "2", "--strip", "safe", ...files]);
}

async function compareBaselines(storyIds, expectedDir, actualDir) {
  const targets = storyTargets(storyIds);
  const expectedFiles = new Set(
    targets.map((target) => storyFileName(target.storyId, target.viewport)),
  );
  // A filtered run only captures a partial target set, so baselines outside it
  // can't be judged as removed stories — skip the removed-story scan entirely.
  const baselineFiles = storyFilters.length > 0 ? [] : await listPngFiles(expectedDir);
  const unexpected = baselineFiles.filter((file) => !expectedFiles.has(file));
  const missing = [];
  const changed = [];
  const tolerated = [];
  // File names (not the annotated display strings) for the refresh manifest.
  const replace = [];
  let identical = 0;

  for (const target of targets) {
    const fileName = storyFileName(target.storyId, target.viewport);
    const expectedFile = path.join(expectedDir, fileName);
    const actualFile = path.join(actualDir, fileName);
    const expected = await readOptionalFile(expectedFile);
    const actual = await readFile(actualFile);

    if (!expected) {
      missing.push(fileName);
      replace.push(fileName);
    } else if (expected.equals(actual)) {
      identical += 1;
    } else {
      const diff = comparePngPixels(expected, actual);
      if (diff.withinTolerance) {
        // Deliberately NOT added to `replace`: the committed bytes stay put,
        // so sub-threshold jitter can never ping-pong the baseline.
        tolerated.push({ fileName, diff });
      } else {
        changed.push(`${fileName} (${diff.summary})`);
        replace.push(fileName);
      }
    }
  }

  printComparison("changed", changed);
  printComparison("new", missing);
  printComparison("removed", unexpected);
  printComparison(
    "within tolerance (informational)",
    tolerated.map(({ fileName, diff }) => `${fileName} (${diff.summary})`),
  );

  // Amplitude heuristic (warn-only): every benign raster-churner class ever
  // observed on this pipeline (compositor layer promotion, border AA — the
  // version-badge/shader-face family) diffs at 0 significant pixels; a
  // tolerated file with significant pixels squeaked UNDER the ratio limit
  // while containing per-channel deltas the significance test itself calls
  // real. That is the fingerprint of a bistable render or a content change
  // hiding under the count-only gate — see
  // docs/defects/2026-07-27-story-check-tolerance-ignores-amplitude.md.
  // Warn loudly; do not fail (single-story calibration so far — promote to a
  // gate once a few real runs have confirmed where the boundary sits).
  const suspects = tolerated.filter(({ diff }) => diff.significantPixels > 0);
  if (suspects.length > 0) {
    console.warn(
      `\nWARNING: ${suspects.length} tolerated file(s) contain significant pixels ` +
        `(Δ>${significanceDelta}) — under the ratio limit but NOT raster jitter, ` +
        "which always diffs at 0 significant pixels. Suspected bistable render " +
        "or under-the-ratio content change:",
    );
    for (const { fileName, diff } of suspects) {
      console.warn(`  ${fileName} (max Δ${diff.maxDelta}, ${diff.significantPixels} significant)`);
    }
    console.warn(
      "  Fresh + baseline pixels are retained as the story-images-tolerated CI artifact.\n" +
        "  See docs/defects/2026-07-27-story-check-tolerance-ignores-amplitude.md.",
    );
  }

  const manifest = {
    replace,
    remove: unexpected,
    // Names only (consumers replace/remove by name; tolerated is evidence
    // retention, applied by the workflow's artifact step, never by
    // story-apply-refresh).
    tolerated: tolerated.map(({ fileName }) => fileName),
  };
  if (changed.length === 0 && missing.length === 0 && unexpected.length === 0) {
    console.log(
      `Story baselines match (${identical} byte-identical, ${tolerated.length} within tolerance).`,
    );
    return { ok: true, ...manifest };
  }
  console.log(`Fresh PNGs: ${path.relative(repoRoot, actualDir)}`);
  return { ok: false, ...manifest };
}

function comparePngPixels(expected, actual) {
  let expectedImage;
  let actualImage;
  try {
    expectedImage = decodePng(expected);
    actualImage = decodePng(actual);
  } catch (error) {
    return {
      withinTolerance: false,
      summary: `bytes differ, pixel compare unavailable: ${error.message}`,
    };
  }

  if (
    expectedImage.width !== actualImage.width ||
    expectedImage.height !== actualImage.height
  ) {
    return {
      withinTolerance: false,
      summary:
        `dimensions ${expectedImage.width}x${expectedImage.height}` +
        ` -> ${actualImage.width}x${actualImage.height}`,
    };
  }

  let diffPixels = 0;
  let significantPixels = 0;
  let maxDelta = 0;
  for (let i = 0; i < expectedImage.rgba.length; i += 4) {
    let pixelDelta = 0;
    for (let channel = 0; channel < 4; channel += 1) {
      const delta = Math.abs(expectedImage.rgba[i + channel] - actualImage.rgba[i + channel]);
      if (delta > pixelDelta) {
        pixelDelta = delta;
      }
    }
    if (pixelDelta > 0) {
      diffPixels += 1;
      if (pixelDelta > maxDelta) {
        maxDelta = pixelDelta;
      }
      if (pixelDelta > significanceDelta) {
        significantPixels += 1;
      }
    }
  }

  const totalPixels = expectedImage.width * expectedImage.height;
  const significantRatio = significantPixels / totalPixels;
  return {
    withinTolerance: significantRatio <= maxSignificantPixelRatio,
    significantPixels,
    maxDelta,
    summary:
      `${significantPixels}/${totalPixels} px (${(significantRatio * 100).toFixed(3)}%)` +
      ` exceed Δ${significanceDelta} [${diffPixels} any-diff, max Δ${maxDelta}]`,
  };
}

function decodePng(buffer) {
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  if (buffer.length < 8 || !buffer.subarray(0, 8).equals(signature)) {
    throw new Error("not a PNG file");
  }

  let ihdr = null;
  let palette = null;
  let transparency = null;
  const idat = [];
  let offset = 8;
  while (offset + 8 <= buffer.length) {
    const length = buffer.readUInt32BE(offset);
    const type = buffer.toString("latin1", offset + 4, offset + 8);
    const data = buffer.subarray(offset + 8, offset + 8 + length);
    if (type === "IHDR") {
      ihdr = {
        width: data.readUInt32BE(0),
        height: data.readUInt32BE(4),
        bitDepth: data[8],
        colorType: data[9],
        interlace: data[12],
      };
    } else if (type === "PLTE") {
      palette = data;
    } else if (type === "tRNS") {
      transparency = data;
    } else if (type === "IDAT") {
      idat.push(data);
    } else if (type === "IEND") {
      break;
    }
    offset += 12 + length;
  }

  if (!ihdr || idat.length === 0) {
    throw new Error("missing IHDR or IDAT chunk");
  }
  if (ihdr.interlace !== 0) {
    throw new Error("interlaced PNG is not supported");
  }
  const channelCounts = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 };
  const channels = channelCounts[ihdr.colorType];
  if (!channels || ![1, 2, 4, 8, 16].includes(ihdr.bitDepth)) {
    throw new Error(`unsupported color type ${ihdr.colorType} / bit depth ${ihdr.bitDepth}`);
  }

  const raw = inflateSync(Buffer.concat(idat));
  const rowBytes = Math.ceil((ihdr.width * channels * ihdr.bitDepth) / 8);
  if (raw.length < (rowBytes + 1) * ihdr.height) {
    throw new Error("truncated image data");
  }
  const filterStep = Math.max(1, Math.ceil((channels * ihdr.bitDepth) / 8));
  const scanlines = unfilterScanlines(raw, ihdr.height, rowBytes, filterStep);
  return {
    width: ihdr.width,
    height: ihdr.height,
    rgba: scanlinesToRgba(ihdr, scanlines, rowBytes, palette, transparency),
  };
}

function unfilterScanlines(raw, height, rowBytes, filterStep) {
  const out = Buffer.alloc(rowBytes * height);
  for (let y = 0; y < height; y += 1) {
    const filter = raw[y * (rowBytes + 1)];
    const src = raw.subarray(y * (rowBytes + 1) + 1, (y + 1) * (rowBytes + 1));
    const row = out.subarray(y * rowBytes, (y + 1) * rowBytes);
    const prev = y > 0 ? out.subarray((y - 1) * rowBytes, y * rowBytes) : null;
    for (let x = 0; x < rowBytes; x += 1) {
      const left = x >= filterStep ? row[x - filterStep] : 0;
      const up = prev ? prev[x] : 0;
      const upLeft = prev && x >= filterStep ? prev[x - filterStep] : 0;
      let value = src[x];
      if (filter === 1) {
        value += left;
      } else if (filter === 2) {
        value += up;
      } else if (filter === 3) {
        value += (left + up) >> 1;
      } else if (filter === 4) {
        value += paethPredictor(left, up, upLeft);
      } else if (filter !== 0) {
        throw new Error(`unsupported scanline filter ${filter}`);
      }
      row[x] = value & 0xff;
    }
  }
  return out;
}

function paethPredictor(a, b, c) {
  const p = a + b - c;
  const pa = Math.abs(p - a);
  const pb = Math.abs(p - b);
  const pc = Math.abs(p - c);
  if (pa <= pb && pa <= pc) {
    return a;
  }
  return pb <= pc ? b : c;
}

function scanlinesToRgba(ihdr, scanlines, rowBytes, palette, transparency) {
  const { width, height, bitDepth, colorType } = ihdr;
  const rgba = new Uint8Array(width * height * 4);
  // Samples are normalized to 8 bits: 16-bit samples keep their high byte,
  // sub-8-bit grayscale samples are rescaled to 0..255.
  const readSample = (row, index) => {
    if (bitDepth === 8) {
      return row[index];
    }
    if (bitDepth === 16) {
      return row[index * 2];
    }
    const bitOffset = index * bitDepth;
    return (row[bitOffset >> 3] >> (8 - bitDepth - (bitOffset & 7))) & ((1 << bitDepth) - 1);
  };
  const grayScale = bitDepth < 8 ? 255 / ((1 << bitDepth) - 1) : 1;
  const transparentGray =
    colorType === 0 && transparency?.length >= 2
      ? transparency.readUInt16BE(0) >> (bitDepth === 16 ? 8 : 0)
      : null;
  const transparentRgb =
    colorType === 2 && transparency?.length >= 6
      ? [0, 2, 4].map((i) => transparency.readUInt16BE(i) >> (bitDepth === 16 ? 8 : 0))
      : null;

  for (let y = 0; y < height; y += 1) {
    const row = scanlines.subarray(y * rowBytes, (y + 1) * rowBytes);
    for (let x = 0; x < width; x += 1) {
      const out = (y * width + x) * 4;
      let r;
      let g;
      let b;
      let a = 255;
      if (colorType === 0) {
        const sample = readSample(row, x);
        r = g = b = Math.round(sample * grayScale);
        if (sample === transparentGray) {
          a = 0;
        }
      } else if (colorType === 2) {
        r = readSample(row, x * 3);
        g = readSample(row, x * 3 + 1);
        b = readSample(row, x * 3 + 2);
        if (
          transparentRgb &&
          r === transparentRgb[0] &&
          g === transparentRgb[1] &&
          b === transparentRgb[2]
        ) {
          a = 0;
        }
      } else if (colorType === 3) {
        const index = readSample(row, x);
        if (!palette || index * 3 + 2 >= palette.length) {
          throw new Error(`palette index ${index} out of range`);
        }
        r = palette[index * 3];
        g = palette[index * 3 + 1];
        b = palette[index * 3 + 2];
        if (transparency && index < transparency.length) {
          a = transparency[index];
        }
      } else if (colorType === 4) {
        r = g = b = readSample(row, x * 2);
        a = readSample(row, x * 2 + 1);
      } else {
        r = readSample(row, x * 4);
        g = readSample(row, x * 4 + 1);
        b = readSample(row, x * 4 + 2);
        a = readSample(row, x * 4 + 3);
      }
      rgba[out] = r;
      rgba[out + 1] = g;
      rgba[out + 2] = b;
      rgba[out + 3] = a;
    }
  }
  return rgba;
}

async function listPngFiles(directory) {
  try {
    return (await readdir(directory)).filter((entry) => entry.endsWith(".png")).sort();
  } catch (error) {
    if (error.code === "ENOENT") {
      return [];
    }
    throw error;
  }
}

async function readOptionalFile(file) {
  try {
    return await readFile(file);
  } catch (error) {
    if (error.code === "ENOENT") {
      return null;
    }
    throw error;
  }
}

async function replaceBaselineImages(source, destination) {
  await mkdir(destination, { recursive: true });

  for (const fileName of await listPngFiles(destination)) {
    await unlink(path.join(destination, fileName));
  }

  for (const fileName of await listPngFiles(source)) {
    await rename(path.join(source, fileName), path.join(destination, fileName));
  }

  await rm(source, { recursive: true, force: true });
}

async function waitForServer(url) {
  const started = Date.now();
  while (Date.now() - started < 10_000) {
    try {
      const response = await fetch(url);
      if (response.ok) {
        return;
      }
    } catch {
      await delay(100);
    }
  }
  throw new Error(`Timed out waiting for ${url}`);
}

async function runChrome(args, options) {
  return await runProcess(
    chrome,
    ["--no-first-run", "--no-default-browser-check", ...args],
    options,
  );
}

// Wait for a child process, BOUNDED. The bare `once(child, "exit")` this
// replaced is what let a hung discovery Chrome burn 5 hours of CI in silence
// (docs/debt/story-capture-pipeline.md, 2026-08-05): nothing else in this file
// covers a child that never exits, because every other timeout guards the CDP
// capture path.
async function runProcess(command, args, { timeoutMs = subprocessTimeoutMs, label } = {}) {
  const name = label ?? path.basename(command);
  const child = spawn(command, args, { stdio: ["ignore", "pipe", "pipe"] });
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => {
    stdout += chunk;
  });
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });

  const exited = once(child, "exit");
  let timer;
  const timedOut = new Promise((resolve) => {
    timer = setTimeout(() => resolve(true), timeoutMs);
  });
  // Race rather than `child.kill` on a timer alone: the point is that this
  // function ALWAYS settles, even if the kill itself does not take.
  const outcome = await Promise.race([
    exited.then(([code]) => ({ code })),
    timedOut.then(() => ({ killed: true })),
  ]);
  clearTimeout(timer);

  if (outcome.killed) {
    // SIGKILL, not SIGTERM: a wedged headless Chrome has nothing to flush, and
    // a polite signal it ignores would just re-open the hole this closes.
    child.kill("SIGKILL");
    exited.catch(() => {});
    throw new Error(
      `${name} did not exit within ${timeoutMs} ms — killed.` +
        (stderr.trim() ? `\nstderr: ${stderr.trim()}` : ""),
    );
  }
  if (outcome.code !== 0) {
    throw new Error(`${name} exited with ${outcome.code}: ${stderr.trim()}`);
  }
  return stdout;
}

function printComparison(label, files) {
  if (files.length === 0) {
    return;
  }
  console.log(`${label}:`);
  for (const file of files) {
    console.log(`  ${file}`);
  }
}

function storyTargets(storyIds) {
  return storyIds.flatMap((storyId) =>
    viewportsFor(storyId).map((viewport) => ({ storyId, viewport })),
  );
}

function viewportsFor(storyId) {
  if (!SCREENSHOT_STORY_IDS.has(storyId)) {
    return STORY_VIEWPORTS;
  }
  return STORY_VIEWPORTS.filter(
    (viewport) => viewport.id === SCREENSHOT_VIEWPORT_ID,
  );
}

function storyFileName(storyId, viewport) {
  return `${storyId.replaceAll("/", "__")}__${viewport.id}.png`;
}

function storyPngUrl(storyId, viewport) {
  return `${baseUrl}?story-png=1&story=${encodeURIComponent(storyId)}&viewport=${viewport.id}#/stories/${storyId}`;
}

function findCommand(command) {
  const lookup = process.platform === "win32" ? "where" : "which";
  const result = spawnSync(lookup, [command], {
    encoding: "utf8",
  });
  if (result.status !== 0) {
    return null;
  }
  return result.stdout
    .split(/\r?\n/)
    .map((line) => line.trim())
    .find(Boolean) ?? null;
}

function findChrome() {
  if (process.platform === "darwin") {
    return "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome";
  }
  return "google-chrome";
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
