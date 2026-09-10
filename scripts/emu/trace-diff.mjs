#!/usr/bin/env node
// Diff two device-event traces — an emulated capture against the silicon
// fixture for the same scenario (emulator plan two, M6).
//
//   node scripts/emu/trace-diff.mjs <silicon.jsonl> <emulated.jsonl> [--labels A,B]
//
// THIS IS A TOOL, NOT A PROSE PASS, on purpose. The walk record quotes its
// output, and the whole claim of plan two is that the comparison can be
// re-run when the emulator changes. A diff produced by hand cannot be.
//
// WHAT IS NEVER COMPARED, and why (plan.md's own anti-oracles):
//
//   t          wall clock. A socket is not deterministic and an agent-driven
//              hidden tab is throttled to ~1 Hz — never assert a duration.
//   session    a fresh `rt_<n>` per run; it names nothing across runs.
//   endpoint   `browser-serial-esp32-port-<n>` is a per-page counter.
//   rx/tx COUNTS  a blank board is a continuous byte source and the door
//              replays a board's backlog to its first client (M5 finding:
//              1.39 MB after 2 minutes), so a line count is a measure of
//              when somebody connected, not of what happened.
//
// WHAT IS COMPARED, because it is what a fixture is FOR:
//
//   state      the classifier's verdict about the board, in order. The one
//              thing a golden trace really pins.
//   flow       the readiness engine's path.
//   pool/mgmt  install/adopt actions and management phases, in order.
//   sync       the connect-as-pull outcome (added because of the s6 sitting).
//   anomaly    counts only, per side. Non-zero on one side and not the other
//              is a finding worth its own paragraph
//              (docs/defects/2026-08-02-serial-line-interleaving.md).
//   BOOT-LINE SET  the distinct set of NORMALISED boot lines, not the count.
//              A line the emulator never emits, or emits differently, is
//              exactly the difference the plan wants named. Read out of `rx`
//              records where they exist and unwrapped from journal entries
//              where they do not — see `bootLines`, and the note the output
//              prints whenever it had to bridge the two.
//
// Normalisation for the boot-line set is deliberately blunt and stated in the
// output: runs of digits collapse to `#`, hex literals to `0x#`, so
// `[perf] frame=473355 fps=963` and the next second's copy are one line.
// Everything the normaliser touches is reported as a count, never as a
// difference — a diff that flagged a frame counter would be unreadable.

import { readFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const NEVER_COMPARED = ["t", "session", "endpoint", "rx/tx line counts"];

function load(file) {
  const text = readFileSync(file, "utf8");
  const records = [];
  let malformed = 0;
  for (const line of text.split("\n")) {
    if (!line.trim()) continue;
    try {
      records.push(JSON.parse(line));
    } catch {
      malformed += 1;
    }
  }
  return { file, records, malformed };
}

/// The comparable rendering of one record, or null when the record is one of
/// the kinds this tool deliberately does not line up.
function key(record) {
  switch (record.kind) {
    case "state":
      return `state  ${record.from ?? "·"} → ${record.to}`;
    case "flow":
      return `flow   ${record.from} → ${record.to}`;
    case "pool":
      return `pool   ${record.action} (${record.detail})`;
    case "mgmt":
      return `mgmt   ${record.phase}: ${record.label}`;
    case "sync":
      return `sync   ${record.content}`;
    case "sweep":
      return `sweep  ${record.disposition}`;
    default:
      return null;
  }
}

/// THE BOOT LINES, from either era of the trace format.
///
/// A 2026-08 fixture carries them as `rx` records. A capture taken today
/// carries the same bytes one level down, inside the device model's journal:
/// `Input(Event(Link { link: LinkId(9), event: Line("…") }))`. That is not a
/// difference between silicon and the emulator — it is a difference between
/// two Studios, and it is filed as
/// docs/defects/2026-09-10-eight-of-ten-device-event-kinds-lost-their-producer.md.
///
/// Unwrapping it here is what keeps the comparison the plan actually wants
/// possible at all: the SET of lines a board emits against the set an
/// emulated chip emits. It is stated in the output, every time, so nobody
/// reads a bridged diff as a like-for-like one.
function bootLines(records) {
  const lines = [];
  let fromJournal = 0;
  for (const record of records) {
    if (record.kind === "rx" && typeof record.line === "string") {
      lines.push(record.line);
      continue;
    }
    if (record.kind !== "journal" || typeof record.entry !== "string") continue;
    // `event: Line("…")` — the payload is Rust's Debug escaping of the line.
    const match = record.entry.match(/event: Line\("((?:[^"\\]|\\.)*)"\)/);
    if (!match) continue;
    fromJournal += 1;
    lines.push(
      match[1]
        .replaceAll('\\"', '"')
        .replaceAll("\\n", "\n")
        .replaceAll("\\t", "\t")
        .replaceAll("\\\\", "\\"),
    );
  }
  return { lines, fromJournal };
}

/// Volatile numbers out, so the SET of boot lines is a set of shapes.
function normaliseLine(line) {
  return line
    .replace(/0x[0-9a-fA-F]+/g, "0x#")
    .replace(/\b\d[\d_.]*\b/g, "#")
    .replace(/\s+/g, " ")
    .trim();
}

function census(records) {
  const counts = {};
  for (const record of records) {
    counts[record.kind] = (counts[record.kind] ?? 0) + 1;
  }
  return counts;
}

/// Longest common subsequence over the two comparable-key sequences, walked
/// back into an aligned script of `=` / `-` / `+` rows.
function align(a, b) {
  const n = a.length;
  const m = b.length;
  const table = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i -= 1) {
    for (let j = m - 1; j >= 0; j -= 1) {
      table[i][j] = a[i] === b[j]
        ? table[i + 1][j + 1] + 1
        : Math.max(table[i + 1][j], table[i][j + 1]);
    }
  }
  const rows = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (a[i] === b[j]) {
      rows.push({ op: "=", a: a[i], b: b[j] });
      i += 1;
      j += 1;
    } else if (table[i + 1][j] >= table[i][j + 1]) {
      rows.push({ op: "-", a: a[i], b: null });
      i += 1;
    } else {
      rows.push({ op: "+", a: null, b: b[j] });
      j += 1;
    }
  }
  while (i < n) rows.push({ op: "-", a: a[i++], b: null });
  while (j < m) rows.push({ op: "+", a: null, b: b[j++] });
  return rows;
}

function pad(text, width) {
  return text.length >= width ? text : text + " ".repeat(width - text.length);
}

function sequenceReport(rows, labels) {
  const width = Math.max(labels[0].length, ...rows.map((row) => (row.a ?? "").length));
  const out = [];
  out.push(`  ${" ".repeat(3)}${pad(labels[0], width)}  ${labels[1]}`);
  out.push(`  ${" ".repeat(3)}${"-".repeat(width)}  ${"-".repeat(Math.max(labels[1].length, 20))}`);
  for (const row of rows) {
    const mark = row.op === "=" ? "   " : ` ${row.op} `;
    out.push(`  ${mark}${pad(row.a ?? "", width)}  ${row.b ?? ""}`);
  }
  return out.join("\n");
}

function setDifference(a, b) {
  const only = [];
  for (const item of a) if (!b.has(item)) only.push(item);
  only.sort();
  return only;
}

function main() {
  const args = process.argv.slice(2);
  let labels = ["silicon", "emulated"];
  const files = [];
  for (let index = 0; index < args.length; index += 1) {
    if (args[index] === "--labels") {
      labels = args[index + 1].split(",");
      index += 1;
    } else {
      files.push(args[index]);
    }
  }
  if (files.length !== 2) {
    console.error("usage: node scripts/emu/trace-diff.mjs <a.jsonl> <b.jsonl> [--labels A,B]");
    process.exit(2);
  }

  const [left, right] = files.map(load);
  const differences = [];

  console.log("");
  console.log(`trace-diff  ${labels[0]}: ${path.basename(left.file)}  (${left.records.length} records)`);
  console.log(`            ${labels[1]}: ${path.basename(right.file)}  (${right.records.length} records)`);
  if (left.malformed || right.malformed) {
    console.log(`  ⚠️ malformed lines: ${left.malformed} / ${right.malformed}`);
  }
  console.log("");
  console.log(`  NOT COMPARED (by design): ${NEVER_COMPARED.join(", ")}`);
  console.log("");

  const leftCensus = census(left.records);
  const rightCensus = census(right.records);
  const kinds = [...new Set([...Object.keys(leftCensus), ...Object.keys(rightCensus)])].sort();
  console.log("  record census");
  for (const kind of kinds) {
    console.log(`    ${pad(kind, 9)} ${String(leftCensus[kind] ?? 0).padStart(6)}  ${String(rightCensus[kind] ?? 0).padStart(6)}`);
  }
  console.log("");

  // --- the lifecycle sequences, aligned --------------------------------
  for (const kind of ["state", "flow", "pool", "mgmt", "sync"]) {
    const a = left.records.filter((r) => r.kind === kind).map(key);
    const b = right.records.filter((r) => r.kind === kind).map(key);
    if (a.length === 0 && b.length === 0) continue;
    const rows = align(a, b);
    const changed = rows.filter((row) => row.op !== "=");
    console.log(`  ${kind} sequence — ${changed.length === 0 ? "IDENTICAL" : `${changed.length} difference(s)`}`);
    console.log(sequenceReport(rows, labels));
    console.log("");
    for (const row of changed) {
      differences.push({
        area: `${kind} sequence`,
        detail: row.op === "-" ? `only ${labels[0]}: ${row.a}` : `only ${labels[1]}: ${row.b}`,
      });
    }
  }

  // --- anomalies ---------------------------------------------------------
  const leftAnomalies = left.records.filter((r) => r.kind === "anomaly");
  const rightAnomalies = right.records.filter((r) => r.kind === "anomaly");
  console.log(`  anomaly count  ${labels[0]}=${leftAnomalies.length}  ${labels[1]}=${rightAnomalies.length}`);
  if (leftAnomalies.length !== rightAnomalies.length) {
    differences.push({
      area: "anomaly count",
      detail: `${labels[0]}=${leftAnomalies.length} vs ${labels[1]}=${rightAnomalies.length}`,
    });
    const details = new Set([...leftAnomalies, ...rightAnomalies].map((r) => normaliseLine(r.detail ?? "")));
    for (const detail of details) console.log(`    seen: ${detail}`);
  }
  console.log("");

  // --- the distinct set of boot lines ------------------------------------
  const leftBoot = bootLines(left.records);
  const rightBoot = bootLines(right.records);
  const leftLines = new Set(leftBoot.lines.map(normaliseLine));
  const rightLines = new Set(rightBoot.lines.map(normaliseLine));
  const onlyLeft = setDifference(leftLines, rightLines);
  const onlyRight = setDifference(rightLines, leftLines);
  const shared = [...leftLines].filter((line) => rightLines.has(line)).length;
  if (leftBoot.fromJournal || rightBoot.fromJournal) {
    console.log(
      `  (boot lines UNWRAPPED FROM THE JOURNAL: ${labels[0]}=${leftBoot.fromJournal}, ${labels[1]}=${rightBoot.fromJournal}.\n` +
        `   A 2026-08 fixture carries them as \`rx\` records; a capture taken today carries the same\n` +
        `   bytes inside journal entries, because \`rx\` lost its producer — that is two Studios,\n` +
        `   not two boards. docs/defects/2026-09-10-eight-of-ten-device-event-kinds-lost-their-producer.md)`,
    );
  }
  console.log(`  distinct boot-line SHAPES (digits → #):  ${labels[0]}=${leftLines.size}  ${labels[1]}=${rightLines.size}  shared=${shared}`);
  console.log(`    only in ${labels[0]}: ${onlyLeft.length}`);
  for (const line of onlyLeft) console.log(`      - ${line}`);
  console.log(`    only in ${labels[1]}: ${onlyRight.length}`);
  for (const line of onlyRight) console.log(`      + ${line}`);
  console.log("");
  if (onlyLeft.length) differences.push({ area: "boot-line set", detail: `${onlyLeft.length} shape(s) only ${labels[0]}` });
  if (onlyRight.length) differences.push({ area: "boot-line set", detail: `${onlyRight.length} shape(s) only ${labels[1]}` });

  console.log(`  VERDICT: ${differences.length} difference(s) to name.`);
  for (const difference of differences) {
    console.log(`    · ${difference.area}: ${difference.detail}`);
  }
  console.log("");
  // Always 0: this tool REPORTS, it does not judge. Naming and classifying
  // every difference is the walk record's job, and a tool that failed a
  // build on a difference would push the next agent to loosen it.
  process.exit(0);
}

main();
