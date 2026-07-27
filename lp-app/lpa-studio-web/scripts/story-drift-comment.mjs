#!/usr/bin/env node
//
// Post (or update in place) the sticky PR comment summarizing a story-baseline
// auto-commit.
//
// CI-only: invoked by the `validate-stories` job right after it pushes a
// baseline refresh to the PR branch (see .github/workflows/pre-merge.yml and
// docs/adr/2026-07-26-ci-story-auto-commit.md). The comment is keyed by a
// hidden HTML marker so repeated drift runs update one comment instead of
// stacking new ones. The repo is public, so raw.githubusercontent.com URLs
// render as inline before/after thumbnails.
//
// Inputs via env: REPO (owner/name), PR_NUMBER, BEFORE_SHA (PR head before the
// bot commit), BOT_SHA (the bot commit), GH_TOKEN (for `gh`).
//
// `--dry-run` skips the `gh` calls and prints the markdown to stdout — usable
// locally against any two commits that differ under story-images/.

import { writeFile, mkdtemp, rm } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import path from "node:path";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");
const IMAGE_DIR = "lp-app/lpa-studio-web/story-images";
const MARKER = "<!-- story-baselines-auto-commit -->";
// GitHub caps comment bodies at 65536 chars; stay clearly under it.
const MAX_BODY = 60_000;
const THUMB_ROWS = 8;
const THUMB_WIDTH = 220;

const dryRun = process.argv.includes("--dry-run");

function run(command, args, { allowFailure = false, input } = {}) {
  const result = spawnSync(command, args, { cwd: repoRoot, encoding: "utf8", input });
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

function requireEnv(name) {
  const value = process.env[name];
  if (!value) fail(`Missing required env var ${name}.`);
  return value;
}

const repo = requireEnv("REPO");
const beforeSha = requireEnv("BEFORE_SHA");
const botSha = requireEnv("BOT_SHA");
const prNumber = dryRun ? process.env.PR_NUMBER ?? "0" : requireEnv("PR_NUMBER");

// -- Collect the baseline delta from the actual bot commit -------------------

const diff = run("git", [
  "diff",
  "--name-status",
  beforeSha,
  botSha,
  "--",
  IMAGE_DIR,
]).stdout;

const added = [];
const modified = [];
const deleted = [];
for (const line of diff.split("\n").filter(Boolean)) {
  const [status, ...paths] = line.split("\t");
  const name = path.basename(paths[paths.length - 1]);
  if (status.startsWith("A")) added.push(name);
  else if (status.startsWith("D")) deleted.push(name);
  // Renames should not occur in a flat delete-then-copy refresh; fold any
  // surprise status in with "modified" rather than dropping it silently.
  else modified.push(name);
}
if (added.length + modified.length + deleted.length === 0) {
  fail(`No story-image changes between ${beforeSha} and ${botSha}; nothing to report.`);
}

// -- Build the markdown ------------------------------------------------------

function rawUrl(sha, name) {
  return `https://raw.githubusercontent.com/${repo}/${sha}/${IMAGE_DIR}/${name}`;
}

function thumb(sha, name) {
  return `<img width="${THUMB_WIDTH}" alt="${name}@${sha.slice(0, 10)}" src="${rawUrl(sha, name)}">`;
}

// Modified stories first — they are the interesting diffs; adds/removes are
// self-explanatory from their single image.
const rows = [
  ...modified.map((name) => ({ name, before: true, after: true })),
  ...added.map((name) => ({ name, before: false, after: true })),
  ...deleted.map((name) => ({ name, before: true, after: false })),
];

function buildBody(thumbRows) {
  const counts = [
    modified.length && `${modified.length} changed`,
    added.length && `${added.length} added`,
    deleted.length && `${deleted.length} deleted`,
  ]
    .filter(Boolean)
    .join(", ");

  const lines = [
    MARKER,
    "### CI refreshed the story baselines on this branch",
    "",
    `The \`validate-stories\` job detected drift and committed the fresh set: **${counts}**`,
    `in https://github.com/${repo}/commit/${botSha}.`,
    "",
    "Review every PNG in the PR's **Files changed** view (swipe / onion-skin).",
    "Your local branch is now behind — `git pull` before pushing again.",
  ];

  const shown = rows.slice(0, thumbRows);
  if (shown.length > 0) {
    lines.push("", "| Story | Before | After |", "|---|---|---|");
    for (const row of shown) {
      const before = row.before ? thumb(beforeSha, row.name) : "—";
      const after = row.after ? thumb(botSha, row.name) : "—";
      lines.push(`| \`${row.name}\` | ${before} | ${after} |`);
    }
  }

  const rest = rows.slice(thumbRows);
  if (rest.length > 0) {
    lines.push("", `<details><summary>${rest.length} more file(s)</summary>`, "");
    for (const row of rest) {
      const tag = row.before && row.after ? "changed" : row.after ? "added" : "deleted";
      lines.push(`- \`${row.name}\` (${tag})`);
    }
    lines.push("", "</details>");
  }

  return lines.join("\n");
}

// Degrade gracefully toward the size cap: fewer thumbnails first, then (as a
// last resort for pathological cases) a bare truncation note.
let body = null;
for (const thumbRows of [THUMB_ROWS, THUMB_ROWS / 2, 0]) {
  const candidate = buildBody(thumbRows);
  if (candidate.length <= MAX_BODY) {
    body = candidate;
    break;
  }
}
if (body === null) {
  body = `${buildBody(0).slice(0, MAX_BODY - 60)}\n\n…(list truncated; see the commit)`;
}

if (dryRun) {
  console.log(body);
  process.exit(0);
}

// -- Sticky upsert via gh ----------------------------------------------------

const comments = JSON.parse(
  run("gh", [
    "api",
    `repos/${repo}/issues/${prNumber}/comments`,
    "--paginate",
    "--slurp",
  ]).stdout,
).flat();
const existing = comments.find((comment) => comment.body?.startsWith(MARKER));

const bodyDir = await mkdtemp(path.join(tmpdir(), "lp-story-comment-"));
try {
  const bodyFile = path.join(bodyDir, "body.md");
  await writeFile(bodyFile, body);
  if (existing) {
    run("gh", [
      "api",
      "--method",
      "PATCH",
      `repos/${repo}/issues/comments/${existing.id}`,
      "-F",
      `body=@${bodyFile}`,
    ]);
    console.log(`Updated existing comment ${existing.id} on PR #${prNumber}.`);
  } else {
    run("gh", [
      "api",
      "--method",
      "POST",
      `repos/${repo}/issues/${prNumber}/comments`,
      "-F",
      `body=@${bodyFile}`,
    ]);
    console.log(`Posted new comment on PR #${prNumber}.`);
  }
} finally {
  await rm(bodyDir, { recursive: true, force: true });
}
