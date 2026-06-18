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
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import path from "node:path";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");
const publicDir = path.join(repoRoot, "lp-app/lp-studio-web/public");
const storyRoot = path.join(repoRoot, "lp-app/lp-studio-web");
const mode = parseMode(process.argv.slice(2));
const port = process.env.STUDIO_STORY_PNGS_PORT ?? "2822";
const baseUrl = `http://127.0.0.1:${port}/`;
const chrome = process.env.CHROME_BIN ?? findChrome();
const baselineDir = path.resolve(repoRoot, baselineDirFromEnv());
const outputDir = path.resolve(repoRoot, outputDirForMode(mode));
const captureDir = mode === "baselines" ? path.join(baselineDir, ".new") : outputDir;

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

function outputDirForMode(currentMode) {
  if (currentMode === "baselines") {
    return baselineDirFromEnv();
  }
  if (currentMode === "check") {
    return (
      process.env.STUDIO_STORY_NEW_DIR ??
      process.env.STUDIO_STORY_PNGS_DIR ??
      "lp-app/lp-studio-web/story-images/.new"
    );
  }
  return (
    process.env.STUDIO_STORY_SCRATCH_DIR ??
    process.env.STUDIO_STORY_PNGS_DIR ??
    "lp-app/lp-studio-web/story-images/.scratch"
  );
}

function baselineDirFromEnv() {
  return (
    process.env.STUDIO_STORY_IMAGES_DIR ??
    process.env.STUDIO_STORY_BASELINES_DIR ??
    "lp-app/lp-studio-web/story-images"
  );
}

async function discoverStoryIds() {
  const html = await runChrome([
    "--headless=new",
    "--disable-gpu",
    "--virtual-time-budget=5000",
    "--dump-dom",
    `${baseUrl}#/stories`,
  ]);
  return Array.from(html.matchAll(/href="#\/stories\/([^"]+)"/g))
    .map((match) => decodeURIComponent(match[1]))
    .filter((value, index, values) => values.indexOf(value) === index)
    .sort();
}

async function captureStories(storyIds, directory) {
  const files = [];
  const browser = await launchCaptureBrowser();
  try {
    for (const storyId of storyIds) {
      const file = path.join(directory, storyFileName(storyId));
      await browser.capture(storyPngUrl(storyId), storyId, file);
      console.log(`wrote ${path.relative(repoRoot, file)}`);
      files.push(file);
    }
  } finally {
    await browser.close();
  }
  return files;
}

async function launchCaptureBrowser() {
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
  const { targetId } = await cdp.send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await cdp.send("Target.attachToTarget", {
    targetId,
    flatten: true,
  });
  await cdp.send("Page.enable", {}, sessionId);
  await cdp.send("Runtime.enable", {}, sessionId);
  await cdp.send(
    "Emulation.setDeviceMetricsOverride",
    {
      width: 1080,
      height: 760,
      deviceScaleFactor: 1,
      mobile: false,
    },
    sessionId,
  );

  return {
    async capture(url, storyId, file) {
      await cdp.send("Page.navigate", { url }, sessionId);
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
  while (Date.now() - started < 10_000) {
    const box = await evaluate(cdp, sessionId, expression);
    if (box) {
      return box;
    }
    await delay(100);
  }
  throw new Error(`Timed out waiting for story capture target: ${storyId}`);
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
  const expectedFiles = new Set(storyIds.map(storyFileName));
  const baselineFiles = await listPngFiles(expectedDir);
  const unexpected = baselineFiles.filter((file) => !expectedFiles.has(file));
  const missing = [];
  const changed = [];

  for (const storyId of storyIds) {
    const fileName = storyFileName(storyId);
    const expectedFile = path.join(expectedDir, fileName);
    const actualFile = path.join(actualDir, fileName);
    const expected = await readOptionalFile(expectedFile);
    const actual = await readFile(actualFile);

    if (!expected) {
      missing.push(fileName);
    } else if (!expected.equals(actual)) {
      changed.push(fileName);
    }
  }

  printComparison("changed", changed);
  printComparison("new", missing);
  printComparison("removed", unexpected);

  if (changed.length === 0 && missing.length === 0 && unexpected.length === 0) {
    console.log("Story baselines match.");
    return true;
  }
  console.log(`Fresh PNGs: ${path.relative(repoRoot, actualDir)}`);
  return false;
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

function storyFileName(storyId) {
  return `${storyId.replaceAll("/", "__")}.png`;
}

function storyPngUrl(storyId) {
  return `${baseUrl}?story-png=1&story=${encodeURIComponent(storyId)}#/stories/${storyId}`;
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
