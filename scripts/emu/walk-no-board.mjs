#!/usr/bin/env node
// THE WALK WITH NO BOARD (emulator plan two, M6; `just walk-no-board`).
//
// One continuous Studio session, in a headless browser, against an emulated
// ESP32-C6 that nothing on the desk is plugged into:
//
//     flash → connect → identify → upload a project → detach → re-attach
//
// That order is `plan.md`'s acceptance criterion 6 verbatim, and this is the
// artefact G2 looks at. It is deliberately NOT six scenarios in a row: the
// point is that one board, one page and one grant carry all the way through,
// because that is where the defects this instrument exists for live.
//
// It is NOT a CI job and must not become one (PD9, pre-ruled E-cost): it
// wants Chrome, a dev server, a packaged firmware and an emulator. It is a
// recipe an agent runs.
//
// TWO BACKINGS, ONE WALK. By default the board is held by a native `lp-cli
// emu serve` on a socket. With `--tab` (or `WALK_BACKING=tab`) it is held by
// a Worker inside the page, and NO SERVER IS STARTED AT ALL — same six
// steps, same page, same assertions, one less process on the desk. That is
// the whole claim of the tab backing, so it is worth being able to run the
// same instrument both ways and compare the two reports face for face.
//
// The board starts `kind=rom-up` with an empty flash file — the mask ROM
// finds no image at the reset vector, which is a blank chip, and it is the
// only board kind Studio's esptool-js flow can actually write (DD30/DD34).
// What it boots afterwards is what Studio wrote, from the reset vector.
//
// Every wait is the page's own (a MutationObserver promise) or the sink's (a
// device-event record arriving). Nothing polls, nothing times, and no claim
// in the report this produces is about a duration — an agent-driven tab is
// throttled to ~1 Hz, so a duration measured here would be a measurement of
// the throttle.

import { createServer } from "node:http";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { execSync } from "node:child_process";

import { StudioDriver } from "./studio-driver.mjs";
import { liveRegistry, startDoor, stopDoor, studioUrlFor } from "./emulated-lane.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");

/// The upload step's project. `peach-1d` and not the silicon capture's
/// `fyeah-sign` for one measured reason, and it is a DEVIATION, not a
/// preference: on an emulated board `fyeah-sign`'s graphics stage overruns
/// the firmware's own 8 s RTC watchdog and the chip resets mid-push, then
/// reboot-loops on the startup project the failed push set —
/// docs/defects/2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md,
/// which is Yona's F1 and which this walk reproduces on purpose in the
/// scenario lane (`s3`, `s7`). A real board does the same step in 0.199 s.
/// `WALK_PROJECT="Fyeah Sign" just walk-no-board` is the control: same walk,
/// same board, `reboots=9` instead of `reboots=3`, and the board never says
/// `Project loaded`.
const WALK_PROJECT = process.env.WALK_PROJECT ?? "Peach (1D)";
/// The model the picker offers for this board — the emulated C6 reports
/// `seeed/xiao-esp32-c6` in its own hardware manifest, so this is the honest
/// pick rather than a convenient one.
const BOARD_MODEL = process.env.WALK_BOARD_MODEL ?? "XIAO ESP32-C6";
/// A wedged-run deadline, never a measurement. Flashing writes ~2.4 MB
/// through the ROM's own download console and then the guest boots it.
const FLASH_DEADLINE_MS = 900_000;
const STEP_DEADLINE_MS = 180_000;
/// How long Studio itself may take to arrive, which is a fact about a
/// hundred-megabyte debug bundle and NOT about the board.
///
/// It is separate from `STEP_DEADLINE_MS` because conflating them makes a
/// slow download look like a board that never answered: the first step's
/// deadline was spent watching a progress bar, the screenshot said `Loading
/// Studio…`, and the verdict said the card never offered `It's connected`
/// (measured 2026-09-10 on the tab lane, with the machine otherwise busy).
/// Wait for the app to exist, then start timing the board.
const STUDIO_LOAD_DEADLINE_MS = 420_000;

function args() {
  const tabByDefault = (process.env.WALK_BACKING ?? "") === "tab";
  const out = {
    shots: null,
    out: null,
    keepOpen: false,
    board: tabByDefault ? "tab-c6" : "c6-a",
    tab: tabByDefault,
  };
  let boardNamed = false;
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--shots") out.shots = argv[++i];
    else if (argv[i] === "--out") out.out = argv[++i];
    else if (argv[i] === "--keep-open") out.keepOpen = true;
    else if (argv[i] === "--tab") out.tab = true;
    else if (argv[i] === "--board") {
      out.board = argv[++i];
      boardNamed = true;
    } else {
      console.error(
        `usage: node scripts/emu/walk-no-board.mjs [--tab] [--shots <dir>] [--out <dir>] [--keep-open]`,
      );
      process.exit(2);
    }
  }
  // The tab backing hosts D20's one board, and its id is not the door's.
  if (out.tab && !boardNamed) out.board = "tab-c6";
  out.shots ??= path.join(ROOT, "target", "walk-no-board", "shots");
  out.out ??= path.join(ROOT, "target", "walk-no-board");
  return out;
}

/// The same capture sink the scenario runner uses, minus the fixture
/// bookkeeping: every device-event line Studio streams, in order, so each
/// step can say which records IT produced.
function startSink() {
  const records = [];
  const raw = [];
  const waiters = [];
  const offer = (record) => {
    for (const waiter of [...waiters]) {
      if (waiter.test(record)) {
        waiters.splice(waiters.indexOf(waiter), 1);
        waiter.resolve(record);
      }
    }
  };
  const server = createServer((request, response) => {
    let body = "";
    request.on("data", (chunk) => { body += chunk; });
    request.on("end", () => {
      for (const line of body.split("\n")) {
        if (!line.trim()) continue;
        raw.push(line);
        try {
          const record = JSON.parse(line);
          records.push(record);
          offer(record);
        } catch { /* keep the raw line */ }
      }
      response.writeHead(204, { "access-control-allow-origin": "*" });
      response.end();
    });
  });
  /// Wait for a record the SINK receives. The record arriving is the event —
  /// no page predicate, no timer, and nothing that could be read as a
  /// duration. `from` lets a caller ignore everything already captured.
  const awaitRecord = (test, timeoutMs, from = 0, what = "a record") =>
    new Promise((resolve, reject) => {
      const already = records.slice(from).find(test);
      if (already) return resolve(already);
      const waiter = { test, resolve };
      waiters.push(waiter);
      const timer = setTimeout(() => {
        const index = waiters.indexOf(waiter);
        if (index >= 0) waiters.splice(index, 1);
        reject(new Error(`${what} never arrived`));
      }, timeoutMs);
      timer.unref?.();
      waiter.resolve = (record) => { clearTimeout(timer); resolve(record); };
    });
  return { server, records, raw, awaitRecord };
}

/// The `loaded_projects` of the LAST heartbeat the board wrote to its own
/// console. The door records that console whether or not anything is
/// listening, which is what makes it the board's word rather than Studio's.
async function lastLoadedProjects(consoleFile) {
  const { readFileSync } = await import("node:fs");
  let text = "";
  try {
    text = readFileSync(consoleFile, "utf8");
  } catch {
    return null;
  }
  const matches = [...text.matchAll(/"loaded_projects":(\[[^\]]*\])/g)];
  return matches.length ? matches[matches.length - 1][1] : null;
}

/// The worktree's own canonical dev server — never a substitute (the scenario
/// runner's standing rule, docs/defects/2026-07-27-launch-json-pinned-port.md).
function studioPort() {
  return execSync('bash scripts/dev-port.sh --query studio-dev "${STUDIO_WEB_PORT:-}"', {
    cwd: ROOT,
    encoding: "utf8",
  }).trim();
}

async function studioUp(port) {
  try {
    return (await fetch(`http://localhost:${port}/`, { signal: AbortSignal.timeout(2_000) })).ok;
  } catch {
    return false;
  }
}

async function main() {
  const options = args();
  mkdirSync(options.shots, { recursive: true });
  mkdirSync(options.out, { recursive: true });

  const port = studioPort();
  if (!(await studioUp(port))) {
    console.error(
      `No Studio on this worktree's canonical port ${port}.\n` +
        `Start one first — \`just studio-dev\` or \`just studio-dev-emu\` — and re-run.\n` +
        `(This walk deliberately does not start one: it must never adopt a sibling worktree's listener.)`,
    );
    process.exit(1);
  }

  const stateDir = path.join(options.out, "state");
  // With `--tab` there is no door: the board is a Worker in the page, and
  // the point of the lane is that nothing else is running.
  const door = options.tab
    ? null
    : await startDoor({
        root: ROOT,
        id: "walk",
        boards: [`${options.board}=blank,kind=rom-up`],
        stateDir,
        consoleDir: path.join(options.out, "console"),
        logFile: path.join(options.out, "serve.log"),
        fresh: true,
      });

  const sink = startSink();
  await new Promise((resolve) => sink.server.listen(0, "127.0.0.1", resolve));
  const sinkUrl = `http://127.0.0.1:${sink.server.address().port}/ingest`;
  const url = studioUrlFor({ studioPort: port, doorAddr: door?.addr ?? null, sinkUrl });

  console.log("");
  console.log("THE WALK WITH NO BOARD");
  console.log(`  emulated board   ${options.board}=blank,kind=rom-up  (nothing is plugged into anything)`);
  console.log(
    door
      ? `  door             http://${door.addr}/boards   (pid ${door.pid})`
      : `  backing          a Worker in the page (?emu=tab) — no server anywhere`,
  );
  console.log(`  studio           http://localhost:${port}/`);
  console.log(`  capture sink     ${sinkUrl}`);
  console.log(`  the page         ${url}`);
  console.log(`  screenshots      ${options.shots}`);
  console.log("");

  const driver = await StudioDriver.launch();
  const steps = [];
  let mark = 0;
  /// One step of the criterion's own list: run it, screenshot it, and keep
  /// the device-event records it produced.
  const step = async (name, describe, body) => {
    console.log(`— ${name}: ${describe}`);
    const before = sink.records.length;
    let error = null;
    try {
      await body();
    } catch (failure) {
      error = failure;
      console.error(`  ✗ ${failure.message.split("\n")[0]}`);
    }
    const shot = path.join(options.shots, `walk-${steps.length + 1}-${name}.png`);
    try {
      await driver.screenshot(shot);
    } catch { /* the page may be gone */ }
    const produced = sink.records.slice(before);
    mark = sink.records.length;
    const census = {};
    for (const record of produced) census[record.kind] = (census[record.kind] ?? 0) + 1;
    console.log(`  ${error ? "✗" : "✓"} ${produced.length} device-event record(s): ${Object.entries(census).map(([k, n]) => `${k}=${n}`).join(" ") || "(none)"}`);
    console.log(`  screenshot → ${shot}`);
    steps.push({ name, describe, ok: !error, error: error?.message ?? null, shot, records: produced });
    if (error) throw error;
  };

  let fatal = null;
  try {
    await driver.navigate(url);
    const boards = await driver.awaitShim();
    console.log(`  shim installed over ${boards.length} board(s): ${boards.map((b) => `${b.boardId} (${b.mac})`).join(", ")}`);

    // The shim installs from an inline script, long before the app's wasm has
    // finished downloading — so this is where Studio's own arrival is waited
    // for, under its own deadline. Everything after it is about the board.
    const appUp = await driver.waitFor(
      `(document.querySelector('#main')?.innerText || '').length > 0`,
      { timeoutMs: STUDIO_LOAD_DEADLINE_MS, what: "Studio to finish loading" },
    );
    if (!appUp) {
      throw new Error(
        `Studio did not finish loading within ${STUDIO_LOAD_DEADLINE_MS / 1000} s ` +
          `— the page is still on the shell loader, which is a bundle problem and not a board one`,
      );
    }
    console.log(`  studio is up\n`);

    // 1. FLASH — Studio's own esptool-js flow, into a chip with nothing on it.
    await step("flash", "Studio flashes the packaged firmware into a blank board", async () => {
      await driver.clickWhenReady("It's connected", { timeoutMs: STEP_DEADLINE_MS });
      await driver.pickBoard(options.board, { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(
        `(document.querySelector('#main')?.innerText || '').includes('needs firmware')`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the blank-flash face" },
      );
      await driver.clickWhenReady("boards fit", { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`Boolean(document.querySelector('[id^="ux-popover-panel"]'))`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the board-model picker",
      });
      await driver.click(BOARD_MODEL, { scope: `document.querySelector('[id^="ux-popover-panel"]')` });
      await driver.clickWhenReady("Flash firmware", { timeoutMs: STEP_DEADLINE_MS });
      // The flow has to START and then FINISH, and both halves are needed.
      // "the card stopped saying `needs firmware`" alone is satisfied the
      // instant the card switches to `Flashing firmware…`, which screenshotted
      // a progress bar and called it a flash.
      await driver.waitFor(
        `(document.querySelector('#main')?.innerText || '').includes('Flashing firmware')`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the flash to start" },
      );
      await driver.waitFor(
        `(() => { const t = document.querySelector('#main')?.innerText || "";
                  return !t.includes('Flashing firmware') && !t.includes('needs firmware'); })()`,
        { timeoutMs: FLASH_DEADLINE_MS, what: "the flash to finish" },
      );
      // …and then the BACKING, which is the only party that can see the
      // reset vector. `flash` answers "does an image magic sit there",
      // recomputed rather than cached (DD34), so `loaded` means what Studio
      // wrote is what the ROM will jump to. The door recomputes it per
      // flush; the tab's worker recomputes it per `stats`.
      const where = door ? "door" : "tab";
      const after = await liveRegistry({ doorAddr: door?.addr ?? null, driver });
      const board = after.find((b) => b.id === options.board);
      console.log(`  ${where}: ${board.id} flash=${board.flash} boot=${board.boot} reboots=${board.reboots}`);
      if (board.flash !== "loaded") {
        throw new Error(`the flash flow finished but the ${where} still reports flash=${board.flash}`);
      }
    });

    // 2 + 3. CONNECT and IDENTIFY. The flash flow leaves the port held; the
    // card comes back on the board's own hello, which is the identity.
    await step("connect", "the session comes back on the flashed board", async () => {
      await driver.waitFor(
        `(() => { const t = document.querySelector('#main')?.innerText || "";
                  return t.includes("Ready") || t.includes("Identifying") || false; })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the card to reconnect" },
      );
    });

    await step("identify", "the board says what it is, in its own hello", async () => {
      await driver.waitFor(
        `(document.querySelector('#main')?.innerText || '').includes('Ready')`,
        { timeoutMs: STEP_DEADLINE_MS, what: "Ready" },
      );
      const identity = await driver.evaluate(`
        (() => { const t = document.querySelector('#main')?.innerText || "";
                 const m = t.match(/fw-esp32c6 [0-9a-f]+[^\\n]*/); return m ? m[0] : null; })()
      `);
      const mac = await driver.evaluate(`
        (() => { const t = document.querySelector('#main')?.innerText || "";
                 const m = t.match(/[0-9a-f]{2}(:[0-9a-f]{2}){5}/); return m ? m[0] : null; })()
      `);
      console.log(`  identity: ${identity}   mac: ${mac}`);
      if (!identity) throw new Error("the card never showed a firmware identity");
      steps.identity = identity;
    });

    // 4. UPLOAD — through Studio, as the criterion says, not the CLI.
    await step("upload", `push ${WALK_PROJECT} onto the board from the gallery`, async () => {
      await driver.clickWhenReady("to choose from", { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`Boolean(document.querySelector('[id^="ux-popover-panel"]'))`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the project popover",
      });
      const before = sink.records.length;
      await driver.click(WALK_PROJECT, { scope: `document.querySelector('[id^="ux-popover-panel"]')` });
      await driver.clickWhenReady("Put it on the board", { timeoutMs: STEP_DEADLINE_MS });
      // NOT "the page mentions the project", which the CHOOSER's own button
      // already satisfies the moment it is picked — a predicate written that
      // way passed instantly and screenshotted a card that still read
      // "Nothing loaded" with an empty `loaded_projects` in the board's next
      // heartbeat. And not "Nothing loaded went away" either: that happens
      // when the push STARTS. The push's own end, in the device model's
      // journal, is the only thing that carries an outcome.
      // THE BOARD'S OWN WORDS decide this step, and they are on the card's
      // terminal, streamed off the wire: `Project loaded` is the firmware
      // saying it. That is the gate.
      await driver.waitFor(
        `(document.querySelector('#main')?.innerText || '').includes('Project loaded')`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the board to say `Project loaded`" },
      );

      // STUDIO'S VERDICT is reported beside it rather than asserted on,
      // because they are different claims and the board's is the stronger
      // one. With `Peach (1D)` both agree (`Succeeded`, three runs of three).
      // With `Fyeah Sign` the board never gets to say `Project loaded` at
      // all: its graphics stage overruns the firmware's own 8 s RTC watchdog
      // on an emulated chip, the board resets, and the door's `reboots` count
      // climbs — 9 against this run's 3. That is F1, and it is filed:
      // docs/defects/2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md
      const ended = await sink
        .awaitRecord(
          (record) => (record.entry ?? "").includes("ActivityEnded { kind: Push"),
          30_000,
          before,
          "the push's own ActivityEnded",
        )
        .catch(() => null);
      console.log(`  the board:  Project loaded`);
      console.log(`  Studio:     ${ended ? ended.entry : "(no ActivityEnded seen)"}`);
      const heartbeat = await lastLoadedProjects(path.join(options.out, "console", `${options.board}.console.log`));
      console.log(`  heartbeat:  "loaded_projects":${heartbeat ?? "(not flushed yet — the console is written on a cadence)"}`);
      steps.pushVerdict = ended?.entry ?? null;
    });

    // 5. DETACH mid-session — the CABLE, through the bus. Not a reset, not a
    // socket close: on native USB the serial block survives every reset the
    // channel can ask for, so only the cable un-mints a port.
    await step("detach", "the cable comes out mid-session", async () => {
      await driver.detach(options.board);
      await driver.waitFor(
        `(() => { const rows = [...document.querySelectorAll('.lp-emu-banner-board')];
                  return rows.some((r) => (r.innerText||'').includes('detached')); })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the banner to report the board detached" },
      );
    });

    // 6. RE-ATTACH — a replug is an enumeration, so the grant moves onto a
    // NEW SerialPort object, which is what `adoptReenumeratedPorts` exists for.
    await step("reattach", "the cable goes back in", async () => {
      await driver.attach(options.board);
      await driver.waitFor(
        `(() => { const rows = [...document.querySelectorAll('.lp-emu-banner-board')];
                  return rows.length > 0 && rows.every((r) => !(r.innerText||'').includes('detached')); })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the banner to report the board attached again" },
      );
      // …and then what Studio makes of it, REPORTED rather than asserted.
      // M3's G1 packet already flagged this as a product question for Yona
      // ("replug → Attached, not listening"): Studio re-derives on the
      // hotplug edge but does not re-open a port it had adopted, and it is
      // the SAME code path on hardware. If the emulated replug reproduces
      // the hardware behaviour faithfully, that is a walk that has moved off
      // hardware — which is exactly G2's first question — so the walk record
      // wants the answer either way, not a green tick.
      const settled = await driver
        .waitFor(
          `(() => { const t = document.querySelector('#main')?.innerText || "";
                    const hit = ["Ready", "Identifying", "not listening", "Gone"].find((w) => t.includes(w));
                    return hit || false; })()`,
          { timeoutMs: 60_000, what: "the card to settle after the replug" },
        )
        .catch(() => "(never settled to a word this walk knows)");
      console.log(`  after the replug the card reads: ${JSON.stringify(settled)}`);
      steps.replugSettled = settled;
    });
  } catch (error) {
    fatal = error;
  }

  const registry = await liveRegistry({ doorAddr: door?.addr ?? null, driver }).catch(() => null);
  const consoleErrors = driver.consoleLines().filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));

  // The trace, and a per-step index into it.
  writeFileSync(path.join(options.out, "walk.jsonl"), sink.raw.join("\n") + "\n");
  writeFileSync(
    path.join(options.out, "walk-steps.json"),
    JSON.stringify(
      {
        board: options.board,
        backing: door ? "door" : "tab",
        door: door?.addr ?? null,
        url,
        project: WALK_PROJECT,
        model: BOARD_MODEL,
        registry,
        consoleErrors,
        steps: steps.map((s) => ({
          name: s.name,
          describe: s.describe,
          ok: s.ok,
          error: s.error,
          shot: s.shot,
          recordCount: s.records.length,
          records: s.records,
        })),
      },
      null,
      2,
    ),
  );

  console.log("");
  console.log("=== the walk, step by step");
  for (const s of steps) {
    console.log(`  ${s.ok ? "✓" : "✗"} ${s.name.padEnd(9)} ${s.recordCount ?? s.records.length} record(s)   ${path.basename(s.shot)}`);
  }
  if (registry) {
    console.log(`\n  the ${door ? "door" : "tab"}'s live registry: ${registry.map((b) => `${b.id} flash=${b.flash} boot=${b.boot} reboots=${b.reboots} state=${b.state}`).join(" · ")}`);
  }
  if (consoleErrors.length) {
    console.log("\n  page console errors:");
    for (const line of consoleErrors.slice(-8)) console.log(`    ${line}`);
  }
  console.log(`\n  trace  → ${path.join(options.out, "walk.jsonl")}  (${sink.records.length} records)`);
  console.log(`  steps  → ${path.join(options.out, "walk-steps.json")}`);
  if (door) {
    console.log(`  board console → ${path.join(options.out, "console", `${options.board}.console.log`)}`);
  }

  if (!options.keepOpen) {
    await driver.close();
    if (door) stopDoor(door);
    sink.server.close();
  } else if (door) {
    console.log(`\n  --keep-open: the door (pid ${door.pid}) is still up; the browser is still attached.`);
  } else {
    console.log(`\n  --keep-open: the browser is still attached, and the board is still in it.`);
  }

  if (fatal) {
    console.error(`\nThe walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  console.log(
    `\n✓ the walk finished: flash → connect → identify → upload → detach → re-attach, ` +
      `with no board${door ? "" : " and no server"}.`,
  );
}

await main();
