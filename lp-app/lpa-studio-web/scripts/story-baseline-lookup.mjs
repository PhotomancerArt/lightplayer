#!/usr/bin/env node
//
// Resolve the story-baseline snapshot for a lightplayer commit.
//
// Story baselines live in the companion repo (PhotomancerArt/lightplayer-stories)
// as one snapshot commit per captured lightplayer main commit, on refs named
// `sha-<full-lightplayer-sha>`. The baseline for any comparison is the nearest
// captured first-parent ancestor of the comparison root:
//
//   - PR CI:        root = the merge-ref checkout's first parent (= the main
//                   tip the merge ref was built against), passed via --root.
//   - main-push CI: root = the pushed commit's first parent (its predecessor
//                   is the acceptance baseline), passed via --root.
//   - local dev:    no --root; defaults to `git merge-base HEAD origin/main`.
//
// Prints a single JSON object on stdout:
//   { "baselineSourceSha": ..., "ref": ..., "storiesCommitSha": ... }
// All progress/log output goes to stderr. Exit 2 when no captured ancestor is
// found within the walk limit.
//
// --fetch <dir>: also shallow-clone the resolved ref into <dir> (reused if it
// already holds the right snapshot commit). The baseline PNG set is then at
// <dir>/images — point STUDIO_STORY_BASELINES_DIR there.

import { spawnSync } from "node:child_process";
import { existsSync } from "node:fs";
import { rm } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import path from "node:path";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");

const STORIES_REPO = process.env.STORIES_REPO ?? "PhotomancerArt/lightplayer-stories";
const READ_URL = `https://github.com/${STORIES_REPO}.git`;
// How far back to walk main's first-parent chain. Bounded by how deep CI
// fetches main (--depth=60 in validate-stories) and by the stories repo's
// retention floor (newest 50 sha-* refs are always kept).
const MAX_WALK = Number(process.env.STUDIO_STORY_BASELINE_MAX_WALK ?? "50");

function run(command, args, { cwd = repoRoot, allowFailure = false } = {}) {
  const result = spawnSync(command, args, { cwd, encoding: "utf8" });
  if (result.error) fail(`Failed to run ${command}: ${result.error.message}`);
  if (result.status !== 0 && !allowFailure) {
    fail(`${command} ${args.join(" ")} failed:\n${result.stderr || result.stdout}`);
  }
  return result;
}

function fail(message, code = 1) {
  console.error(message);
  process.exit(code);
}

function parseArgs(argv) {
  const options = { root: null, fetchDir: null };
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (arg === "--root") options.root = argv[(i += 1)];
    else if (arg === "--fetch") options.fetchDir = argv[(i += 1)];
    else fail(`Unknown argument: ${arg}`);
  }
  return options;
}

const { root: rootArg, fetchDir } = parseArgs(process.argv.slice(2));

const root =
  rootArg ??
  run("git", ["merge-base", "HEAD", "origin/main"]).stdout.trim();
if (!/^[0-9a-f]{40}$/.test(root)) {
  const resolved = run("git", ["rev-parse", root]).stdout.trim();
  if (!/^[0-9a-f]{40}$/.test(resolved)) fail(`Cannot resolve comparison root: ${root}`);
}
const rootSha = /^[0-9a-f]{40}$/.test(root)
  ? root
  : run("git", ["rev-parse", root]).stdout.trim();

// First-parent candidates, newest first. On a shallow clone the walk simply
// stops at the shallow boundary, which is fine — CI fetches main deeper than
// the walk limit.
const candidates = run("git", [
  "rev-list",
  "--first-parent",
  `--max-count=${MAX_WALK}`,
  rootSha,
])
  .stdout.trim()
  .split("\n")
  .filter(Boolean);
if (candidates.length === 0) fail(`No candidate commits reachable from ${rootSha}.`);

// One network call: list every sha-* ref, then match candidates locally.
const lsRemote = run("git", ["ls-remote", READ_URL, "refs/heads/sha-*"]).stdout;
const captured = new Map();
for (const line of lsRemote.split("\n").filter(Boolean)) {
  const [storiesSha, ref] = line.split("\t");
  const match = ref?.match(/^refs\/heads\/sha-([0-9a-f]{40})$/);
  if (match) captured.set(match[1], storiesSha);
}
console.error(`Stories repo has ${captured.size} captured commit(s); walking ${candidates.length} candidate(s) from ${rootSha.slice(0, 10)}.`);

const baselineSourceSha = candidates.find((sha) => captured.has(sha));
if (!baselineSourceSha) {
  fail(
    `No captured baseline found among the ${candidates.length} first-parent ancestor(s) of ` +
      `${rootSha.slice(0, 10)}.\n` +
      "Either this branch is based on a main commit older than the stories repo's retention\n" +
      "window (rebase on main), or main captures have not run since the range was created.",
    2,
  );
}
const ref = `sha-${baselineSourceSha}`;
const storiesCommitSha = captured.get(baselineSourceSha);
if (baselineSourceSha !== candidates[0]) {
  console.error(
    `Nearest captured ancestor is ${candidates.indexOf(baselineSourceSha)} commit(s) behind ` +
      `${rootSha.slice(0, 10)} (a newer capture may still be in flight).`,
  );
}

if (fetchDir) {
  const resolvedDir = path.resolve(fetchDir);
  const existingSha =
    existsSync(resolvedDir) &&
    run("git", ["-C", resolvedDir, "rev-parse", "HEAD"], { allowFailure: true }).stdout?.trim();
  if (existingSha === storiesCommitSha) {
    console.error(`Reusing existing baseline checkout at ${resolvedDir}.`);
  } else {
    await rm(resolvedDir, { recursive: true, force: true });
    console.error(`Fetching ${ref} into ${resolvedDir}...`);
    run("git", ["clone", "--quiet", "--depth", "1", "--branch", ref, READ_URL, resolvedDir]);
    const fetched = run("git", ["-C", resolvedDir, "rev-parse", "HEAD"]).stdout.trim();
    if (fetched !== storiesCommitSha) {
      // sha-* refs never move; a mismatch means the remote changed under us.
      fail(`Fetched ${ref} resolved to ${fetched}, expected ${storiesCommitSha}.`);
    }
  }
}

process.stdout.write(`${JSON.stringify({ baselineSourceSha, ref, storiesCommitSha })}\n`);
