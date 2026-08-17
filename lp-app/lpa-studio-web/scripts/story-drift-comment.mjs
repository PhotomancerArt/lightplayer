#!/usr/bin/env node
//
// Post (or update in place) the sticky PR comment summarizing story visual
// changes against the baseline snapshot.
//
// CI-only: invoked by `validate-stories` after it pushes a `pr-<n>` snapshot
// to the companion stories repo (see .github/workflows/pre-merge.yml). Visual
// changes do not fail CI — this comment IS the review surface, and the
// compare link it carries (baseline snapshot ... PR snapshot in the stories
// repo) is the full-detail view with GitHub's swipe/onion-skin PNG viewer.
// Merging the PR is acceptance.
//
// The comment is keyed by a hidden HTML marker so repeated runs update one
// comment instead of stacking new ones. When a later run finds NO changes but
// an earlier comment exists, the comment is rewritten to say so — a stale
// change list must not outlive the changes.
//
// Inputs via env:
//   REPO                 owner/name of the lightplayer repo
//   PR_NUMBER            pull request number
//   HEAD_SHA             PR head at capture time (provenance line)
//   STORIES_REPO         owner/name of the stories repo
//   BASELINE_STORIES_SHA baseline snapshot commit in the stories repo
//   BASELINE_SOURCE_SHA  lightplayer main commit the baseline came from
//   CHANGES_FILE         path to story-snapshot.mjs's JSON output
//   GH_TOKEN             for `gh`
//
// `--dry-run` skips the `gh` calls and prints the markdown to stdout.

import { writeFile, mkdtemp, rm, readFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import { tmpdir } from "node:os";
import { fileURLToPath } from "node:url";
import path from "node:path";

const scriptDir = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(scriptDir, "../../..");
const MARKER = "<!-- story-snapshot-comment -->";
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
const storiesRepo = requireEnv("STORIES_REPO");
const baselineStoriesSha = requireEnv("BASELINE_STORIES_SHA");
const baselineSourceSha = requireEnv("BASELINE_SOURCE_SHA");
const headSha = requireEnv("HEAD_SHA");
const changesFile = requireEnv("CHANGES_FILE");
const prNumber = dryRun ? process.env.PR_NUMBER ?? "0" : requireEnv("PR_NUMBER");

const changes = JSON.parse(await readFile(changesFile, "utf8"));
const { snapshotSha, added = [], modified = [], removed = [] } = changes;
if (!snapshotSha) fail(`${changesFile} has no snapshotSha.`);
const total = added.length + modified.length + removed.length;

// -- Build the markdown ------------------------------------------------------

function rawUrl(sha, name) {
  return `https://raw.githubusercontent.com/${storiesRepo}/${sha}/images/${name}`;
}

function thumb(sha, name) {
  return `<img width="${THUMB_WIDTH}" alt="${name}@${sha.slice(0, 10)}" src="${rawUrl(sha, name)}">`;
}

const compareUrl = `https://github.com/${storiesRepo}/compare/${baselineStoriesSha}...${snapshotSha}`;
const provenance =
  `Baseline: main \`${baselineSourceSha.slice(0, 10)}\` · captured at head \`${headSha.slice(0, 10)}\` · ` +
  `merging this PR is acceptance — no baseline files to commit.`;

// Modified stories first — they are the interesting diffs; adds/removes are
// self-explanatory from their single image.
const rows = [
  ...modified.map((name) => ({ name, before: true, after: true })),
  ...added.map((name) => ({ name, before: false, after: true })),
  ...removed.map((name) => ({ name, before: true, after: false })),
];

function buildBody(thumbRows) {
  const counts = [
    modified.length && `${modified.length} changed`,
    added.length && `${added.length} added`,
    removed.length && `${removed.length} deleted`,
  ]
    .filter(Boolean)
    .join(", ");

  const lines = [
    MARKER,
    "### Story visual changes",
    "",
    `This PR changes rendered stories: **${counts}**.`,
    "",
    `**[Review all changes in the stories repo](${compareUrl})** (swipe / onion-skin on every PNG).`,
    "",
    provenance,
  ];

  const shown = rows.slice(0, thumbRows);
  if (shown.length > 0) {
    lines.push("", "| Story | Before | After |", "|---|---|---|");
    for (const row of shown) {
      const before = row.before ? thumb(baselineStoriesSha, row.name) : "—";
      const after = row.after ? thumb(snapshotSha, row.name) : "—";
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

function buildNoChangesBody() {
  return [
    MARKER,
    "### Story visual changes",
    "",
    `No story visual changes at \`${headSha.slice(0, 10)}\` (an earlier revision of this PR had some).`,
    "",
    provenance,
  ].join("\n");
}

let body = null;
if (total > 0) {
  // Degrade gracefully toward the size cap: fewer thumbnails first, then (as
  // a last resort for pathological cases) a bare truncation note.
  for (const thumbRows of [THUMB_ROWS, THUMB_ROWS / 2, 0]) {
    const candidate = buildBody(thumbRows);
    if (candidate.length <= MAX_BODY) {
      body = candidate;
      break;
    }
  }
  if (body === null) {
    body = `${buildBody(0).slice(0, MAX_BODY - 60)}\n\n…(list truncated; see the compare link)`;
  }
}

if (dryRun) {
  console.log(body ?? `[no changes] would ${total === 0 ? "update an existing comment only" : "post"}:\n${buildNoChangesBody()}`);
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

if (total === 0 && !existing) {
  console.log("No story visual changes and no existing comment — nothing to post.");
  process.exit(0);
}
if (total === 0) body = buildNoChangesBody();

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
