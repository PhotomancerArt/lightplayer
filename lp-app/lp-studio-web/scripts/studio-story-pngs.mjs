#!/usr/bin/env node

import { mkdir, rm } from "node:fs/promises";
import { spawn } from "node:child_process";
import { once } from "node:events";
import { fileURLToPath } from "node:url";
import path from "node:path";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");
const publicDir = path.join(repoRoot, "lp-app/lp-studio-web/public");
const outputDir = path.resolve(
  repoRoot,
  process.env.STUDIO_STORY_PNGS_DIR ?? "lp-app/lp-studio-web/story-pngs",
);
const port = process.env.STUDIO_STORY_PNGS_PORT ?? "2822";
const baseUrl = `http://127.0.0.1:${port}/`;
const chrome = process.env.CHROME_BIN ?? findChrome();

if (!chrome) {
  console.error(
    "Could not find Google Chrome. Set CHROME_BIN=/path/to/chrome to generate story PNGs.",
  );
  process.exit(1);
}

await rm(outputDir, { recursive: true, force: true });
await mkdir(outputDir, { recursive: true });

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

  for (const storyId of storyIds) {
    const file = path.join(outputDir, `${storyId.replaceAll("/", "__")}.png`);
    await runChrome([
      "--headless=new",
      "--disable-gpu",
      "--hide-scrollbars",
      "--window-size=1080,760",
      "--virtual-time-budget=3000",
      `--screenshot=${file}`,
      `${baseUrl}?story-png=1#/stories/${storyId}`,
    ]);
    console.log(`wrote ${path.relative(repoRoot, file)}`);
  }

  console.log(`Story PNGs: ${path.relative(repoRoot, outputDir)}`);
} finally {
  if (server.exitCode === null) {
    server.kill("SIGTERM");
  }
  await Promise.race([serverExited, delay(1_000)]);
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
  const child = spawn(chrome, args, { stdio: ["ignore", "pipe", "pipe"] });
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
    throw new Error(`Chrome exited with ${code}: ${stderr.trim()}`);
  }
  return stdout;
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
