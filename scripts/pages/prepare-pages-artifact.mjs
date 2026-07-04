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

// Changelog derivation config. Declared before the top-level artifact writes
// below so `changelogInfo()` (invoked during that write) does not hit these
// `const`s while they are still in the temporal dead zone.
const CHANGELOG_ENTRY_LIMIT = 8;
const CHANGELOG_SUMMARY_MAX = 120;
const VERSION_TAG_PATTERN = /^v[0-9]{4}\.[0-9]{2}\.[0-9]{2}-[0-9]+$/;
const MERGE_SUBJECT_PATTERN = /^Merge pull request #(\d+) from /;

const configs = {
  studio: {
    app: "lightplayer-studio",
    sourceDir: path.join(repoRoot, "target/dx/lpa-studio-web/release/web/public"),
    entries: ["index.html", "assets", "pkg", "lpa-link", "firmware", "serial-debug.html"],
    required: [
      "index.html",
      { prefix: "assets/tailwind-", suffix: ".css" },
      { prefix: "assets/lpa-studio-web-", suffix: ".js" },
      { prefix: "assets/lpa-studio-web_bg-", suffix: ".wasm" },
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
  const file = await findRequiredAsset(outDir, required);
  if (!existsSync(file)) {
    throw new Error(`missing required deploy asset: ${formatRequiredAsset(required)}`);
  }
}

await writeFile(path.join(outDir, ".nojekyll"), "");
if (domain) {
  await writeFile(path.join(outDir, "CNAME"), `${domain}\n`);
}
await writeFile(path.join(outDir, "version.json"), `${JSON.stringify(versionInfo(config.app), null, 2)}\n`);
await writeFile(path.join(outDir, "changelog.json"), `${JSON.stringify(changelogInfo(), null, 2)}\n`);

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

// Best-effort "Recent updates" list built from git version tags. Requires the
// checkout to have tags/history (both Pages workflows use `fetch-depth: 0`); on
// a shallow clone or a tagless tree this simply yields `entries: []`. It must
// never throw — a missing changelog must not fail the deploy.
function changelogInfo() {
  return {
    schemaVersion: 1,
    generatedAt: new Date().toISOString(),
    entries: versionTags()
      .slice(0, CHANGELOG_ENTRY_LIMIT)
      .map(changelogEntry)
      .filter((entry) => entry !== null),
  };
}

function versionTags() {
  const output = commandOrEmpty("git", ["tag", "--sort=-creatordate", "--list", "v*"]);
  return output
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => VERSION_TAG_PATTERN.test(line));
}

// Each version tag is treated as a single unit and summarized best-effort from
// the commit it points at: a GitHub merge commit contributes the PR number and
// its body (the human PR title); any other commit contributes its subject.
function changelogEntry(tag) {
  const subject = commandOrEmpty("git", ["log", "-1", "--pretty=%s", tag]);
  const version = tag.replace(/^v/, "");
  if (!version) {
    return null;
  }

  const merge = subject.match(MERGE_SUBJECT_PATTERN);
  let summary = subject;
  let pr = null;
  if (merge) {
    pr = Number(merge[1]);
    const body = commandOrEmpty("git", ["log", "-1", "--pretty=%b", tag]);
    summary = firstNonEmptyLine(body) ?? subject;
  }

  return {
    version,
    date: commandOrEmpty("git", ["log", "-1", "--pretty=%cs", tag]) || null,
    summary: summary.trim().slice(0, CHANGELOG_SUMMARY_MAX),
    pr,
  };
}

function firstNonEmptyLine(text) {
  for (const line of text.split("\n")) {
    const trimmed = line.trim();
    if (trimmed) {
      return trimmed;
    }
  }
  return null;
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

async function findRequiredAsset(root, required) {
  if (typeof required === "string") {
    return path.join(root, required);
  }

  const files = await listFiles(root);
  const match = files.find((file) => {
    const relative = path.relative(root, file.path);
    return relative.startsWith(required.prefix) && relative.endsWith(required.suffix);
  });
  return match?.path ?? path.join(root, `${required.prefix}*${required.suffix}`);
}

function formatRequiredAsset(required) {
  if (typeof required === "string") {
    return required;
  }
  return `${required.prefix}*${required.suffix}`;
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

function commandOrEmpty(command, args = []) {
  try {
    return execFileSync(command, args, {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
  } catch {
    return "";
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
