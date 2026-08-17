#!/usr/bin/env node
//
// Build and push a story snapshot to the companion stories repo.
//
// A snapshot is one commit in PhotomancerArt/lightplayer-stories holding the
// baseline PNG set (`images/`), fresh pixels for tolerated-diff files
// (`tolerated/`, evidence retention), and a provenance `manifest.json`. The
// commit is PARENTED on the snapshot it was compared against, so GitHub's
// compare view between the two shows exactly the changed stories.
//
// `images/` is manifest-APPLIED, not a wholesale copy of the fresh capture:
// files the check tolerated as sub-threshold raster jitter keep the parent
// snapshot's bytes. This is deliberate and load-bearing — a wholesale copy
// would (a) let jitter ping-pong baseline bytes on every main capture and
// (b) pollute the snapshot-to-snapshot compare view (the human review
// surface) with tolerated noise. The fresh pixels are not lost: they land in
// `tolerated/` (see docs/defects/2026-07-27-story-check-tolerance-ignores-
// amplitude.md for why that evidence must be retained).
//
// Usage:
//   node story-snapshot.mjs \
//     --new-dir <capture dir with .check-complete + .refresh-manifest.json> \
//     --baseline-ref sha-<sha> --baseline-commit <stories sha> \
//     --target-ref <pr-N | sha-<source sha>> --source-sha <sha> \
//     [--run-url <url>] [--update-latest] [--push-url <url>] [--dry-run]
//
// Prints JSON on stdout:
//   { "snapshotSha", "added": [...], "modified": [...], "removed": [...],
//     "toleratedCount": n, "pushed": true|false }
// Pushing: `pr-*` refs are force-updated; a `sha-*` ref that already exists
// remotely is left alone (re-runs are idempotent) but `latest` still updates.

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { copyFile, mkdir, mkdtemp, readFile, readdir, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import path from "node:path";
import { applyRefresh } from "./story-apply-refresh.mjs";

const STORIES_REPO = process.env.STORIES_REPO ?? "PhotomancerArt/lightplayer-stories";
const READ_URL = `https://github.com/${STORIES_REPO}.git`;
const REFRESH_MANIFEST_FILE = ".refresh-manifest.json";

function fail(message) {
  console.error(message);
  process.exit(1);
}

function parseArgs(argv) {
  const options = {
    newDir: null,
    baselineRef: null,
    baselineCommit: null,
    targetRef: null,
    sourceSha: null,
    runUrl: null,
    updateLatest: false,
    pushUrl: null,
    dryRun: false,
  };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--new-dir") options.newDir = argv[(i += 1)];
    else if (arg === "--baseline-ref") options.baselineRef = argv[(i += 1)];
    else if (arg === "--baseline-commit") options.baselineCommit = argv[(i += 1)];
    else if (arg === "--target-ref") options.targetRef = argv[(i += 1)];
    else if (arg === "--source-sha") options.sourceSha = argv[(i += 1)];
    else if (arg === "--run-url") options.runUrl = argv[(i += 1)];
    else if (arg === "--update-latest") options.updateLatest = true;
    else if (arg === "--push-url") options.pushUrl = argv[(i += 1)];
    else if (arg === "--dry-run") options.dryRun = true;
    else fail(`Unknown argument: ${arg}`);
  }
  for (const key of ["newDir", "baselineRef", "baselineCommit", "targetRef", "sourceSha"]) {
    if (!options[key]) fail(`Missing required option --${key.replace(/[A-Z]/g, (c) => `-${c.toLowerCase()}`)}`);
  }
  if (!/^(pr-\d+|sha-[0-9a-f]{40})$/.test(options.targetRef)) {
    fail(`--target-ref must be pr-<number> or sha-<full-sha>, got: ${options.targetRef}`);
  }
  return options;
}

const options = parseArgs(process.argv.slice(2));
const newDir = path.resolve(options.newDir);

function git(args, { cwd, allowFailure = false } = {}) {
  const result = spawnSync("git", args, { cwd, encoding: "utf8" });
  if (result.error) fail(`Failed to run git: ${result.error.message}`);
  if (result.status !== 0 && !allowFailure) {
    fail(`git ${args.join(" ")} failed:\n${result.stderr || result.stdout}`);
  }
  return result;
}

// A snapshot may only be built from a COMPLETE comparison — a partial capture
// is missing stories, and applying its manifest would still produce an
// images/ set whose absences lie.
if (!existsSync(path.join(newDir, ".check-complete"))) {
  fail(`${newDir} has no .check-complete sentinel — refusing to snapshot a partial capture.`);
}
const manifest = JSON.parse(await readFile(path.join(newDir, REFRESH_MANIFEST_FILE), "utf8"));

// -- Assemble the snapshot working tree --------------------------------------

const workDir = await mkdtemp(path.join(tmpdir(), "lp-story-snapshot-"));
try {
  git([
    "clone", "--quiet", "--depth", "1",
    "--branch", options.baselineRef,
    READ_URL, workDir,
  ]);
  const parentSha = git(["rev-parse", "HEAD"], { cwd: workDir }).stdout.trim();
  if (parentSha !== options.baselineCommit) {
    // sha-* refs never move; a mismatch means the comparison baseline and the
    // snapshot parent would disagree, which corrupts the compare-view story.
    fail(`${options.baselineRef} is at ${parentSha}, expected ${options.baselineCommit}.`);
  }

  const imagesDir = path.join(workDir, "images");
  await mkdir(imagesDir, { recursive: true });

  // Classify against the parent's images/ BEFORE applying (added vs modified).
  const parentImages = new Set(await readdir(imagesDir));
  const replace = manifest.replace ?? [];
  const added = replace.filter((name) => !parentImages.has(name));
  const modified = replace.filter((name) => parentImages.has(name));
  const removed = manifest.remove ?? [];
  const tolerated = manifest.tolerated ?? [];

  await applyRefresh(newDir, imagesDir);

  const toleratedDir = path.join(workDir, "tolerated");
  await rm(toleratedDir, { recursive: true, force: true });
  if (tolerated.length > 0) {
    await mkdir(toleratedDir, { recursive: true });
    for (const name of tolerated) {
      await copyFile(path.join(newDir, name), path.join(toleratedDir, name));
    }
  }

  await writeFile(
    path.join(workDir, "manifest.json"),
    `${JSON.stringify(
      {
        sourceRepo: "PhotomancerArt/lightplayer",
        sourceSha: options.sourceSha,
        baselineSourceSha: options.baselineRef.replace(/^sha-/, ""),
        capturedAt: new Date().toISOString(),
        runUrl: options.runUrl,
        chromeForTesting: process.env.CHROME_FOR_TESTING_VERSION ?? null,
        oxipng: process.env.OXIPNG_VERSION ?? null,
        wasmBindgen: process.env.WASM_BINDGEN_VERSION ?? null,
        dioxusCli: process.env.DIOXUS_CLI_VERSION ?? null,
        added,
        modified,
        removed,
        tolerated,
      },
      null,
      2,
    )}\n`,
  );

  git(["config", "user.name", "github-actions[bot]"], { cwd: workDir });
  git(["config", "user.email", "41898282+github-actions[bot]@users.noreply.github.com"], {
    cwd: workDir,
  });
  git(["add", "-A"], { cwd: workDir });
  git(
    [
      "commit", "--quiet",
      "-m", `snapshot: lightplayer ${options.sourceSha.slice(0, 10)} (${options.targetRef})`,
      ...(options.runUrl ? ["-m", options.runUrl] : []),
    ],
    { cwd: workDir },
  );
  const snapshotSha = git(["rev-parse", "HEAD"], { cwd: workDir }).stdout.trim();

  // -- Push --------------------------------------------------------------------

  let pushed = false;
  if (options.dryRun) {
    console.error(`[dry-run] would push ${snapshotSha} to ${options.targetRef}` +
      (options.updateLatest ? " and latest" : ""));
  } else {
    const pushUrl = options.pushUrl ?? READ_URL;
    const refSpecs = [];
    if (options.targetRef.startsWith("pr-")) {
      refSpecs.push(`+HEAD:refs/heads/${options.targetRef}`);
      if (options.updateLatest) refSpecs.push("+HEAD:refs/heads/latest");
    } else {
      const existing = git(["ls-remote", pushUrl, `refs/heads/${options.targetRef}`])
        .stdout.trim();
      if (existing) {
        // Idempotent re-run: the ref (and any latest update) landed with the
        // run that created it. Pushing latest again here would point it at a
        // DIFFERENT commit (fresh capturedAt) than the sha-* ref — skip both.
        console.error(`${options.targetRef} already exists remotely; skipping (idempotent re-run).`);
      } else {
        refSpecs.push(`HEAD:refs/heads/${options.targetRef}`);
        if (options.updateLatest) refSpecs.push("+HEAD:refs/heads/latest");
      }
    }
    if (refSpecs.length > 0) {
      git(["push", "--quiet", pushUrl, ...refSpecs], { cwd: workDir });
      pushed = true;
      console.error(`Pushed ${snapshotSha.slice(0, 10)} to ${refSpecs.join(", ")}.`);
    }
  }

  process.stdout.write(
    `${JSON.stringify({ snapshotSha, added, modified, removed, toleratedCount: tolerated.length, pushed })}\n`,
  );
} finally {
  await rm(workDir, { recursive: true, force: true });
}
