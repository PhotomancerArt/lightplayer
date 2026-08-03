#!/usr/bin/env node
// The guided device-scenario runner (multi-device roadmap M8).
//
// Golden-trace capture sittings: each scenario is a spec under
// scripts/device-scenarios/<id>.json declaring how to put a board into a
// KNOWN state (automated setup where possible, exact manual steps where
// not), and what the captured trace must contain to count. The runner
//
//   1. shows a status table (`just device-scenario`),
//   2. runs a scenario (`just device-scenario run <id> [--port </dev/...>]`):
//      preconditions → setup (foreground) → hand-off checklist → a local
//      HTTP capture sink that Studio streams device events to (M0's
//      `?capture-sink=` mode) → writes the JSONL to
//      lp-app/lpa-link/testdata/device-traces/<id>.jsonl as it arrives →
//      validates the capture against the spec's `expect` list on finish.
//
// Design rules baked in (they have each broken a sitting before):
//  - NOTHING touches the serial port after hand-off — setup runs strictly
//    before the browser takes the port, and the runner refuses to run
//    setup while a capture is streaming.
//  - Setup commands run in the FOREGROUND with inherited stdio (a
//    backgrounded espflash has died silently mid-write).
//  - The runner never starts or re-ports the Studio dev server; it prints
//    the URL of the one `just studio-dev` is already serving.

import { readdirSync, readFileSync, existsSync, statSync, mkdirSync, appendFileSync, writeFileSync, renameSync, rmSync } from "node:fs";
import { createServer } from "node:http";
import { spawnSync, execSync } from "node:child_process";
import { createInterface } from "node:readline";
import path from "node:path";
import process from "node:process";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const SPEC_DIR = path.join(ROOT, "scripts", "device-scenarios");
const TRACE_DIR = path.join(ROOT, "lp-app", "lpa-link", "testdata", "device-traces");

function loadScenarios() {
  return readdirSync(SPEC_DIR)
    .filter((name) => name.endsWith(".json"))
    .sort()
    .map((name) => {
      const spec = JSON.parse(readFileSync(path.join(SPEC_DIR, name), "utf8"));
      if (!spec.id || !spec.title || !Array.isArray(spec.expect)) {
        throw new Error(`spec ${name} is missing id/title/expect`);
      }
      return spec;
    });
}

function tracePath(id) {
  return path.join(TRACE_DIR, `${id}.jsonl`);
}

function captureStatus(id) {
  const file = tracePath(id);
  if (!existsSync(file)) return { state: "missing" };
  const stat = statSync(file);
  if (stat.size === 0) return { state: "empty" };
  return { state: "captured", when: stat.mtime.toISOString().slice(0, 16).replace("T", " ") };
}

function printStatus(scenarios) {
  console.log("\nDevice scenarios — golden-trace capture status\n");
  const rows = scenarios.map((s) => {
    const cap = captureStatus(s.id);
    const setup = s.setup?.length
      ? s.setup.every((step) => step.verified) ? "scripted" : "scripted (unverified)"
      : "procedure only";
    const status = cap.state === "captured" ? `captured ${cap.when}` : cap.state;
    return [s.id, status, setup, s.board, s.title];
  });
  const widths = [0, 0, 0, 0];
  for (const row of rows) for (let i = 0; i < 4; i++) widths[i] = Math.max(widths[i], row[i].length);
  for (const row of rows) {
    console.log(
      `  ${row[0].padEnd(widths[0])}  ${row[1].padEnd(widths[1])}  ${row[2].padEnd(widths[2])}  ${row[3].padEnd(widths[3])}  ${row[4]}`,
    );
  }
  console.log(`\nRun one with: just device-scenario run <id> [--port /dev/cu.usbmodemXXXX]`);
  console.log(`Captures land in ${path.relative(ROOT, TRACE_DIR)}/ (commit them — they are fixtures).\n`);
}

function ask(question) {
  const rl = createInterface({ input: process.stdin, output: process.stdout });
  return new Promise((resolve) => {
    let settled = false;
    const settle = (answer) => {
      if (settled) return;
      settled = true;
      rl.close();
      resolve(answer.trim());
    };
    rl.question(question, settle);
    // A closed stdin (piped runs, Ctrl-D) must end the prompt, not hang it.
    rl.on("close", () => settle(""));
  });
}

function listPorts() {
  // Passive by design: `hardware list` never opens a port and cannot hang
  // or reset a board. NEVER swap this for a --probe.
  try {
    const out = execSync("cargo run -q -p lp-cli -- hardware list --json", { cwd: ROOT, encoding: "utf8", stdio: ["ignore", "pipe", "inherit"] });
    return JSON.parse(out);
  } catch {
    return [];
  }
}

async function pickPort(argPort) {
  if (argPort) return argPort;
  console.log("\nListing attached serial ports (passive — nothing is opened or reset;");
  console.log("first run may take a couple of minutes while lp-cli builds)…\n");
  const ports = listPorts();
  if (ports.length === 0) {
    console.log("  (no ports found — is a board plugged in?)");
    const answer = await ask("\nPaste the /dev/... path, or Enter to skip: ");
    return answer || null;
  }
  for (const [index, port] of ports.entries()) {
    const serial = port.serial_number ? ` · ${port.serial_number}` : "";
    console.log(`  ${index + 1}. ${port.port}  (${port.kind}${serial})`);
  }
  const answer = await ask("\nWhich port is the scenario board on? (number, /dev/... path, or Enter to skip): ");
  if (!answer) return null;
  const number = Number.parseInt(answer, 10);
  if (Number.isInteger(number) && number >= 1 && number <= ports.length) {
    return ports[number - 1].port;
  }
  return answer;
}

function runSetup(spec, port) {
  for (const [index, step] of (spec.setup ?? []).entries()) {
    console.log(`\n— setup ${index + 1}/${spec.setup.length}: ${step.describe}`);
    if (!step.run) {
      console.log("  (manual step — do it now, then continue)");
      continue;
    }
    const command = step.run.replaceAll("{port}", port ?? "");
    if (command.includes("{port}") || (step.run.includes("{port}") && !port)) {
      console.log("  ⚠️ needs a port and none was given — run it yourself:");
      console.log(`     ${command}`);
      continue;
    }
    if (!step.verified) {
      console.log("  ⚠️ first-run command (unverified) — watch it, and fix the spec if it is wrong:");
    }
    console.log(`  $ ${command}`);
    const result = spawnSync("bash", ["-c", command], { cwd: ROOT, stdio: "inherit" });
    if (result.status !== 0) {
      console.error(`\nSetup step failed (exit ${result.status}). Fix it and re-run; nothing was captured.`);
      console.error("If espflash wedged the port, a physical replug is the only reliable release (S3 rule).");
      process.exit(1);
    }
  }
}

function summarize(records) {
  const lines = [];
  let anomalies = 0;
  for (const record of records) {
    if (record.kind === "state") lines.push(`  state  ${record.from ?? "·"} → ${record.to}`);
    else if (record.kind === "flow") lines.push(`  flow   ${record.from} → ${record.to}`);
    else if (record.kind === "pool") lines.push(`  pool   ${record.action} (${record.detail})`);
    else if (record.kind === "mgmt") lines.push(`  mgmt   ${record.phase}: ${record.label}`);
    else if (record.kind === "anomaly") anomalies += 1;
  }
  if (anomalies) lines.push(`  ⚠️ ${anomalies} parse anomalies (serial-interleaving evidence — see docs/defects/2026-08-02-serial-line-interleaving.md)`);
  return lines.join("\n") || "  (no lifecycle events captured)";
}

function validate(spec, records) {
  const failures = [];
  for (const expectation of spec.expect) {
    // "state:ready" or alternatives "a|b" — any match passes.
    const ok = expectation.split("|").some((alt) => {
      const [kind, value] = alt.split(":");
      return records.some((record) => {
        if (record.kind !== kind) return false;
        if (!value) return true;
        return record.to === value || record.action === value || record.disposition === value
          || record.phase === value || record.from === value;
      });
    });
    if (!ok) failures.push(expectation);
  }
  return failures;
}

/// Resolve a scenario by exact id, short id ("s2" → "s2-fresh-fw-no-lpfs";
/// first-segment equality keeps "s1" from colliding with "s10"), or a
/// unique prefix.
function resolveScenario(scenarios, query) {
  const exact = scenarios.find((s) => s.id === query);
  if (exact) return exact;
  const bySegment = scenarios.filter((s) => s.id.split("-")[0] === query);
  if (bySegment.length === 1) return bySegment[0];
  const byPrefix = scenarios.filter((s) => s.id.startsWith(query));
  if (byPrefix.length === 1) return byPrefix[0];
  const candidates = bySegment.length > 1 ? bySegment : byPrefix;
  if (candidates.length > 1) {
    console.error(`'${query}' is ambiguous:`);
    for (const s of candidates) console.error(`  ${s.id}`);
  } else {
    console.error(`Unknown scenario '${query}'. Known:`);
    for (const s of scenarios) console.error(`  ${s.id}`);
  }
  process.exit(1);
}

async function runScenario(id, argPort) {
  const scenarios = loadScenarios();
  const spec = resolveScenario(scenarios, id);
  console.log(`\n=== ${spec.id} — ${spec.title}`);
  console.log(`Board: ${spec.board}`);
  for (const dep of spec.needs ?? []) {
    console.log(`Builds on: ${dep} (make sure the board is in that state, or run it first)`);
  }

  const port = await pickPort(argPort);
  runSetup(spec, port);

  // The capture sink: Studio (M0 capture mode) POSTs JSONL batches here;
  // every line is appended AS IT ARRIVES, so a crash or an abandoned
  // sitting still leaves everything received so far. It appends to a
  // .partial file and only replaces the real capture on a non-empty
  // finish — re-running a scenario and aborting must never destroy a
  // previous golden trace (nearly happened 2026-08-03: a dry run
  // truncated the fixture before anything arrived).
  mkdirSync(TRACE_DIR, { recursive: true });
  const file = tracePath(spec.id);
  const partial = `${file}.partial`;
  writeFileSync(partial, "");
  const records = [];
  const sink = createServer((request, response) => {
    let body = "";
    request.on("data", (chunk) => { body += chunk; });
    request.on("end", () => {
      const lines = body.split("\n").filter((line) => line.trim().length > 0);
      for (const line of lines) {
        try { records.push(JSON.parse(line)); } catch { /* keep raw anyway */ }
      }
      if (lines.length) appendFileSync(partial, lines.join("\n") + "\n");
      response.writeHead(204, { "access-control-allow-origin": "*" });
      response.end();
    });
  });
  await new Promise((resolve) => sink.listen(0, "127.0.0.1", resolve));
  const sinkUrl = `http://127.0.0.1:${sink.address().port}/ingest`;

  console.log("\n— hand-off: the port is now FREE; nothing here will touch it again.");
  console.log("  Studio dev server: use the URL `just studio-dev` printed (never re-port it),");
  console.log("  and open it with the capture parameter appended:\n");
  console.log(`    <studio-url>/?capture-sink=${encodeURIComponent(sinkUrl)}\n`);
  console.log("— now, in the browser:");
  for (const [index, step] of (spec.manual ?? []).entries()) {
    console.log(`  ${index + 1}. ${step}`);
  }
  await ask("\nPress Enter here when the scenario is done… ");
  sink.close();

  if (records.length > 0) {
    renameSync(partial, file);
  } else {
    rmSync(partial, { force: true });
    if (existsSync(file)) {
      console.log(`\n(nothing arrived — the previous capture at ${path.relative(ROOT, file)} is untouched)`);
    }
  }
  console.log(`\nCaptured ${records.length} events → ${path.relative(ROOT, file)}`);
  console.log("\nWhat the trace says happened:");
  console.log(summarize(records));
  const failures = validate(spec, records);
  if (records.length === 0) {
    console.log("\n✗ nothing arrived — was the ?capture-sink= parameter on the URL, and is M0's capture mode in this build?");
    process.exitCode = 1;
  } else if (failures.length) {
    console.log(`\n✗ capture is missing expected evidence: ${failures.join(", ")}`);
    console.log("  Keep it if the run itself was right and the expectation is wrong — then fix the spec.");
    process.exitCode = 1;
  } else {
    console.log("\n✓ capture validates — commit the trace file (it is a fixture, not a story PNG).");
  }
}

const [, , command, maybeId, ...rest] = process.argv;
const portFlag = (() => {
  const all = [maybeId, ...rest];
  const index = all.indexOf("--port");
  return index >= 0 ? all[index + 1] : null;
})();

if (!command || command === "status" || command === "list") {
  printStatus(loadScenarios());
} else if (command === "run" && maybeId && maybeId !== "--port") {
  await runScenario(maybeId, portFlag);
} else {
  console.error("usage: just device-scenario            # status table");
  console.error("       just device-scenario run <id> [--port /dev/cu.usbmodemXXXX]");
  process.exit(2);
}
