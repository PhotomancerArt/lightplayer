#!/usr/bin/env node
// The guided device-scenario runner (multi-device roadmap M8).
//
// Golden-trace capture sittings: each scenario is a spec under
// scripts/device-scenarios/<id>.json declaring how to put a board into a
// KNOWN state (automated setup where possible, exact manual steps where
// not), and what the captured trace must contain to count. The runner
//
//   1. shows a status table (`just device-scenario`),
//   2. runs THE SITTING (`just device-scenario run [id]`): ensures Studio is
//      serving on this worktree's canonical port (reusing a live server or
//      starting `just studio-dev` itself), opens ONE browser tab whose
//      `?capture-sink=` streams to ONE persistent local sink, then loops
//      scenarios — setup (foreground) → printed steps → capture → validate —
//      until `q`. Enter-through is the happy path: next uncaptured scenario,
//      first serial port, same tab.
//
// Design rules baked in (they have each broken a sitting before):
//  - NOTHING touches the serial port during a capture — setup runs strictly
//    before the browser takes the port, and between scenarios the user is
//    told to Disconnect (the card's Danger tab) before port-using setup.
//  - Setup commands run in the FOREGROUND with inherited stdio (a
//    backgrounded espflash has died silently mid-write).
//  - The runner never uses a NON-canonical port — it reuses or starts the
//    worktree's own `just studio-dev`, never a substitute server
//    (docs/defects/2026-07-27-launch-json-pinned-port.md).
//  - A re-run that captures nothing never destroys a previous fixture
//    (.partial swap on non-empty finish only).

import { readdirSync, readFileSync, existsSync, statSync, mkdirSync, appendFileSync, writeFileSync, renameSync, rmSync, openSync } from "node:fs";
import { createServer } from "node:http";
import { spawn, spawnSync, execSync } from "node:child_process";
import { createInterface } from "node:readline";
import path from "node:path";
import { tmpdir } from "node:os";
import process from "node:process";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "..");
const SPEC_DIR = path.join(ROOT, "scripts", "device-scenarios");
const TRACE_DIR = path.join(ROOT, "lp-app", "lpa-link", "testdata", "device-traces");

function loadScenarios() {
  // Numeric order (s2 before s10), not lexicographic.
  const ordinal = (name) => {
    const match = name.match(/^s(\d+)/);
    return match ? Number(match[1]) : Number.MAX_SAFE_INTEGER;
  };
  return readdirSync(SPEC_DIR)
    .filter((name) => name.endsWith(".json"))
    .sort((a, b) => ordinal(a) - ordinal(b) || a.localeCompare(b))
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
  if (existsSync(file) && statSync(file).size > 0) {
    return { state: "captured", when: statSync(file).mtime.toISOString().slice(0, 16).replace("T", " ") };
  }
  // A filed FINDING (the run happened but did not do what the spec
  // expects) is a visible state of its own — not silently "missing".
  const failed = path.join(TRACE_DIR, `${id}.failed.jsonl`);
  if (existsSync(failed)) {
    return { state: "finding", when: statSync(failed).mtime.toISOString().slice(0, 16).replace("T", " ") };
  }
  return { state: "missing" };
}

function printStatus(scenarios) {
  console.log("\nDevice scenarios — golden-trace capture status\n");
  const rows = scenarios.map((s) => {
    const cap = captureStatus(s.id);
    const setup = s.setup?.length
      ? s.setup.every((step) => step.verified) ? "scripted" : "scripted (unverified)"
      : "procedure only";
    const status =
      cap.state === "captured" ? `captured ${cap.when}`
      : cap.state === "finding" ? `✗ finding ${cap.when} (FINDINGS.md)`
      : cap.state;
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
    // Ctrl-C means QUIT, not "empty answer": without this, readline just
    // closes the prompt, the close-handler resolved "", and the sitting
    // marched forward as if Enter had been pressed (2026-08-03). Any
    // in-flight capture is safe — it lives in the .partial until a
    // deliberate finish.
    rl.on("SIGINT", () => {
      console.log("\n(interrupted — partial captures are preserved as .partial files)");
      process.exit(130);
    });
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
  // Enter = the first port: the happy path is Enter-through.
  const answer = await ask("\nWhich port is the scenario board on? (number [1], /dev/... path, or 's' to skip): ");
  if (!answer) return ports[0].port;
  if (answer === "s") return null;
  const number = Number.parseInt(answer, 10);
  if (Number.isInteger(number) && number >= 1 && number <= ports.length) {
    return ports[number - 1].port;
  }
  return answer;
}

// --- Studio lifecycle -----------------------------------------------------
//
// The runner owns the whole loop (sitting feedback, 2026-08-03: "we should
// probably have this app start its own studio… just pressing enter should
// get you through the happy path"). It reuses a dev server already on this
// worktree's canonical port, or starts `just studio-dev` itself — NEVER a
// different port (docs/defects/2026-07-27-launch-json-pinned-port.md).

function studioPort() {
  return execSync('bash scripts/dev-port.sh --query studio-dev "${STUDIO_WEB_PORT:-}"', {
    cwd: ROOT,
    encoding: "utf8",
  }).trim();
}

async function studioUp(port) {
  try {
    const response = await fetch(`http://localhost:${port}/`, { signal: AbortSignal.timeout(1500) });
    return response.ok;
  } catch {
    return false;
  }
}

/// Who is holding a TCP port (pids; possibly several with many studios
/// running across worktrees — the port hash makes collisions unlikely,
/// not impossible, and a STALE server from an old build is the real trap:
/// it would silently lack capture mode).
function portHolders(port) {
  try {
    const out = execSync(`lsof -ti :${port}`, { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
    return out.split("\n").map((s) => s.trim()).filter(Boolean);
  } catch {
    return [];
  }
}

async function ensureStudio() {
  const port = studioPort();
  if (await studioUp(port)) {
    const holders = portHolders(port);
    const who = holders
      .map((pid) => {
        try {
          return `${pid} (${execSync(`ps -p ${pid} -o comm=`, { encoding: "utf8" }).trim()})`;
        } catch {
          return pid;
        }
      })
      .join(", ");
    console.log(`\nSomething is already serving on this worktree's port ${port}: ${who || "unknown"}`);
    console.log("If it predates the current build, capture streaming will silently not exist in it.");
    const answer = await ask("[Enter] reuse it · r = restart it fresh: ");
    if (answer !== "r") {
      return { port, startedPid: null };
    }
    for (const pid of holders) {
      try { process.kill(Number(pid), "SIGTERM"); } catch { /* already gone */ }
    }
    await new Promise((resolve) => setTimeout(resolve, 1500));
  }
  const log = path.join(tmpdir(), `studio-dev-${port}.log`);
  const fd = openSync(log, "a");
  const child = spawn("just", ["studio-dev"], {
    cwd: ROOT,
    detached: true,
    stdio: ["ignore", fd, fd],
  });
  child.unref();
  process.stdout.write(
    `\nStarting Studio (just studio-dev, port ${port}) — a cold wasm build can take ~10 min.` +
    `\nBuild log: ${log}\nWaiting`,
  );
  for (;;) {
    if (await studioUp(port)) break;
    try {
      process.kill(child.pid, 0);
    } catch {
      console.error(`\n\nStudio failed to start — tail of ${log}:`);
      try {
        console.error(readFileSync(log, "utf8").split("\n").slice(-15).join("\n"));
      } catch { /* log unreadable */ }
      return null;
    }
    process.stdout.write(".");
    await new Promise((resolve) => setTimeout(resolve, 3000));
  }
  console.log(" up.");
  return { port, startedPid: child.pid, log };
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
      return false;
    }
  }
  return true;
}

function summarize(records) {
  const lines = [];
  let anomalies = 0;
  for (const record of records) {
    if (record.kind === "state") lines.push(`  state  ${record.from ?? "·"} → ${record.to}`);
    else if (record.kind === "flow") lines.push(`  flow   ${record.from} → ${record.to}`);
    else if (record.kind === "pool") lines.push(`  pool   ${record.action} (${record.detail})`);
    else if (record.kind === "mgmt") lines.push(`  mgmt   ${record.phase}: ${record.label}`);
    else if (record.kind === "sync") lines.push(`  sync   ${record.content}`);
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
          || record.phase === value || record.from === value || record.content === value;
      });
    });
    if (!ok) failures.push(expectation);
  }
  return failures;
}

/// Resolve a scenario by exact id, short id ("s2" → "s2-fresh-fw-no-lpfs";
/// first-segment equality keeps "s1" from colliding with "s10"), a unique
/// prefix, or a 1-based menu number. `null` (with the reason printed) lets
/// the session loop re-prompt instead of exiting.
function resolveScenario(scenarios, query) {
  const number = Number.parseInt(query, 10);
  if (Number.isInteger(number) && String(number) === query && number >= 1 && number <= scenarios.length) {
    return scenarios[number - 1];
  }
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
    console.error(`Unknown scenario '${query}'.`);
  }
  return null;
}

/// The in-session scenario menu. Enter = the first scenario without a
/// capture (the happy path walks the matrix in order).
async function menuPick(scenarios) {
  console.log("\nScenarios:");
  const firstMissing = scenarios.findIndex((s) => captureStatus(s.id).state !== "captured");
  for (const [index, s] of scenarios.entries()) {
    const cap = captureStatus(s.id);
    const mark = cap.state === "captured" ? "✓" : cap.state === "finding" ? "✗" : " ";
    const hint = index === firstMissing ? "  ← Enter" : "";
    console.log(`  ${index + 1}. ${mark} ${s.id} — ${s.title}${hint}`);
  }
  const fallback = firstMissing >= 0 ? scenarios[firstMissing] : null;
  const answer = await ask("\nScenario (number/id, Enter = next uncaptured, q = quit): ");
  if (answer === "q") return "quit";
  if (!answer) return fallback ?? "quit";
  return resolveScenario(scenarios, answer);
}

/// One persistent capture sink for the whole sitting: the browser tab is
/// opened ONCE with the sink URL and keeps streaming; the runner just
/// switches which scenario's .partial the events land in. Events arriving
/// between scenarios are counted and dropped.
function startSink() {
  const state = { active: null, dropped: 0 };
  const sink = createServer((request, response) => {
    let body = "";
    request.on("data", (chunk) => { body += chunk; });
    request.on("end", () => {
      const lines = body.split("\n").filter((line) => line.trim().length > 0);
      if (state.active) {
        for (const line of lines) {
          try { state.active.records.push(JSON.parse(line)); } catch { /* keep raw anyway */ }
        }
        if (lines.length) appendFileSync(state.active.partial, lines.join("\n") + "\n");
      } else {
        state.dropped += lines.length;
      }
      response.writeHead(204, { "access-control-allow-origin": "*" });
      response.end();
    });
  });
  return { sink, state };
}

/// A scenario run that did NOT do what its spec expects is EVIDENCE, not
/// garbage (sitting feedback, 2026-08-03: "when the scenario fails we need
/// a way of indicating that for you to look at later"): the trace moves to
/// <id>.failed.jsonl and FINDINGS.md gets an entry an agent can pick up.
/// Failed traces are never golden fixtures — the replay test skips them.
function fileFinding(spec, records, failures, partial, note) {
  const failed = path.join(TRACE_DIR, `${spec.id}.failed.jsonl`);
  renameSync(partial, failed);
  const findings = path.join(TRACE_DIR, "FINDINGS.md");
  const entry = [
    `## ${new Date().toISOString()} — ${spec.id}`,
    ``,
    `- Expected: ${spec.expect.join(", ")} (missing: ${failures.join(", ")})`,
    `- Observed (trace summary):`,
    summarize(records).split("\n").map((line) => `  ${line.trim() ? "- " + line.trim() : ""}`).filter(Boolean).join("\n"),
    note ? `- Note: ${note}` : null,
    `- Trace: ${path.basename(failed)} (${records.length} events)`,
    ``,
  ].filter((line) => line !== null).join("\n");
  appendFileSync(findings, entry + "\n");
  console.log(`finding filed → ${path.relative(ROOT, findings)} (trace kept as ${path.basename(failed)})`);
}

/// Run one scenario inside the sitting: setup (with the port-held guard),
/// hand-off steps, capture, validate. Returns false only when setup
/// failed (the session continues either way).
async function runOne(spec, state, argPort) {
  console.log(`\n=== ${spec.id} — ${spec.title}`);
  console.log(`Board: ${spec.board}`);
  for (const dep of spec.needs ?? []) {
    console.log(`Builds on: ${dep} (make sure the board is in that state, or run it first)`);
  }

  // Only scenarios whose setup actually touches the serial port need a
  // port — and they need it FREE: with the session's Studio tab open, a
  // connected board holds the port, so the user must Disconnect first
  // (the card's Danger tab — that affordance exists for exactly this).
  const needs_port = (spec.setup ?? []).some((step) => step.run?.includes("{port}"));
  if (needs_port) {
    await ask(
      "\nSetup is about to use the serial port. If this board is connected in \n" +
      "Studio, Disconnect it first (card → Danger tab → Disconnect). Enter when free… ",
    );
    const port = await pickPort(argPort);
    if (!runSetup(spec, port)) {
      return false;
    }
  } else if ((spec.setup ?? []).length > 0) {
    runSetup(spec, null);
  }

  mkdirSync(TRACE_DIR, { recursive: true });
  const file = tracePath(spec.id);
  const partial = `${file}.partial`;
  writeFileSync(partial, "");
  const records = [];
  state.active = { records, partial };

  console.log("\n— in the Studio tab (the one this sitting opened — it keeps streaming):");
  for (const [index, step] of (spec.manual ?? []).entries()) {
    console.log(`  ${index + 1}. ${step}`);
  }
  await ask("\nPress Enter here when the scenario is done… ");
  state.active = null;

  console.log(`\nCaptured ${records.length} events.`);
  console.log("\nWhat the trace says happened:");
  console.log(summarize(records));

  // Validate BEFORE the fixture swap: a failing capture defaults to
  // DISCARD, leaving any previous golden trace untouched (2026-08-03: an
  // s4 attempt without the old firmware on hand captured a healthy boot —
  // a fixture that lies is worse than a missing one).
  const failures = validate(spec, records);
  if (records.length === 0) {
    rmSync(partial, { force: true });
    console.log("\n✗ nothing arrived — is the sitting's tab open (with ?capture-sink=), and did the scenario touch the device?");
    if (existsSync(file)) {
      console.log(`  (the previous capture at ${path.relative(ROOT, file)} is untouched)`);
    }
  } else if (failures.length) {
    console.log(`\n✗ capture is missing expected evidence: ${failures.join(", ")}`);
    const answer = await ask(
      "[Enter] file it as a FINDING (evidence kept for later; fixture untouched) · " +
      "k = keep as the golden fixture (the spec is wrong) · d = discard: ",
    );
    if (answer === "k") {
      renameSync(partial, file);
      console.log(`kept → ${path.relative(ROOT, file)} — now fix the spec's expect list.`);
    } else if (answer === "d") {
      rmSync(partial, { force: true });
      console.log("discarded.");
    } else {
      const note = await ask("One line on what actually happened (for the finding): ");
      fileFinding(spec, records, failures, partial, note);
    }
  } else {
    renameSync(partial, file);
    console.log(`\n✓ capture validates → ${path.relative(ROOT, file)} — commit it (a fixture, not a story PNG).`);
  }
  return true;
}

/// The sitting: Studio up (reused or started), ONE sink + ONE browser tab,
/// then scenarios in a loop until `q`. Enter-through walks the happy path:
/// next uncaptured scenario, first serial port, same tab.
async function sitting(initialId, argPort) {
  const scenarios = loadScenarios();
  const studio = await ensureStudio();
  if (!studio) {
    process.exit(1);
  }
  const { sink, state } = startSink();
  await new Promise((resolve) => sink.listen(0, "127.0.0.1", resolve));
  const sinkUrl = `http://127.0.0.1:${sink.address().port}/ingest`;
  const studioUrl = `http://localhost:${studio.port}/?capture-sink=${encodeURIComponent(sinkUrl)}`;
  console.log(`\nStudio, capture armed:\n\n    ${studioUrl}\n`);
  if (process.platform === "darwin" && process.stdout.isTTY) {
    spawnSync("open", [studioUrl]);
    console.log("(opened in your browser — keep using that ONE tab for every scenario)");
  }

  let pending = initialId ?? null;
  for (;;) {
    let spec;
    if (pending) {
      spec = resolveScenario(scenarios, pending);
      pending = null;
      if (!spec) continue;
    } else {
      spec = await menuPick(scenarios);
      if (spec === "quit") break;
      if (!spec) continue;
    }
    await runOne(spec, state, argPort);
    const next = await ask("\nNext scenario (number/id, Enter = menu, q = quit): ");
    if (next === "q") break;
    pending = next || null;
  }

  sink.close();
  if (state.dropped > 0) {
    console.log(`(${state.dropped} events arrived between scenarios and were dropped)`);
  }
  if (studio.startedPid) {
    console.log(
      `\nStudio (started by this sitting) is still serving on port ${studio.port} — ` +
      `leave it for more work, or stop it with: kill ${studio.startedPid}`,
    );
  }
  printStatus(scenarios);
}

const [, , command, maybeId, ...rest] = process.argv;
const portFlag = (() => {
  const all = [maybeId, ...rest];
  const index = all.indexOf("--port");
  return index >= 0 ? all[index + 1] : null;
})();

if (!command || command === "status" || command === "list") {
  printStatus(loadScenarios());
} else if (command === "run") {
  const initial = maybeId && maybeId !== "--port" ? maybeId : null;
  await sitting(initial, portFlag);
} else {
  console.error("usage: just device-scenario            # status table");
  console.error("       just device-scenario run [id]   # the sitting (Enter-through happy path)");
  console.error("                [--port /dev/cu.usbmodemXXXX]");
  process.exit(2);
}
