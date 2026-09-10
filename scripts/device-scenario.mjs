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
//      starting `just studio-dev` itself), then loops scenarios until `q` —
//      setup (foreground, capture tab CLOSED so the port is free) → the tab
//      opens at hand-off with `?capture-sink=` streaming to one persistent
//      sink → printed steps → capture → validate. Enter-through is the
//      happy path: next uncaptured scenario, first serial port.
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
//
// TWO LANES (emulator plan two, M6). The SILICON lane above is unchanged. The
// EMULATED lane (`--emu`) runs the same scenarios with no board: `setup:`
// becomes an `lp-cli emu serve` holding boards on a fresh state dir, and
// `manual:` becomes the spec's `emulated.steps`, driven in headless Chrome.
// See scripts/emu/emulated-lane.mjs, and README.md's "The emulated lane".
//
//  - THE GUARD (the single most important rule in the emulated lane): an
//    emulated run can only ever write a name containing `.emu.`. It is code —
//    `assertLaneMayWrite` below, called at every write site — not a
//    convention, because a board's bytes are not reproducible and a
//    clobbered silicon fixture is gone. `just device-scenario check-guard`
//    proves it by trying, and prints the silicon shas it did not touch.
//  - The emulated lane never prompts. There is no person at a chooser, so
//    there is nothing to ask; a run whose capture misses its `expect` list
//    files a FINDING (`.emu.failed.jsonl`) and says so. The silicon lane's
//    "k = keep as the golden fixture" branch DOES NOT EXIST here.

import { readdirSync, readFileSync, existsSync, statSync, mkdirSync, appendFileSync, writeFileSync, renameSync, rmSync, openSync } from "node:fs";
import { createServer } from "node:http";
import { spawn, spawnSync, execSync } from "node:child_process";
import { createInterface } from "node:readline";
import path from "node:path";
import { tmpdir } from "node:os";
import process from "node:process";

// The emulated lane's machinery. Importing it has no side effects; the
// silicon lane never calls any of it.
import {
  PACKAGED_C6_ELF as LANE_PACKAGED_C6_ELF,
  StudioDriver as LaneStudioDriver,
  boardRegistry as lane_boardRegistry,
  runSteps as lane_runSteps,
  startDoor as lane_startDoor,
  stopDoor as lane_stopDoor,
  studioUrlFor as lane_studioUrlFor,
} from "./emu/emulated-lane.mjs";

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

/// The two lanes' file names, and the whole of OQ3's mechanism.
///
/// `<id>.jsonl`      — a BOARD's bytes through Studio's own classifier.
/// `<id>.emu.jsonl`  — the same scenario on `lp-cli emu serve`, with no board.
///
/// A filename discriminator rather than a sidecar, because it is the only one
/// of the three shapes a GUARD can be written against: "the emulated lane may
/// only ever write a name containing `.emu.`" is a string test, and
/// `trace_replay.rs` picks the new file up with no change because it walks
/// `*.jsonl`. Provenance that survives a copy rides INSIDE the file, as the
/// first `journal` record (`provenanceRecord` below) — the record kind
/// already exists, so nothing about the JSONL contract changes.
function tracePath(id, lane = "silicon") {
  return path.join(TRACE_DIR, lane === "emulated" ? `${id}.emu.jsonl` : `${id}.jsonl`);
}

function findingPath(id, lane = "silicon") {
  return path.join(TRACE_DIR, lane === "emulated" ? `${id}.emu.failed.jsonl` : `${id}.failed.jsonl`);
}

/// THE GUARD. A silicon fixture is a board's own bytes; it is not
/// reproducible and a clobbered one is gone, so an emulated run must not be
/// ABLE to write one — not on the happy path, not through a prompt, not
/// through a flag. Every rename in this file goes through here.
function assertLaneMayWrite(file, lane) {
  const name = path.basename(file);
  if (lane !== "emulated") {
    return file;
  }
  if (!name.includes(".emu.")) {
    throw new Error(
      `the emulated lane tried to write ${name}, which is a SILICON fixture name.\n` +
        `  A board's bytes are not reproducible and a clobbered fixture is gone, so this is\n` +
        `  refused rather than confirmed. Emulated captures are <id>.emu.jsonl (plan two, OQ3).`,
    );
  }
  return file;
}

function captureStatus(id, lane = "silicon") {
  const file = tracePath(id, lane);
  if (existsSync(file) && statSync(file).size > 0) {
    return { state: "captured", when: statSync(file).mtime.toISOString().slice(0, 16).replace("T", " ") };
  }
  // A filed FINDING (the run happened but did not do what the spec
  // expects) is a visible state of its own — not silently "missing".
  const failed = findingPath(id, lane);
  if (existsSync(failed)) {
    return { state: "finding", when: statSync(failed).mtime.toISOString().slice(0, 16).replace("T", " ") };
  }
  return { state: "missing" };
}

function statusWord(cap) {
  return cap.state === "captured" ? `captured ${cap.when}`
    : cap.state === "finding" ? `✗ finding ${cap.when}`
    : "—";
}

function printStatus(scenarios) {
  console.log("\nDevice scenarios — golden-trace capture status\n");
  console.log("  Two lanes. SILICON is a board's own bytes (<id>.jsonl); EMULATED is the same");
  console.log("  scenario on `lp-cli emu serve` with no board (<id>.emu.jsonl). An emulated");
  console.log("  capture never overwrites a silicon one — see check-guard.\n");
  const rows = scenarios.map((s) => {
    const setup = s.setup?.length
      ? s.setup.every((step) => step.verified) ? "scripted" : "scripted (unverified)"
      : "procedure only";
    return [
      s.id,
      statusWord(captureStatus(s.id, "silicon")),
      s.emulated ? statusWord(captureStatus(s.id, "emulated")) : "(no lane)",
      setup,
      s.board,
      s.title,
    ];
  });
  const header = ["scenario", "silicon", "emulated", "setup", "board", "title"];
  const widths = [0, 0, 0, 0, 0];
  for (const row of [header, ...rows]) for (let i = 0; i < 5; i++) widths[i] = Math.max(widths[i], row[i].length);
  const line = (row) =>
    `  ${row[0].padEnd(widths[0])}  ${row[1].padEnd(widths[1])}  ${row[2].padEnd(widths[2])}  ${row[3].padEnd(widths[3])}  ${row[4].padEnd(widths[4])}  ${row[5]}`;
  console.log(line(header));
  console.log(`  ${widths.map((w) => "-".repeat(w)).join("  ")}  -----`);
  for (const row of rows) console.log(line(row));
  console.log(`\nSilicon (a board on the desk):  just device-scenario run <id> [--port /dev/cu.usbmodemXXXX]`);
  console.log(`Emulated (no board at all):     just device-scenario run <id> --emu [--shots <dir>]`);
  console.log(`The overwrite guard, proved:    just device-scenario check-guard`);
  console.log(`Captures land in ${path.relative(ROOT, TRACE_DIR)}/ (commit them — they are fixtures).\n`);
}

/// Prove the guard by trying it, and print the silicon fixtures' sha256 so a
/// caller can see they were not touched. This is the re-runnable form of
/// invariant 1 (plan two, M6).
function checkGuard(scenarios) {
  console.log("\nThe overwrite guard — trying to make the emulated lane write silicon names\n");
  let refused = 0;
  for (const spec of scenarios) {
    for (const candidate of [tracePath(spec.id, "silicon"), findingPath(spec.id, "silicon")]) {
      try {
        assertLaneMayWrite(candidate, "emulated");
        console.log(`  ✗ ALLOWED  ${path.basename(candidate)}  ← the guard did not hold`);
      } catch (error) {
        refused += 1;
        console.log(`  ✓ refused  ${path.basename(candidate)}  (${error.message.split("\n")[0]})`);
      }
    }
    // …and the names it IS allowed to write.
    for (const candidate of [tracePath(spec.id, "emulated"), findingPath(spec.id, "emulated")]) {
      assertLaneMayWrite(candidate, "emulated");
    }
  }
  console.log(`\n  ${refused} silicon name(s) refused; every <id>.emu.* name allowed.`);
  console.log("\n  sha256 of the committed silicon fixtures, for a before/after comparison:");
  for (const name of readdirSync(TRACE_DIR).filter((n) => n.endsWith(".jsonl") && !n.includes(".emu."))) {
    const sum = execSync(`shasum -a 256 ${JSON.stringify(path.join(TRACE_DIR, name))}`, { encoding: "utf8" })
      .trim()
      .split(/\s+/)[0];
    console.log(`    ${sum}  ${name}`);
  }
  console.log("");
  return refused;
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

/// Board verification + selection (a real, checkable step): list ports
/// passively, narrow by the spec's port_filter (e.g. the classic scenario
/// insists on the CH34x bridge), auto-select when exactly one matches —
/// the Enter-through path involves no prompt at all.
async function pickPort(argPort, spec) {
  if (argPort) return argPort;
  console.log("(listing ports passively — nothing is opened or reset;");
  console.log(" first run may take a couple of minutes while lp-cli builds)");
  const all = listPorts();
  const wanted = spec?.port_filter?.kind_contains ?? null;
  const matching = wanted ? all.filter((port) => port.kind?.includes(wanted)) : all;
  if (all.length === 0) {
    console.log("  ✗ no serial ports found — plug the board in.");
    const answer = await ask("Paste the /dev/... path once attached, or Enter to skip: ");
    return answer || null;
  }
  if (wanted && matching.length === 0) {
    console.log(`  ✗ no attached port matches "${wanted}" (this scenario needs: ${spec.board}).`);
    for (const port of all) console.log(`      have: ${port.port}  (${port.kind})`);
    const answer = await ask("Attach the right board and Enter to re-check, or paste a /dev/... path: ");
    if (answer) return answer;
    return pickPort(argPort, spec);
  }
  if (matching.length === 1) {
    const only = matching[0];
    const serial = only.serial_number ? ` · ${only.serial_number}` : "";
    console.log(`  ✓ one matching board: ${only.port}  (${only.kind}${serial})`);
    return only.port;
  }
  for (const [index, port] of matching.entries()) {
    const serial = port.serial_number ? ` · ${port.serial_number}` : "";
    console.log(`  ${index + 1}. ${port.port}  (${port.kind}${serial})`);
  }
  const answer = await ask("Several match — which one? (number [1], /dev/... path, or 's' to skip): ");
  if (!answer) return matching[0].port;
  if (answer === "s") return null;
  const number = Number.parseInt(answer, 10);
  if (Number.isInteger(number) && number >= 1 && number <= matching.length) {
    return matching[number - 1].port;
  }
  return answer;
}

/// Whether anything holds the serial device open (Chrome with a connected
/// tab, a wedged espflash). lsof reads kernel tables — it never opens or
/// resets the port, so this check is safe mid-sitting.
function portHeldBy(portPath) {
  try {
    const out = execSync(`lsof -t ${JSON.stringify(portPath)}`, { encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] });
    return out.split("\n").map((s) => s.trim()).filter(Boolean);
  } catch {
    return [];
  }
}

/// The "ensure the tabs are closed" step, made VERIFIABLE: we cannot see
/// browser tabs, but we can see whether the port is actually free — which
/// is what the step is for.
async function ensurePortFree(portPath) {
  for (;;) {
    // The path can VANISH between the pick and the flash: a native-USB
    // board re-enumerates on reset/replug, and espflash then fails with
    // a bare "Serial port not found" after the download already ran
    // (2026-08-03 sitting). Catch it here, where re-picking is cheap.
    if (!existsSync(portPath)) {
      console.log(`  ✗ ${portPath} is GONE — the board re-enumerated or was unplugged.`);
      const answer = await ask("    Enter to re-check · p = pick a port again: ");
      if (answer === "p") return null;
      continue;
    }
    const holders = portHeldBy(portPath);
    if (holders.length === 0) {
      console.log(`  ✓ ${portPath} is free.`);
      return true;
    }
    const who = holders.map((pid) => {
      try { return `${pid} (${execSync(`ps -p ${pid} -o comm=`, { encoding: "utf8" }).trim().split("/").pop()})`; }
      catch { return pid; }
    }).join(", ");
    console.log(`  ✗ ${portPath} is HELD by: ${who}`);
    console.log("    Close the Studio tab (or Disconnect the board on its card's Danger tab).");
    const answer = await ask("    Enter to re-check · s = proceed anyway: ");
    if (answer === "s") return true;
  }
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

/// One `expect` entry against a set of records. "state:ready", or
/// alternatives "a|b" — any match passes; a bare "<kind>" matches any record
/// of that kind. UNCHANGED by the emulated lane, deliberately: the whole
/// claim of M6 is that the same matchers pass with no board, so a matcher
/// loosened to make the emulator pass would be the failure, not the fix.
function matchesExpectation(expectation, records) {
  return expectation.split("|").some((alt) => {
    const [kind, value] = alt.split(":");
    return records.some((record) => {
      if (record.kind !== kind) return false;
      if (!value) return true;
      return record.to === value || record.action === value || record.disposition === value
        || record.phase === value || record.from === value || record.content === value;
    });
  });
}

function validate(spec, records) {
  return spec.expect.filter((expectation) => !matchesExpectation(expectation, records));
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
  const state = { active: null, dropped: 0, waiters: [] };
  const offer = (record) => {
    // The emulated lane's `await` step resolves HERE: a record arriving is
    // the event, so no wait in that lane polls a throttled page.
    for (const waiter of [...state.waiters]) {
      if (waiter.matches(record)) {
        state.waiters.splice(state.waiters.indexOf(waiter), 1);
        waiter.resolve(record);
      }
    }
  };
  const sink = createServer((request, response) => {
    let body = "";
    request.on("data", (chunk) => { body += chunk; });
    request.on("end", () => {
      const lines = body.split("\n").filter((line) => line.trim().length > 0);
      if (state.active) {
        for (const line of lines) {
          try {
            const record = JSON.parse(line);
            state.active.records.push(record);
            offer(record);
          } catch { /* keep raw anyway */ }
        }
        if (lines.length) appendFileSync(state.active.partial, lines.join("\n") + "\n");
      } else {
        state.dropped += lines.length;
      }
      response.writeHead(204, { "access-control-allow-origin": "*" });
      response.end();
    });
  });
  /// Wait for a record matching one `expect`-style matcher. Records already
  /// captured in this scenario count — a step that asks for something that
  /// has already happened must not hang.
  state.mark = () => { state.markAt = state.active?.records.length ?? 0; };
  state.awaitRecord = (matcher, timeoutMs, fresh = false) =>
    new Promise((resolve, reject) => {
      const matches = (record) => matchesExpectation(matcher, [record]);
      const from = fresh ? (state.markAt ?? 0) : 0;
      const already = (state.active?.records ?? []).slice(from).find(matches);
      if (already) return resolve(already);
      const waiter = { matches, resolve };
      state.waiters.push(waiter);
      const timer = setTimeout(() => {
        const index = state.waiters.indexOf(waiter);
        if (index >= 0) state.waiters.splice(index, 1);
        reject(new Error(`no device-event record matched \`${matcher}\` before the deadline`));
      }, timeoutMs);
      timer.unref?.();
      waiter.resolve = (record) => { clearTimeout(timer); resolve(record); };
    });
  return { sink, state };
}

/// A scenario run that did NOT do what its spec expects is EVIDENCE, not
/// garbage (sitting feedback, 2026-08-03: "when the scenario fails we need
/// a way of indicating that for you to look at later"): the trace moves to
/// <id>.failed.jsonl and FINDINGS.md gets an entry an agent can pick up.
/// Failed traces are never golden fixtures — the replay test skips them.
function fileFinding(spec, records, failures, partial, note, lane = "silicon") {
  const failed = assertLaneMayWrite(findingPath(spec.id, lane), lane);
  renameSync(partial, failed);
  const findings = path.join(TRACE_DIR, "FINDINGS.md");
  const entry = [
    `## ${new Date().toISOString()} — ${spec.id}${lane === "emulated" ? " (emulated lane)" : ""}`,
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
async function runOne(spec, state, argPort, studioUrl) {
  console.log(`\n=== ${spec.id} — ${spec.title}`);
  console.log(`Board: ${spec.board}`);
  for (const dep of spec.needs ?? []) {
    console.log(`Builds on: ${dep} (make sure the board is in that state, or run it first)`);
  }

  // Sequential, checkable steps (sitting feedback, 2026-08-03): board →
  // port free → setup → tab → do-and-record. Scenarios whose setup never
  // touches the serial port (procedure scenarios) skip straight to the
  // tab — s7 (unplug mid-op) WANTS the board connected already.
  const needs_port = (spec.setup ?? []).some((step) => step.run?.includes("{port}"));
  const total = needs_port ? 5 : 2;
  let step = 0;
  const banner = (title) => {
    step += 1;
    console.log(`\n— step ${step}/${total}: ${title}`);
  };

  if (needs_port) {
    banner(`the board (${spec.board})`);
    let port = await pickPort(argPort, spec);
    if (port) {
      banner("the port must be free (close the Studio tab / Disconnect the card)");
      if ((await ensurePortFree(port)) === null) {
        const repicked = await pickPort(null, spec);
        if (repicked) {
          port = repicked;
          await ensurePortFree(port);
        }
      }
    } else {
      step += 1; // keep numbering honest when the port check is skipped
    }
    banner("setup — putting the board into the known state");
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

  banner("opening the capture tab (automated)");
  console.log(`    ${studioUrl}`);
  if (process.platform === "darwin" && process.stdout.isTTY) {
    spawnSync("open", [studioUrl]);
    console.log("    ✓ opened.");
  }
  banner("in that tab — then the capture records itself:");
  for (const [index, manual] of (spec.manual ?? []).entries()) {
    console.log(`  ${index + 1}. ${manual}`);
  }
  // `r` restarts the CAPTURE, not the scenario: the expensive half
  // (flash/erase setup, the board's known state) is already done, so a
  // false start in the UI should cost the buffer, not the sitting
  // (sitting feedback, 2026-08-03 — "sometimes the UI is in a bad state
  // and I need to change something before running the scenario").
  for (;;) {
    const answer = await ask(
      "\nPress Enter when the scenario is done · r = reset the capture buffer and re-do it here: ",
    );
    if (answer !== "r") {
      break;
    }
    const dropped = records.length;
    records.length = 0;
    writeFileSync(partial, "");
    console.log(`  ↺ discarded ${dropped} events — the board and its setup are untouched.`);
    console.log("    Get the UI into the state you want, then run the steps again:");
    for (const [index, manual] of (spec.manual ?? []).entries()) {
      console.log(`      ${index + 1}. ${manual}`);
    }
  }
  state.active = null;
  console.log("\n(close the Studio tab before the next scenario's setup — an open tab holds the port)");

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

// --- the emulated lane ----------------------------------------------------
//
// One scenario, no board: its own `emu serve` on a fresh state dir, the
// worktree's own `just studio-dev` for the page, one headless Chrome, and the
// spec's `emulated.steps` where the `manual:` list would be.

/// The provenance line, and OQ3's answer to "what tells them apart INSIDE the
/// file". `journal` is an existing record kind (`scope` + `entry`), so this
/// changes nothing about the JSONL contract: `trace_replay.rs` reads `rx` and
/// `state` and ignores it, `validate()` cannot match it (no spec expects a
/// `journal`), and a copy of the file carries its own provenance.
///
/// `configuration` mirrors the transcript system's `lp-emu:<chip>:<grade>`
/// naming (vision D18) — the same words the `.txt.meta.json` sidecars use.
function provenanceRecord(spec, { doorAddr, boards, image, command }) {
  return JSON.stringify({
    t: Date.now() / 1000,
    kind: "journal",
    scope: "capture",
    entry:
      `emulator-captured trace (plan two M6). configuration=lp-emu:esp32c6:t1 ` +
      `scenario=${spec.id} boards=${boards.join(" ")} image=${image} door=${doorAddr} ` +
      `command=${command}. NOT a silicon fixture: no board produced these bytes.`,
  });
}

async function runOneEmulated(spec, state, studio, sinkUrl, options) {
  const lane = "emulated";
  const emulated = spec.emulated;
  if (!emulated) {
    console.log(`\n=== ${spec.id} — SKIPPED: no \`emulated\` block in the spec.`);
    console.log(`    ${spec.title}`);
    return { id: spec.id, skipped: "no emulated lane in the spec" };
  }
  console.log(`\n=== ${spec.id} — ${spec.title}   [EMULATED LANE — no board]`);
  console.log(`Board(s): ${emulated.boards.join(", ")}`);
  if (emulated.note) console.log(`Note: ${emulated.note}`);

  const runDir = path.join(ROOT, "target", "emu-scenarios", spec.id);
  const door = await lane_startDoor({
    root: ROOT,
    id: spec.id,
    boards: emulated.boards,
    stateDir: path.join(runDir, "state"),
    consoleDir: path.join(runDir, "console"),
    logFile: path.join(runDir, "serve.log"),
    fresh: options.freshState !== false,
  });
  console.log(`  emu serve: http://${door.addr}/boards  (pid ${door.pid}, state ${path.relative(ROOT, door.stateDir)})`);

  mkdirSync(TRACE_DIR, { recursive: true });
  const file = assertLaneMayWrite(tracePath(spec.id, lane), lane);
  const partial = `${file}.partial`;
  const records = [];
  writeFileSync(
    partial,
    provenanceRecord(spec, {
      doorAddr: door.addr,
      boards: emulated.boards,
      image: laneImage(),
      command: `just device-scenario run ${spec.id} --emu`,
    }) + "\n",
  );
  state.active = { records, partial };

  const studioUrl = lane_studioUrlFor({ studioPort: studio.port, doorAddr: door.addr, sinkUrl });
  console.log(`  studio:    ${studioUrl}`);
  // NEVER `spawnSync("open", …)` here: a visible window steals the desk's
  // focus and there are other agents on this box. Headless, always.
  const driver = await LaneStudioDriver.launch();
  let stepError = null;
  let steps = [];
  try {
    await driver.navigate(studioUrl);
    await driver.awaitShim();
    steps = await lane_runSteps(emulated.steps, {
      driver,
      shotDir: options.shotDir,
      doorAddr: door.addr,
      awaitRecord: state.awaitRecord,
      mark: state.mark,
    });
  } catch (error) {
    stepError = error;
    console.error(`  ✗ step failed: ${error.message}`);
    if (options.shotDir) {
      try {
        console.error(`    (failure screenshot → ${await driver.screenshot(path.join(options.shotDir, `${spec.id}-FAILED.png`))})`);
      } catch { /* the page may be gone */ }
    }
  }

  const registry = await lane_boardRegistry(door.addr).catch(() => null);
  const consoleNoise = driver.consoleLines().filter((line) => line.startsWith("[error]") || line.startsWith("[exception]"));
  await driver.close();
  state.active = null;
  stopDoorSafely(door);

  console.log(`\nCaptured ${records.length} events.`);
  console.log("\nWhat the trace says happened:");
  console.log(summarize(records));
  if (registry) {
    console.log(`\nDoor's live registry at the end: ${registry.map((b) => `${b.id} flash=${b.flash} boot=${b.boot} reboots=${b.reboots} state=${b.state}`).join(" · ")}`);
  }
  if (consoleNoise.length) {
    console.log("\nPage console errors:");
    for (const line of consoleNoise.slice(-10)) console.log(`  ${line}`);
  }

  const failures = validate(spec, records);
  if (records.length === 0) {
    rmSync(partial, { force: true });
    console.log("\n✗ nothing arrived at the sink — did the page carry ?capture-sink=?");
    return { id: spec.id, ok: false, failures: spec.expect, records: 0, stepError, door, registry };
  }
  if (failures.length || stepError) {
    console.log(`\n✗ capture is missing expected evidence: ${failures.join(", ") || "(steps failed)"}`);
    // No prompt and no "keep as the golden fixture" branch: the emulated lane
    // has nobody to ask, and a fixture kept because the spec was inconvenient
    // is exactly what this milestone must not produce.
    fileFinding(spec, records, failures, partial, stepError ? `step failed: ${stepError.message.split("\n")[0]}` : "emulated lane", lane);
    return { id: spec.id, ok: false, failures, records: records.length, stepError, door, registry, steps };
  }
  renameSync(partial, assertLaneMayWrite(file, lane));
  console.log(`\n✓ capture validates → ${path.relative(ROOT, file)} — commit it (a fixture, not a story PNG).`);
  return { id: spec.id, ok: true, failures: [], records: records.length, door, registry, steps, file };
}

function laneImage() {
  return LANE_PACKAGED_C6_ELF;
}

function stopDoorSafely(door) {
  try {
    lane_stopDoor(door);
  } catch (error) {
    console.warn(`  (could not stop emu serve ${door.pid}: ${error.message})`);
  }
}

/// The emulated sitting: Studio once, a sink once, then N scenarios each with
/// their own door and their own browser. Non-interactive from end to end.
async function emulatedSitting(ids, options) {
  const scenarios = loadScenarios();
  const chosen = ids.length
    ? ids.map((id) => resolveScenario(scenarios, id)).filter(Boolean)
    : scenarios.filter((spec) => spec.emulated);
  if (chosen.length === 0) {
    console.error("no scenarios with an emulated lane matched.");
    process.exit(2);
  }
  const studio = await ensureStudio();
  if (!studio) process.exit(1);
  const { sink, state } = startSink();
  await new Promise((resolve) => sink.listen(0, "127.0.0.1", resolve));
  const sinkUrl = `http://127.0.0.1:${sink.address().port}/ingest`;

  const results = [];
  for (const spec of chosen) {
    results.push(await runOneEmulated(spec, state, studio, sinkUrl, options));
  }
  sink.close();

  console.log("\n=== emulated lane — verdicts\n");
  for (const result of results) {
    const verdict = result.skipped ? `SKIP (${result.skipped})` : result.ok ? "✓ validates" : `✗ ${result.failures.join(", ") || "steps failed"}`;
    console.log(`  ${result.id.padEnd(28)} ${verdict}`);
  }
  console.log("");
  printStatus(loadScenarios());
  if (studio.startedPid) {
    console.log(`Studio (started by this run) is still serving on port ${studio.port} — stop it with: kill ${studio.startedPid}`);
  }
  return results.every((result) => result.ok || result.skipped) ? 0 : 1;
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
  // Deliberately NOT opened here: Studio's load-time auto-connect sweep
  // takes the serial port the moment the tab exists, which is exactly
  // when setup needs the port free (sitting feedback, 2026-08-03 — "the
  // browser steals the port"). Each scenario opens the tab AFTER its
  // setup releases the port, and asks for it to be closed again before
  // the next one.
  console.log(`\nStudio is up (port ${studio.port}). The capture tab opens per-scenario,`);
  console.log("AFTER setup — keep it closed while setup runs.");

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
    await runOne(spec, state, argPort, studioUrl);
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

const [, , command, ...rest] = process.argv;

/// `--flag value` for the two that take one, `--emu` as a bare switch;
/// everything else positional is a scenario id.
function parseArgs(argv) {
  const takesValue = new Set(["--port", "--shots"]);
  const flags = { emu: false, port: null, shots: null, keepState: false, ids: [] };
  for (let index = 0; index < argv.length; index += 1) {
    const arg = argv[index];
    if (arg === "--emu") flags.emu = true;
    else if (arg === "--keep-state") flags.keepState = true;
    else if (takesValue.has(arg)) {
      flags[arg.slice(2)] = argv[index + 1] ?? null;
      index += 1;
    } else if (arg.startsWith("--")) {
      console.error(`unknown flag ${arg}`);
      process.exit(2);
    } else {
      flags.ids.push(arg);
    }
  }
  return flags;
}

const usage = [
  "usage: just device-scenario                    # status table (both lanes)",
  "",
  "  SILICON lane — a board on the desk:",
  "       just device-scenario run [id] [--port /dev/cu.usbmodemXXXX]",
  "         The sitting: setup, hand-off, the tab, capture, validate.",
  "",
  "  EMULATED lane — no board at all (emulator plan two, M6):",
  "       just device-scenario run [id...] --emu [--shots <dir>] [--keep-state]",
  "         Its own `lp-cli emu serve` per scenario on a fresh state dir, the",
  "         worktree's own `just studio-dev` for the page, and the spec's",
  "         `emulated.steps` driven in HEADLESS Chrome. No id runs every",
  "         scenario that has an emulated lane. Writes <id>.emu.jsonl only.",
  "",
  "       just device-scenario check-guard",
  "         Prove, by trying, that the emulated lane cannot write a silicon",
  "         fixture name; prints the silicon fixtures' sha256.",
].join("\n");

if (!command || command === "status" || command === "list") {
  printStatus(loadScenarios());
} else if (command === "check-guard") {
  checkGuard(loadScenarios());
} else if (command === "run") {
  const flags = parseArgs(rest);
  if (flags.emu) {
    process.exitCode = await emulatedSitting(flags.ids, {
      shotDir: flags.shots,
      freshState: !flags.keepState,
    });
  } else {
    await sitting(flags.ids[0] ?? null, flags.port);
  }
} else {
  console.error(usage);
  process.exit(2);
}
