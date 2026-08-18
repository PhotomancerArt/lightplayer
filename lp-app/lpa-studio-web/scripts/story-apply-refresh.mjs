#!/usr/bin/env node
//
// Apply a fresh story capture onto the committed baselines, honouring the
// refresh manifest the `check` comparison wrote next to it.
//
// Why not just swap the whole set: `check` deliberately TOLERATES diffs below
// its significance threshold (anti-aliasing and sub-pixel raster jitter that
// differs run to run on identical input). A wholesale copy commits those
// tolerated files anyway, so a story that renders one hairline differently
// this run gets its baseline rewritten — and rewritten back the next run.
// That ping-pong is the churner class in docs/debt/story-capture-pipeline.md.
// Replacing only manifest-listed files makes the tolerance mean what it says.
//
// Usage: story-apply-refresh.mjs <fresh-capture-dir> [baseline-dir]

import { readFile, readdir, copyFile, unlink } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const REFRESH_MANIFEST_FILE = ".refresh-manifest.json";
const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");

export async function applyRefresh(freshDir, baselineDir) {
  let manifest;
  try {
    manifest = JSON.parse(await readFile(path.join(freshDir, REFRESH_MANIFEST_FILE), "utf8"));
  } catch (error) {
    // No manifest means the capture predates this mechanism or crashed before
    // comparing. Refuse rather than silently falling back to a wholesale swap
    // — that fallback is exactly the behaviour this script exists to prevent.
    throw new Error(
      `No ${REFRESH_MANIFEST_FILE} in ${freshDir} (${error.message}). ` +
        "The capture must come from a complete `check` run.",
    );
  }
  const replace = manifest.replace ?? [];
  const remove = manifest.remove ?? [];
  const bad = [...replace, ...remove].filter(
    (name) => !name.endsWith(".png") || name.includes("/") || name.includes(".."),
  );
  if (bad.length > 0) {
    throw new Error(`Manifest contains unexpected entries: ${bad.slice(0, 5).join(", ")}`);
  }

  const available = new Set(
    (await readdir(freshDir)).filter((name) => name.endsWith(".png")),
  );
  const absent = replace.filter((name) => !available.has(name));
  if (absent.length > 0) {
    throw new Error(
      `Manifest lists files missing from the capture: ${absent.slice(0, 5).join(", ")}`,
    );
  }

  for (const name of replace) {
    await copyFile(path.join(freshDir, name), path.join(baselineDir, name));
  }
  for (const name of remove) {
    await unlink(path.join(baselineDir, name)).catch(() => {});
  }
  return { replaced: replace.length, removed: remove.length };
}

// CLI entry only. Argument parsing MUST stay inside this guard:
// `story-snapshot.mjs` imports `applyRefresh`, and a module-scope
// `process.exit(2)` for missing argv would kill the importer before it did
// anything (that exact bug shipped once via the old story-pull.mjs).
if (import.meta.url === `file://${process.argv[1]}`) {
  const freshDir = path.resolve(process.argv[2] ?? "");
  const baselineDir = path.resolve(
    process.argv[3] ?? path.join(repoRoot, "lp-app/lpa-studio-web/story-images"),
  );
  if (!process.argv[2]) {
    console.error("Usage: story-apply-refresh.mjs <fresh-capture-dir> [baseline-dir]");
    process.exit(2);
  }
  try {
    const { replaced, removed } = await applyRefresh(freshDir, baselineDir);
    console.log(
      `Applied story refresh: ${replaced} baseline(s) replaced, ${removed} removed ` +
        "(files within the check's pixel tolerance were left untouched).",
    );
  } catch (error) {
    console.error(error.message);
    process.exit(1);
  }
}
