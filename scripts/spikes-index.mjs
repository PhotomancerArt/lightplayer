#!/usr/bin/env node
// Generate `spikes/index.html` — the browsable contact sheet for the design
// spikes. Everything on that page is DERIVED from the spikes themselves, so
// adding a spike is the only step: `just spikes-index` rewrites the index and
// `just lint-spikes-index` (in `check-lint`) fails when the checked-in copy
// has drifted. Spikes are committed design records — this script never edits
// them, it only reads their `<title>` and opening paragraph.
//
//   node scripts/spikes-index.mjs            # write spikes/index.html
//   node scripts/spikes-index.mjs --check    # fail if it would change
//
// Output must be deterministic: no timestamps, no live git state, entries in
// a stable order. A `--check` that can flap is worse than no gate at all —
// which is why dates live in `spikes/dates.json` instead of being read from
// git on every run. CI checks out shallow (`actions/checkout` defaults to
// depth 1), so `git log -- <path>` there answers for the tip commit alone and
// would disagree with any full clone. The date is asked of git ONCE, when a
// spike first appears, and is a plain committed fact from then on.

import { readFileSync, readdirSync, existsSync, writeFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const spikesDir = path.join(repoRoot, "spikes");
const indexPath = path.join(spikesDir, "index.html");
const datesPath = path.join(spikesDir, "dates.json");
const check = process.argv.slice(2).includes("--check");

/** Ledes are teasers, not abstracts — long opening paragraphs get cut here. */
const LEDE_MAX = 260;

// Called from the bottom of the file: the helpers below own `const` tables
// that this walk reaches through, and they are in the temporal dead zone
// until the whole module has evaluated.
function main() {
  const spikes = collectSpikes().sort(newestFirst);
  const pages = spikes.filter((spike) => spike.href !== null);
  const code = spikes.filter((spike) => spike.href === null);
  const outputs = [
    [datesPath, renderDates(spikes)],
    [indexPath, renderIndex(pages, code)],
  ];
  const tally = `${pages.length} pages, ${code.length} code spikes`;

  const stale = outputs.filter(([file, want]) => !existsSync(file) || readFileSync(file, "utf8") !== want);
  if (!check) {
    for (const [file, contents] of outputs) writeFileSync(file, contents);
    console.log(`wrote spikes/index.html and spikes/dates.json (${tally})`);
  } else if (stale.length > 0) {
    for (const [file] of stale) console.error(`${path.relative(repoRoot, file)} is out of date.`);
    console.error("  regenerate with: just spikes-index");
    process.exit(1);
  } else {
    console.log(`spikes/index.html is up to date (${tally})`);
  }
}

/**
 * Newest spike first — the contact sheet is read for "what happened lately".
 * Undated spikes sink to the bottom, and the name breaks ties so the order
 * never depends on readdir.
 */
function newestFirst(a, b) {
  if (a.date !== b.date) return (b.date ?? "").localeCompare(a.date ?? "");
  return a.name.localeCompare(b.name);
}

// ---------------------------------------------------------------- collection

/**
 * Every `spikes/<dir>/`, sorted. An `index.html` makes it a browsable page
 * spike (title + lede from the page); otherwise it is a code spike and the
 * README speaks for it. A directory with neither is a bug worth failing on —
 * silently dropping it is how an index stops being a complete list.
 */
function collectSpikes() {
  const recorded = existsSync(datesPath) ? JSON.parse(readFileSync(datesPath, "utf8")) : {};
  return readdirSync(spikesDir, { withFileTypes: true })
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name)
    .sort()
    .map((name) => {
      const date = spikeDate(name, recorded);
      const page = path.join(spikesDir, name, "index.html");
      if (existsSync(page)) {
        return { name, date, href: `./${name}/index.html`, ...describePage(readFileSync(page, "utf8")) };
      }
      const readme = path.join(spikesDir, name, "README.md");
      if (existsSync(readme)) {
        return { name, date, href: null, ...describeReadme(readFileSync(readme, "utf8")) };
      }
      throw new Error(`spikes/${name}/ has neither index.html nor README.md — nothing to index`);
    });
}

/**
 * A spike's date, in `spikes/dates.json` order of authority: what is recorded
 * there wins, and git is asked only for a spike that has none yet. Recorded
 * dates are never refreshed — see the header note on shallow CI clones.
 *
 * Edit `dates.json` by hand when a spike gets a later round and you want the
 * index to say so; that file is the fact, and a regeneration will keep it.
 * A spike still uncommitted has no date yet and sorts last until it does.
 */
function spikeDate(name, recorded) {
  if (typeof recorded[name] === "string") return recorded[name];
  try {
    const date = execFileSync("git", ["log", "-1", "--format=%as", "--", `spikes/${name}`], {
      cwd: repoRoot,
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    }).trim();
    return /^\d{4}-\d{2}-\d{2}$/.test(date) ? date : null;
  } catch {
    return null;
  }
}

/** Title from `<title>`; lede from the first paragraph in the body. */
function describePage(source) {
  const title = source.match(/<title>([\s\S]*?)<\/title>/i);
  const bodyAt = source.search(/<body[^>]*>/i);
  const body = bodyAt === -1 ? source : source.slice(bodyAt);
  const lede = body.match(/<p\b[^>]*>([\s\S]*?)<\/p>/i);
  return {
    title: title ? plainText(title[1]) : "",
    lede: lede ? truncate(plainText(lede[1]), LEDE_MAX) : "",
  };
}

/** Title from the README's `# ` heading; lede from its first prose paragraph. */
function describeReadme(source) {
  const lines = source.split("\n");
  const heading = lines.find((line) => line.startsWith("# "));
  const paragraph = [];
  let fenced = false;
  for (const line of lines) {
    if (line.startsWith("```")) {
      fenced = !fenced;
      if (paragraph.length > 0) break;
      continue;
    }
    if (fenced || line.startsWith("#")) continue;
    if (line.trim() === "") {
      if (paragraph.length > 0) break;
      continue;
    }
    paragraph.push(line);
  }
  return {
    title: heading ? heading.slice(2).trim() : "",
    lede: truncate(markdownText(paragraph.join(" ")), LEDE_MAX),
  };
}

// ------------------------------------------------------------------ text ops

/**
 * HTML fragment → collapsed plain text. Tags become spaces so words either
 * side of an inline `<b>` stay apart, which strands a space in front of any
 * punctuation that followed the tag — hence the closing tidy-up.
 */
function plainText(fragment) {
  return decodeEntities(fragment.replace(/<[^>]+>/g, " "))
    .replace(/\s+/g, " ")
    .replace(/\s+([,.;:!?)\]])/g, "$1")
    .replace(/([(\[])\s+/g, "$1")
    .trim();
}

/** Markdown → collapsed plain text: links become their label, marks vanish. */
function markdownText(source) {
  return source
    .replace(/!?\[([^\]]*)\]\([^)]*\)/g, "$1")
    .replace(/[`*_]/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

const NAMED_ENTITIES = {
  amp: "&",
  apos: "'",
  deg: "°",
  gt: ">",
  hellip: "…",
  lt: "<",
  mdash: "—",
  middot: "·",
  nbsp: " ",
  ndash: "–",
  quot: '"',
  rarr: "→",
  times: "×",
};

function decodeEntities(text) {
  return text.replace(/&(#x[0-9a-fA-F]+|#[0-9]+|[a-zA-Z]+);/g, (whole, body) => {
    if (body.startsWith("#x") || body.startsWith("#X")) {
      return String.fromCodePoint(Number.parseInt(body.slice(2), 16));
    }
    if (body.startsWith("#")) {
      return String.fromCodePoint(Number.parseInt(body.slice(1), 10));
    }
    return NAMED_ENTITIES[body] ?? whole;
  });
}

/** Cut at a word boundary so a lede never ends mid-word. */
function truncate(text, max) {
  if (text.length <= max) return text;
  const cut = text.slice(0, max);
  const space = cut.lastIndexOf(" ");
  return `${(space > max * 0.6 ? cut.slice(0, space) : cut).replace(/[\s,;:.—-]+$/, "")}…`;
}

function escapeHtml(text) {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// ------------------------------------------------------------------ rendering

function renderIndex(pages, code) {
  const cards = pages.map(renderCard).join("\n");
  const rows = code.map(renderCodeRow).join("\n");
  return `<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<title>LightPlayer design spikes</title>
<meta name="viewport" content="width=device-width, initial-scale=1">
<!--
  GENERATED FILE — do not edit by hand.
  Source: scripts/spikes-index.mjs (\`just spikes-index\`); \`just lint-spikes-index\`
  fails when this copy has drifted from the spikes it describes.
-->
<style>
  /* ---- studio dark palette (from spikes/wiring-ui) ----------------- */
  :root {
    --bg: #101317;
    --surface: #171b20;
    --raised: #20272e;
    --border: #2a3138;
    --border-muted: #252d34;
    --border-strong: #3e4852;
    --text: #f2f0e8;
    --strong: #fffaf0;
    --muted: #c7cbd0;
    --subtle: #99a2ad;
    --dim: #9ba4ad;
    --heading: #94b8aa;
    --accent: #7be0b2;
    --mono: "SF Mono", ui-monospace, Menlo, monospace;
  }
  * { box-sizing: border-box; }
  body {
    margin: 0; padding: 26px 30px 90px;
    background: var(--bg); color: var(--text);
    font: 14px/1.45 -apple-system, "Segoe UI", sans-serif;
  }
  h1 { font-size: 13px; letter-spacing: .12em; color: var(--muted);
       text-transform: uppercase; margin: 0 0 6px; }
  p.hint { color: var(--dim); font-size: 12px; margin: 0 0 4px; max-width: 760px; }
  h2 { font-size: 12px; letter-spacing: .1em; color: var(--heading);
       text-transform: uppercase; margin: 34px 0 10px; }

  .grid { display: grid; gap: 12px; grid-template-columns: repeat(auto-fill, minmax(320px, 1fr)); }
  a.card {
    display: grid; gap: 6px; align-content: start;
    padding: 14px 15px 15px;
    background: var(--surface); border: 1px solid var(--border); border-radius: 7px;
    color: inherit; text-decoration: none;
    transition: background .12s, border-color .12s, transform .12s;
  }
  a.card:hover { background: var(--raised); border-color: var(--border-strong); transform: translateY(-1px); }
  a.card:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
  .meta { display: flex; align-items: baseline; gap: 10px; }
  .meta .name { font: 500 11px/1 var(--mono); color: var(--accent); letter-spacing: .02em; }
  .meta .date { margin-left: auto; flex: none; font: 400 11px/1 var(--mono); color: var(--subtle); }
  .card .title { font-size: 13.5px; font-weight: 600; color: var(--strong); }
  .card .lede { font-size: 12px; line-height: 1.5; color: var(--dim); }

  ul.code { list-style: none; margin: 0; padding: 0; display: grid; gap: 8px; max-width: 760px; }
  ul.code li {
    padding: 10px 13px;
    background: var(--surface); border: 1px solid var(--border-muted); border-radius: 6px;
  }
  ul.code .name { color: var(--subtle); }
  ul.code .lede { font-size: 12px; color: var(--dim); margin-top: 5px; }

  footer { margin-top: 40px; color: var(--subtle); font-size: 11px; }
  code { font-family: var(--mono); color: var(--dim); }
</style>
</head>
<body>

<h1>LightPlayer design spikes</h1>
<p class="hint">
  Self-contained HTML playgrounds, one per design exploration. They are
  <b>records, not living UI</b> — each is frozen at the state it was judged in,
  and several carry their gate verdicts in the copy, so a spike disagreeing with
  the shipped Studio means the design moved on, not that the spike is broken.
</p>
<p class="hint">
  Newest first. Titles and blurbs are read straight out of each spike, so this
  page cannot describe them wrongly for long; the dates come from
  <code>spikes/dates.json</code>, which records when each spike landed.
</p>

<h2>Playgrounds <span style="color:var(--dim);font-weight:400;letter-spacing:0;text-transform:none">· ${pages.length}</span></h2>
<div class="grid">
${cards}
</div>

<h2>Code spikes <span style="color:var(--dim);font-weight:400;letter-spacing:0;text-transform:none">· ${code.length}, not browsable</span></h2>
<ul class="code">
${rows}
</ul>

<footer>
  Generated by <code>scripts/spikes-index.mjs</code> — regenerate with <code>just spikes-index</code>.
  Served by <code>just studio-dev</code> at <code>/spikes/index.html</code>; never deployed.
</footer>

</body>
</html>
`;
}

function renderCard(spike) {
  return `  <a class="card" href="${escapeHtml(spike.href)}">
    <span class="meta">
      <span class="name">${escapeHtml(spike.name)}</span>
      <span class="date">${escapeHtml(spike.date ?? "undated")}</span>
    </span>
    <span class="title">${escapeHtml(spike.title)}</span>
    <span class="lede">${escapeHtml(spike.lede)}</span>
  </a>`;
}

function renderCodeRow(spike) {
  return `  <li>
    <div class="meta">
      <span class="name">spikes/${escapeHtml(spike.name)}</span>
      <span class="date">${escapeHtml(spike.date ?? "undated")}</span>
    </div>
    <div class="lede">${escapeHtml(spike.lede)}</div>
  </li>`;
}

/**
 * `spikes/dates.json` — one date per spike, sorted by name so the file reads
 * as a lookup table and diffs stay small. Written on every run so a deleted
 * spike stops being listed here too.
 */
function renderDates(spikes) {
  const table = {};
  for (const spike of [...spikes].sort((a, b) => a.name.localeCompare(b.name))) {
    if (spike.date !== null) table[spike.name] = spike.date;
  }
  return `${JSON.stringify(table, null, 2)}\n`;
}

main();
