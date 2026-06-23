#!/usr/bin/env node
import { execFileSync } from "node:child_process";
import { cp, mkdir, rm, stat, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../..");
const args = parseArgs(process.argv.slice(2));
const kind = requiredArg(args, "kind");
const outDir = path.resolve(repoRoot, requiredArg(args, "out"));
const channel = args.channel ?? "local";
const domain = args.domain ?? "";

const configs = {
  studio: {
    app: "lightplayer-studio",
    sourceDir: path.join(repoRoot, "lp-app/lpa-studio-web/public"),
    entries: ["index.html", "pkg", "lpa-link", "firmware", "serial-debug.html"],
    required: [
      "index.html",
      "pkg/lpa-studio-web.js",
      "pkg/lpa-studio-web_bg.wasm",
      "pkg/fw_browser.js",
      "pkg/fw_browser_bg.wasm",
      "lpa-link/browser_esp32_device_controller.js",
      "firmware/esp32c6/manifest.json",
    ],
  },
  "web-demo": {
    app: "lightplayer-web-demo",
    sourceDir: path.join(repoRoot, "lp-app/web-demo/www"),
    entries: ["index.html", "rainbow-default.glsl", "pkg"],
    required: ["index.html", "rainbow-default.glsl", "pkg/web_demo.js", "pkg/web_demo_bg.wasm"],
  },
};

const config = configs[kind];
if (!config) {
  throw new Error(`unknown artifact kind: ${kind}`);
}

await rm(outDir, { recursive: true, force: true });
await mkdir(outDir, { recursive: true });

for (const entry of config.entries) {
  const source = path.join(config.sourceDir, entry);
  if (!existsSync(source)) {
    continue;
  }
  await cp(source, path.join(outDir, entry), { recursive: true });
}

for (const required of config.required) {
  const file = path.join(outDir, required);
  if (!existsSync(file)) {
    throw new Error(`missing required deploy asset: ${required}`);
  }
}

await writeFile(path.join(outDir, ".nojekyll"), "");
if (domain) {
  await writeFile(path.join(outDir, "CNAME"), `${domain}\n`);
}
await writeFile(path.join(outDir, "version.json"), `${JSON.stringify(versionInfo(config.app), null, 2)}\n`);

const files = await listFiles(outDir);
const totalBytes = files.reduce((sum, file) => sum + file.size, 0);
console.log(`Pages artifact: ${path.relative(repoRoot, outDir)}`);
console.log(`Channel: ${channel}`);
console.log(`Total size: ${formatBytes(totalBytes)}`);
console.log("Largest files:");
for (const file of files.sort((a, b) => b.size - a.size).slice(0, 8)) {
  console.log(`  ${formatBytes(file.size).padStart(9)}  ${path.relative(outDir, file.path)}`);
}

function versionInfo(app) {
  const generatedAt = new Date().toISOString();
  return {
    schemaVersion: 1,
    app,
    channel,
    version: commandOrUnknown("scripts/print-app-version.sh"),
    source: {
      repository: process.env.GITHUB_REPOSITORY ?? "light-player/lightplayer",
      ref: process.env.GITHUB_REF_NAME ?? commandOrUnknown("git", ["branch", "--show-current"]),
      sha: process.env.GITHUB_SHA ?? commandOrUnknown("git", ["rev-parse", "HEAD"]),
      dirty: isDirty(),
    },
    build: {
      generatedAt,
      workflow: process.env.GITHUB_WORKFLOW ?? null,
      runId: process.env.GITHUB_RUN_ID ?? null,
      runAttempt: process.env.GITHUB_RUN_ATTEMPT ?? null,
    },
  };
}

async function listFiles(directory) {
  const entries = await import("node:fs/promises").then((fs) =>
    fs.readdir(directory, { withFileTypes: true }),
  );
  const files = [];
  for (const entry of entries) {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await listFiles(entryPath)));
    } else if (entry.isFile()) {
      const metadata = await stat(entryPath);
      files.push({ path: entryPath, size: metadata.size });
    }
  }
  return files;
}

function isDirty() {
  if (process.env.GITHUB_ACTIONS === "true") {
    return false;
  }
  try {
    execFileSync("git", ["diff", "--quiet", "--ignore-submodules"], {
      cwd: repoRoot,
      stdio: "ignore",
    });
    execFileSync("git", ["diff", "--cached", "--quiet", "--ignore-submodules"], {
      cwd: repoRoot,
      stdio: "ignore",
    });
    return false;
  } catch {
    return true;
  }
}

function commandOrUnknown(command, args = []) {
  try {
    return execFileSync(command, args, {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    return "unknown";
  }
}

function formatBytes(bytes) {
  const units = ["B", "KiB", "MiB", "GiB"];
  let value = bytes;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return `${value.toFixed(unit === 0 ? 0 : 1)} ${units[unit]}`;
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
