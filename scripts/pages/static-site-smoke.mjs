#!/usr/bin/env node
import { spawn, spawnSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../..");
const args = parseArgs(process.argv.slice(2));
const kind = requiredArg(args, "kind");
const siteDir = path.resolve(repoRoot, requiredArg(args, "dir"));
const port = Number(args.port ?? "2830");
const browserMode = args.browser ?? "optional";
const serverMode = args.server ?? "required";
const baseUrl = `http://127.0.0.1:${port}/`;

const checks = {
  studio: {
    indexNeedle: "assets/lpa-studio-web-",
    required: [
      "index.html",
      "version.json",
      { prefix: "assets/tailwind-", suffix: ".css" },
      { prefix: "assets/lpa-studio-web-", suffix: ".js" },
      { prefix: "assets/lpa-studio-web_bg-", suffix: ".wasm" },
      "pkg/fw_browser.js",
      "pkg/fw_browser_bg.wasm",
      "firmware/esp32c6/manifest.json",
      "lpa-link/browser_esp32_device_controller.js",
    ],
  },
  "web-demo": {
    indexNeedle: "pkg/web_demo.js",
    required: [
      "index.html",
      "version.json",
      "rainbow-default.glsl",
      "pkg/web_demo.js",
      "pkg/web_demo_bg.wasm",
    ],
  },
};

const check = checks[kind];
if (!check) {
  throw new Error(`unknown smoke kind: ${kind}`);
}
if (!existsSync(siteDir)) {
  throw new Error(`site directory does not exist: ${siteDir}`);
}

checkLocalFiles();
if (serverMode === "off") {
  console.log(`Static file smoke passed: ${kind} at ${path.relative(repoRoot, siteDir)}`);
  process.exit(0);
}

const server = spawn("python3", ["-m", "http.server", String(port), "--bind", "127.0.0.1"], {
  cwd: siteDir,
  stdio: ["ignore", "pipe", "pipe"],
});

try {
  await waitForServer(baseUrl);
  const index = await fetchText(new URL("index.html", baseUrl));
  if (!index.includes(check.indexNeedle)) {
    throw new Error(`index.html does not reference ${check.indexNeedle}`);
  }
  for (const asset of check.required) {
    await fetchBytes(new URL(requiredAssetPath(asset), baseUrl));
  }
  await maybeRunBrowserSmoke();
  console.log(`Static smoke passed: ${kind} at ${path.relative(repoRoot, siteDir)}`);
} finally {
  server.kill("SIGTERM");
}

function checkLocalFiles() {
  const indexPath = path.join(siteDir, "index.html");
  const index = readFileSync(indexPath, "utf8");
  if (!index.includes(check.indexNeedle)) {
    throw new Error(`index.html does not reference ${check.indexNeedle}`);
  }
  for (const asset of check.required) {
    const assetPath = findRequiredAsset(asset);
    if (!existsSync(assetPath)) {
      throw new Error(`missing required asset: ${formatRequiredAsset(asset)}`);
    }
    if (statSync(assetPath).size === 0) {
      throw new Error(`required asset is empty: ${formatRequiredAsset(asset)}`);
    }
  }
}

function findRequiredAsset(required) {
  if (typeof required === "string") {
    return path.join(siteDir, required);
  }

  const match = listFiles(siteDir).find((file) => {
    const relative = path.relative(siteDir, file);
    return relative.startsWith(required.prefix) && relative.endsWith(required.suffix);
  });
  return match ?? path.join(siteDir, `${required.prefix}*${required.suffix}`);
}

function requiredAssetPath(required) {
  const assetPath = findRequiredAsset(required);
  return path.relative(siteDir, assetPath).split(path.sep).join("/");
}

function formatRequiredAsset(required) {
  if (typeof required === "string") {
    return required;
  }
  return `${required.prefix}*${required.suffix}`;
}

function listFiles(directory) {
  const files = [];
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...listFiles(entryPath));
    } else if (entry.isFile()) {
      files.push(entryPath);
    }
  }
  return files;
}

async function maybeRunBrowserSmoke() {
  if (browserMode === "off") {
    return;
  }
  const chrome = process.env.CHROME_BIN ?? findChrome();
  if (!chrome) {
    if (browserMode === "required") {
      throw new Error("Chrome not found; set CHROME_BIN or use --browser optional/off");
    }
    console.log("Chrome not found; skipped browser DOM smoke.");
    return;
  }
  const result = spawnSync(
    chrome,
    ["--headless=new", "--disable-gpu", "--virtual-time-budget=8000", "--dump-dom", baseUrl],
    { encoding: "utf8", timeout: 20_000 },
  );
  if (result.status !== 0) {
    const message = result.stderr || result.stdout || `Chrome exited with ${result.status}`;
    if (browserMode === "required") {
      throw new Error(message);
    }
    console.log(`Browser DOM smoke skipped after Chrome failure: ${message.trim()}`);
    return;
  }
  if (!result.stdout.includes("<html") && !result.stdout.includes("<!DOCTYPE html")) {
    throw new Error("browser DOM smoke did not return an HTML document");
  }
}

async function waitForServer(url) {
  let lastError;
  for (let attempt = 0; attempt < 50; attempt += 1) {
    try {
      await fetchText(url);
      return;
    } catch (error) {
      lastError = error;
      await delay(100);
    }
  }
  throw lastError;
}

async function fetchText(url) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${url.pathname} returned HTTP ${response.status}`);
  }
  return response.text();
}

async function fetchBytes(url) {
  const response = await fetch(url);
  if (!response.ok) {
    throw new Error(`${url.pathname} returned HTTP ${response.status}`);
  }
  const bytes = await response.arrayBuffer();
  if (bytes.byteLength === 0) {
    throw new Error(`${url.pathname} is empty`);
  }
}

function findChrome() {
  const candidates = [
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
    "/Applications/Chromium.app/Contents/MacOS/Chromium",
    "/usr/bin/google-chrome",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
  ];
  return candidates.find((candidate) => existsSync(candidate)) ?? null;
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function parseArgs(values) {
  const parsed = {};
  for (let index = 0; index < values.length; index += 1) {
    const value = values[index];
    if (!value.startsWith("--")) {
      throw new Error(`unexpected argument: ${value}`);
    }
    const key = value.slice(2);
    const next = values[index + 1];
    if (!next || next.startsWith("--")) {
      parsed[key] = "true";
    } else {
      parsed[key] = next;
      index += 1;
    }
  }
  return parsed;
}

function requiredArg(values, key) {
  const value = values[key];
  if (!value) {
    throw new Error(`missing required argument: --${key}`);
  }
  return value;
}
