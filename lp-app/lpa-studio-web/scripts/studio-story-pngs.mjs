#!/usr/bin/env node

import {
  mkdir,
  mkdtemp,
  readdir,
  readFile,
  rename,
  rm,
  unlink,
  writeFile,
} from "node:fs/promises";
import { spawn, spawnSync } from "node:child_process";
import { once } from "node:events";
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
const mode = parseMode(process.argv.slice(2));
// Default to an OS-assigned free port so parallel runs (multiple agents, each in
// its own git worktree — which isolates files but NOT ports) never fight over a
// fixed port. Set STUDIO_STORY_PNGS_PORT to pin one (e.g. for debugging).
const port = process.env.STUDIO_STORY_PNGS_PORT ?? String(await findFreePort());
const requestedCaptureConcurrency = parseCaptureConcurrency();
const captureTimeoutMs = parsePositiveIntegerEnv("STUDIO_STORY_CAPTURE_TIMEOUT_MS", 10_000);
// Captures of the same build still differ in a few pixels from anti-aliasing and
// sub-pixel text layout jitter (high per-channel delta, but only along glyph edges).
// So `check` counts pixels whose per-channel delta exceeds a significance threshold
// and fails only when that count is more than a small fraction of the image —
// pixelmatch-style — rather than gating on the single worst pixel. This has a noise
// floor: changes below the ratio don't fail the check (reviewers still see the
// baseline image diff in the PR).
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

  send(method, params = {}, sessionId = undefined) {
    const id = this.nextId;
    this.nextId += 1;
    const message = { id, method, params };
    if (sessionId) {
      message.sessionId = sessionId;
    }

    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
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

await rm(captureDir, { recursive: true, force: true });
await mkdir(captureDir, { recursive: true });

const server = spawn("python3", ["-m", "http.server", port, "--bind", "127.0.0.1"], {
  cwd: publicDir,
  stdio: ["ignore", "pipe", "pipe"],
});
const serverExited = once(server, "exit").catch(() => {});
server.once("error", (error) => {
  console.error(`Failed to start static server from ${publicDir}: ${error.message}`);
});

try {
  await waitForServer(baseUrl);
  const storyIds = await discoverStoryIds();
  if (storyIds.length === 0) {
    throw new Error("No story links were discovered from the storybook page.");
  }

  const files = await captureStories(storyIds, captureDir);
  await optimizePngs(files, { required: mode !== "pngs" });

  if (mode === "baselines") {
    await replaceBaselineImages(captureDir, outputDir);
    console.log(`Story baselines: ${path.relative(repoRoot, outputDir)}`);
  } else if (mode === "check") {
    const ok = await compareBaselines(storyIds, baselineDir, outputDir);
    if (!ok) {
      console.error("\nStory baselines differ. Run `just studio-story-baselines` to update them.");
      process.exitCode = 1;
    }
  } else {
    console.log(`Story PNGs: ${path.relative(repoRoot, outputDir)}`);
  }
} finally {
  if (server.exitCode === null) {
    server.kill("SIGTERM");
  }
  await Promise.race([serverExited, delay(1_000)]);
}

function parseMode(args) {
  const value = args[0] ?? "pngs";
  if (["pngs", "baselines", "check"].includes(value)) {
    return value;
  }
  console.error("Usage: studio-story-pngs.mjs [pngs|baselines|check]");
  process.exit(2);
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
  return parsePositiveIntegerEnv("STUDIO_STORY_PNGS_CONCURRENCY", 1);
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

async function discoverStoryIds() {
  const html = await runChrome([
    "--headless=new",
    "--disable-gpu",
    "--disable-application-cache",
    "--disk-cache-size=0",
    "--virtual-time-budget=5000",
    "--dump-dom",
    `${baseUrl}?story-discovery=${Date.now()}#/stories`,
  ]);
  return Array.from(html.matchAll(/href="#\/stories\/([^"]+)"/g))
    .map((match) => decodeURIComponent(match[1]))
    .map((storyId) => storyId.split(/[?#]/, 1)[0])
    .filter((value, index, values) => values.indexOf(value) === index)
    .sort();
}

async function captureStories(storyIds, directory) {
  const targets = storyTargets(storyIds);
  const concurrency = Math.min(requestedCaptureConcurrency, targets.length);
  const files = new Array(targets.length);
  const browser = await launchCaptureBrowser(concurrency);
  let nextTargetIndex = 0;

  console.log(
    `Capturing ${targets.length} story viewports (${storyIds.length} stories x ${STORY_VIEWPORTS.length} sizes) with ${concurrency} Chrome pages...`,
  );

  try {
    await Promise.all(
      Array.from({ length: concurrency }, (_, pageIndex) =>
        captureStoryWorker({
          browser,
          directory,
          files,
          nextTargetIndex: () => nextTargetIndex++,
          pageIndex,
          targets,
        }),
      ),
    );
  } finally {
    await browser.close();
  }
  return files;
}

async function captureStoryWorker({
  browser,
  directory,
  files,
  nextTargetIndex,
  pageIndex,
  targets,
}) {
  while (true) {
    const targetIndex = nextTargetIndex();
    if (targetIndex >= targets.length) {
      return;
    }

    const target = targets[targetIndex];
    const file = path.join(directory, storyFileName(target.storyId, target.viewport));
    await browser.capture(
      pageIndex,
      storyPngUrl(target.storyId, target.viewport),
      target.storyId,
      target.viewport,
      file,
    );
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
      await rm(userDataDir, { recursive: true, force: true });
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
          mobile: viewport.width <= 640,
        },
        sessionId,
      );
      await cdp.send("Page.navigate", { url }, sessionId);
      await waitForCaptureBox(cdp, sessionId, storyId);
      await waitForStoryReady(cdp, sessionId, storyId);
      const box = await waitForCaptureBox(cdp, sessionId, storyId);
      const clip = captureClip(box);
      const { data } = await cdp.send(
        "Page.captureScreenshot",
        {
          format: "png",
          captureBeyondViewport: true,
          fromSurface: true,
          clip,
        },
        sessionId,
      );
      await writeFile(file, Buffer.from(data, "base64"));
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

async function waitForStoryReady(cdp, sessionId, storyId) {
  const expression = `
    (() => {
      const el = document.querySelector('[data-story-capture="1"]');
      if (!el || el.getAttribute('data-story-id') !== ${JSON.stringify(storyId)}) {
        return false;
      }
      return !document.querySelector('[data-story-wait="1"]');
    })()
  `;
  const started = Date.now();
  while (Date.now() - started < 10_000) {
    const ready = await evaluate(cdp, sessionId, expression);
    if (ready) {
      return;
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
  const baselineFiles = await listPngFiles(expectedDir);
  const unexpected = baselineFiles.filter((file) => !expectedFiles.has(file));
  const missing = [];
  const changed = [];
  const tolerated = [];
  let identical = 0;

  for (const target of targets) {
    const fileName = storyFileName(target.storyId, target.viewport);
    const expectedFile = path.join(expectedDir, fileName);
    const actualFile = path.join(actualDir, fileName);
    const expected = await readOptionalFile(expectedFile);
    const actual = await readFile(actualFile);

    if (!expected) {
      missing.push(fileName);
    } else if (expected.equals(actual)) {
      identical += 1;
    } else {
      const diff = comparePngPixels(expected, actual);
      if (diff.withinTolerance) {
        tolerated.push(`${fileName} (${diff.summary})`);
      } else {
        changed.push(`${fileName} (${diff.summary})`);
      }
    }
  }

  printComparison("changed", changed);
  printComparison("new", missing);
  printComparison("removed", unexpected);
  printComparison("within tolerance (informational)", tolerated);

  if (changed.length === 0 && missing.length === 0 && unexpected.length === 0) {
    console.log(
      `Story baselines match (${identical} byte-identical, ${tolerated.length} within tolerance).`,
    );
    return true;
  }
  console.log(`Fresh PNGs: ${path.relative(repoRoot, actualDir)}`);
  return false;
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

async function runChrome(args) {
  return await runProcess(chrome, [
    "--no-first-run",
    "--no-default-browser-check",
    ...args,
  ]);
}

async function runProcess(command, args) {
  const child = spawn(command, args, { stdio: ["ignore", "pipe", "pipe"] });
  let stdout = "";
  let stderr = "";
  child.stdout.on("data", (chunk) => {
    stdout += chunk;
  });
  child.stderr.on("data", (chunk) => {
    stderr += chunk;
  });
  const [code] = await once(child, "exit");
  if (code !== 0) {
    throw new Error(`${command} exited with ${code}: ${stderr.trim()}`);
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
    STORY_VIEWPORTS.map((viewport) => ({ storyId, viewport })),
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
