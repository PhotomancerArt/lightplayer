#!/usr/bin/env node
//
// Pull CI-captured story baselines for the current branch and stage them.
//
// MANUAL FALLBACK. Story baselines are CI-canonical, and on same-repo PRs the
// `validate-stories` job now auto-commits refreshed baselines to the branch
// directly (docs/adr/2026-07-26-ci-story-auto-commit.md) — the common case
// needs no pull at all (just `git pull` the bot commit). This helper covers
// the paths auto-commit can't: fork PRs, push races, and main-push drift,
// where the job still fails and uploads the fresh set as the
// `story-images-fresh` artifact. It downloads that artifact, replaces the
// committed baseline set with it, and stages the result — the branch owner
// reviews the diff and commits. This helper never commits.

import { mkdtemp, readdir, rm } from "node:fs/promises";
import { applyRefresh } from "./story-apply-refresh.mjs";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import path from "node:path";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");
const baselineDir = path.join(repoRoot, "lp-app/lpa-studio-web/story-images");
const ARTIFACT_NAME = "story-images-fresh";
// The workflow FILE name, not its display name — the display name ("CI") is
// ambiguous to `gh` because renamed/deleted workflows stay registered.
const WORKFLOW_NAME = "pre-merge.yml";

function run(command, args, { allowFailure = false } = {}) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8" });
  if (result.error) {
    fail(`Failed to run ${command}: ${result.error.message}`);
  }
  if (result.status !== 0 && !allowFailure) {
    fail(`${command} ${args.join(" ")} failed:\n${result.stderr || result.stdout}`);
  }
  return result;
}

function fail(message) {
  console.error(message);
  process.exit(1);
}

const ghAuth = run("gh", ["auth", "status"], { allowFailure: true });
if (ghAuth.status !== 0) {
  fail(
    "`gh` is not authenticated (or not installed). Run `gh auth login` first.\n" +
      (ghAuth.stderr || ghAuth.stdout || ""),
  );
}

const branch = run("git", ["branch", "--show-current"]).stdout.trim();
if (!branch) {
  fail("Detached HEAD: check out the branch whose CI run captured the baselines.");
}
const localHead = run("git", ["rev-parse", "HEAD"]).stdout.trim();

const runsJson = run("gh", [
  "run",
  "list",
  "--branch",
  branch,
  "--workflow",
  WORKFLOW_NAME,
  "--json",
  "databaseId,headSha,status,conclusion,createdAt",
  "--limit",
  "20",
]).stdout;
const runs = JSON.parse(runsJson);
if (runs.length === 0) {
  fail(`No "${WORKFLOW_NAME}" workflow runs found for branch ${branch}. Push (and open a PR) first.`);
}

// Newest run whose artifacts include the fresh capture set. A run can still be
// in_progress overall while validate-stories has already failed and uploaded,
// so artifact presence — not run status — is the criterion.
let sourceRun = null;
for (const candidate of runs) {
  const artifacts = JSON.parse(
    run("gh", [
      "api",
      `repos/{owner}/{repo}/actions/runs/${candidate.databaseId}/artifacts`,
    ]).stdout,
  );
  if (artifacts.artifacts?.some((artifact) => artifact.name === ARTIFACT_NAME && !artifact.expired)) {
    sourceRun = candidate;
    break;
  }
}
if (!sourceRun) {
  const newest = runs[0];
  fail(
    `No "${ARTIFACT_NAME}" artifact found on the last ${runs.length} ${WORKFLOW_NAME} run(s) for ${branch}.\n` +
      `Newest run: ${newest.status}${newest.conclusion ? ` (${newest.conclusion})` : ""} at ${newest.createdAt}.\n` +
      "Either the story check passed (nothing to pull), the job is still running, the\n" +
      "artifact expired (7-day retention), or the change didn't trigger the story path gate.",
  );
}

if (sourceRun.headSha !== localHead) {
  console.warn(
    `WARNING: artifact was captured at ${sourceRun.headSha.slice(0, 10)} but local HEAD is ` +
      `${localHead.slice(0, 10)}.\n` +
      "Proceeding — but if UI changes landed since that run, these baselines lag them and\n" +
      "the next CI run will fail again.",
  );
}

const downloadDir = await mkdtemp(path.join(tmpdir(), "lp-story-pull-"));
try {
  run("gh", [
    "run",
    "download",
    String(sourceRun.databaseId),
    "--name",
    ARTIFACT_NAME,
    "--dir",
    downloadDir,
  ]);

  const downloadedAll = await readdir(downloadDir);
  // The capture script writes .check-complete only after comparing a COMPLETE
  // capture set. Without it this artifact is a partial capture from a crashed
  // run — staging it would delete every baseline the capture didn't reach.
  if (!downloadedAll.includes(".check-complete")) {
    fail(
      `Artifact "${ARTIFACT_NAME}" from run ${sourceRun.databaseId} has no .check-complete ` +
        "sentinel — it is a partial capture from a crashed run. Not staging it.\n" +
        "Fix or re-run the validate-stories job and pull again.",
    );
  }
  const downloaded = downloadedAll.filter((name) => !name.startsWith("."));
  if (downloaded.length === 0) {
    fail(`Artifact "${ARTIFACT_NAME}" from run ${sourceRun.databaseId} is empty.`);
  }
  const nonPng = downloaded.filter((name) => !name.endsWith(".png"));
  if (nonPng.length > 0) {
    fail(`Artifact contains unexpected non-PNG files: ${nonPng.slice(0, 5).join(", ")}`);
  }

  // Manifest-driven (same helper CI uses): replace only the baselines the
  // check judged stale. A wholesale swap would also stage the files the check
  // deliberately tolerated as sub-threshold raster jitter, which is how that
  // noise became committed baseline churn.
  await applyRefresh(downloadDir, baselineDir);
} finally {
  await rm(downloadDir, { recursive: true, force: true });
}

run("git", ["add", "-A", "--", "lp-app/lpa-studio-web/story-images"]);

const status = run("git", [
  "status",
  "--porcelain",
  "--",
  "lp-app/lpa-studio-web/story-images",
]).stdout.split("\n").filter(Boolean);
const counts = { added: 0, modified: 0, deleted: 0 };
for (const line of status) {
  const code = line[0];
  if (code === "A") counts.added += 1;
  else if (code === "M") counts.modified += 1;
  else if (code === "D") counts.deleted += 1;
}

const runUrl = `${run("gh", ["repo", "view", "--json", "url", "--jq", ".url"]).stdout.trim()}/actions/runs/${sourceRun.databaseId}`;
console.log(
  `Staged baselines from run ${sourceRun.databaseId} (${sourceRun.headSha.slice(0, 10)}): ` +
    `${counts.added} added, ${counts.modified} changed, ${counts.deleted} deleted ` +
    `(${status.length} total staged paths).`,
);
console.log(`Source run: ${runUrl}`);
console.log("Review the staged diff, then commit these with your UI change (or as a baseline-refresh commit).");
