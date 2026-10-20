#!/usr/bin/env node
// THE RE-ENUMERATION WALK (`just walk-reenumerate-emu`).
//
// Yona's C6, 2026-10-19: "the card periodically 'refreshes' which is jarring
// and resets the ui." A native-USB chip re-enumerates on any reset that
// takes its USB with it, and comes back a moment later as a new SerialPort
// under the same grant. With its port CLOSED that was fixed in #1198. With
// its port OPEN, the read pump heard the dead port's own error and detached
// the link at once: the board was offline for that moment, and its card
// left Online boards for Offline boards and came back, remounted, its open
// details closed (docs/defects/2026-10-19-a-re-enumerating-board-remounts-its-card.md).
//
// This walks it on an emulated C6, in headless Chrome, through Studio's
// real device stack (`?emu=`):
//
//     connect → a running board, its port open
//       → its Hardware details open, three re-enumerations of the OPEN
//         port (the shim's, with a gap): the same card element, under
//         Online boards at every change the page made, its details still
//         open, and the board back on the new port each time
//       → the port dies and does not come back: the card is under Offline
//         boards
//       → the port back: the card is under Online boards again
//
// The emulated C6 does not re-enumerate on a chip reset (the shim's ruling:
// a C6's USB-Serial-JTAG survives a reset the serial channel asks for), so
// the walk does what Chrome does, through the shim: the open port dies the
// way an unplugged one does (its readable ERRORS with "The device has been
// lost.", which is what the controller's read pump is written to catch) and
// `disconnect` fires; a second later a NEW SerialPort holds the same grant
// (`bus.adopt`) and `connect` fires. Not the cable: the emulated USB host
// stays attached, as a chip's own re-enumeration leaves it.
//
// Every wait is on the page's own state, never a timer, and nothing here is
// a duration: an agent-driven tab is throttled. Where the card was at every
// change the page made is RECORDED by a MutationObserver in the page (the
// card element, its section, its top's `data-surface`), so "it never left
// Online boards" is read off the page, not inferred from the moments the
// walk looked.
//
// It proves Studio's hold against the emulated chip; it does NOT prove a
// real C6's re-enumeration timing, nor Chromium's serial backend across it.
// Those are the desk's.
//
// NOT a CI job. `--serve-release` serves the release bundle
// (`just studio-web-story-build`) and the packaged firmware itself, so it
// runs as one foreground command; without it, a Studio must be up on this
// worktree's port.

import { createServer } from "node:http";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { execSync } from "node:child_process";

import { StudioDriver } from "./studio-driver.mjs";
import { serveStudioBundle, startDoor, stopDoor, studioUrlFor, walkPort } from "./emulated-lane.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const BOARD = process.env.WALK_BOARD ?? "c6-a";
const TURNS = 3;
/// Wedged-run deadlines, never measurements.
const STEP_DEADLINE_MS = 180_000;
const STUDIO_LOAD_DEADLINE_MS = 420_000;

const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;
const SURFACE = `(document.querySelector('[data-board-card] [data-surface]')?.getAttribute('data-surface') ?? null)`;
/// Pin the card's element, so a remount (a new element) is read off the page.
const PIN_CARD = `(() => { window.__lpCardPin = document.querySelector('[data-board-card]'); return Boolean(window.__lpCardPin); })()`;
const CARD_HELD = (layer) => `(() => { const card = document.querySelector('[data-board-card]');
  const trigger = card?.querySelector('[data-bar="${layer}"] button[aria-label$=" details"]');
  return JSON.stringify({ same: Boolean(card) && card === window.__lpCardPin,
                          open: trigger?.getAttribute('aria-expanded') === 'true',
                          section: card?.closest('#home-online-boards') ? 'online'
                            : card?.closest('#home-offline-boards') ? 'offline' : null }); })()`;
const SECTION = `(() => { const card = document.querySelector('[data-board-card]'); if (!card) return null;
  return card.closest('#home-online-boards') ? 'online' : card.closest('#home-offline-boards') ? 'offline' : 'neither'; })()`;
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/// Every (card element, section, surface) the page shows, in order, from
/// now on: one entry per change.
const WATCH_CARD = `(() => {
  window.__lpCardTrail = [];
  const note = () => {
    const card = document.querySelector('[data-board-card]');
    const entry = !card ? 'no card'
      : [card === window.__lpCardPin ? 'same' : 'NEW',
         card.closest('#home-online-boards') ? 'online' : card.closest('#home-offline-boards') ? 'offline' : 'neither',
         card.querySelector('[data-surface]')?.getAttribute('data-surface') ?? '?'].join(' ');
    const trail = window.__lpCardTrail;
    if (trail[trail.length - 1] !== entry) trail.push(entry);
  };
  note();
  new MutationObserver(note).observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ['data-surface'] });
  return true;
})()`;
const TRAIL = `JSON.stringify(window.__lpCardTrail)`;

/// The board's open port dies the way Chrome's does under a re-enumeration
/// or an unplug, and `disconnect` fires. Answers whether it was open.
const KILL_PORT = `(() => { const bus = window.__lpEmuSerial.bus;
  const old = bus.livePortFor(${JSON.stringify(BOARD)});
  if (!old) return JSON.stringify({ open: false, missing: true });
  const open = old.opened;
  window.__lpDeadPort = old;
  old.unplug("The device has been lost.");
  bus.dispatchEvent(new CustomEvent("disconnect", { detail: { port: old } }));
  return JSON.stringify({ open }); })()`;
/// A new SerialPort under the same grant, and `connect`.
const PORT_BACK = `(() => { const bus = window.__lpEmuSerial.bus;
  window.__lpFreshPort = bus.adopt(window.__lpDeadPort);
  bus.dispatchEvent(new CustomEvent("connect", { detail: { port: window.__lpFreshPort } }));
  return true; })()`;
/// Studio opened the new port: the board is back on its new link.
const ON_FRESH_PORT = `(window.__lpFreshPort?.opened === true)`;

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
  const out = path.join(ROOT, "target", "walk-reenumerate-emu");
  const shots = path.join(out, "shots");
  mkdirSync(shots, { recursive: true });

  const bundle = process.argv.includes("--serve-release")
    ? await serveStudioBundle({ root: ROOT, port: walkPort(ROOT, "walk-reenumerate-emu") })
    : null;
  const port = bundle ? bundle.address().port : studioPort();
  if (!bundle && !(await studioUp(port))) {
    console.error(
      `No Studio on this worktree's canonical port ${port}. Start one first ` +
        "(`just studio-dev`), or run with --serve-release; this walk never adopts a sibling's listener.",
    );
    process.exit(1);
  }

  const door = await startDoor({
    root: ROOT,
    id: "walk-reenumerate",
    boards: [`${BOARD}={merged},kind=rom-up`],
    stateDir: path.join(out, "state"),
    consoleDir: path.join(out, "console"),
    logFile: path.join(out, "serve.log"),
    fresh: true,
  });
  // The device events Studio streams (`?record=`), so each step can say
  // which ones it produced: a link that left and came back is read here.
  const records = [];
  const sink = createServer((request, response) => {
    let body = "";
    request.on("data", (chunk) => { body += chunk; });
    request.on("end", () => {
      for (const line of body.split("\n")) {
        if (!line.trim()) continue;
        try { records.push(JSON.parse(line)); } catch { /* not a record */ }
      }
      response.writeHead(204, { "access-control-allow-origin": "*" });
      response.end();
    });
  });
  await new Promise((resolve) => sink.listen(0, "127.0.0.1", resolve));
  const url = studioUrlFor({
    studioPort: port,
    doorAddr: door.addr,
    sinkUrl: `http://127.0.0.1:${sink.address().port}/ingest`,
  });

  console.log("\nTHE RE-ENUMERATION WALK");
  console.log(`  emulated board   ${BOARD} (the packaged fw-esp32c6, ROM-up), door http://${door.addr}/boards`);
  console.log(`  the page         ${url}\n`);

  const driver = await StudioDriver.launch();
  const report = { board: BOARD, url, steps: [] };
  const step = async (name, describe, body) => {
    console.log(`— ${name}: ${describe}`);
    const from = records.length;
    let error = null;
    let note = null;
    try {
      note = await body();
    } catch (failure) {
      error = failure;
    }
    const produced = records.slice(from);
    const census = {};
    for (const record of produced) census[record.kind] = (census[record.kind] ?? 0) + 1;
    const shot = path.join(shots, `reenum-${report.steps.length + 1}-${name}.png`);
    try {
      await driver.screenshot(shot);
    } catch {
      /* the page may be gone */
    }
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"}${note ? `  ${note}` : ""}`);
    console.log(`  ${produced.length} device event(s): ${Object.entries(census).map(([k, n]) => `${k}=${n}`).join(" ") || "(none)"}`);
    console.log(`  screenshot → ${shot}`);
    report.steps.push({ name, describe, ok: !error, error: error?.message ?? null, note, shot, records: produced });
    if (error) throw error;
  };

  let fatal = null;
  try {
    await driver.navigate(url);
    await driver.awaitShim();
    await driver.waitFor(`${MAIN_TEXT}.length > 0`, {
      timeoutMs: STUDIO_LOAD_DEADLINE_MS,
      what: "Studio to finish loading",
    });
    console.log(`  studio is up — page visibility: ${await driver.evaluate("document.visibilityState")}\n`);

    await step("connect", "Connect a board over USB: the emulated C6 on the packaged firmware", async () => {
      await driver.pressConnect("USB", { timeoutMs: STEP_DEADLINE_MS });
      await driver.pickBoard(BOARD, { timeoutMs: STEP_DEADLINE_MS });
      const runs = await driver.boardRuns({ timeoutMs: STEP_DEADLINE_MS });
      const section = await driver.evaluate(SECTION);
      if (section !== "online") throw new Error(`the running board's card is under ${section}`);
      report.startSurface = await driver.evaluate(SURFACE);
      return `the board runs ${runs === "empty" ? "nothing" : "a project"}; its card says ${report.startSurface}, under Online boards`;
    });

    await step(
      "reenumerate",
      `${TURNS} re-enumerations of the OPEN port, the Hardware details open: the same card, under Online boards throughout`,
      async () => {
        await driver.openBar("hardware", { timeoutMs: STEP_DEADLINE_MS });
        await driver.evaluate(PIN_CARD);
        await driver.evaluate(WATCH_CARD);
        const turns = [];
        // Where the card has been since the watch began, kept on the report
        // and named in any failure: a remount reads as a `NEW` entry.
        const fail = async (why) => {
          const trail = JSON.parse(await driver.evaluate(TRAIL));
          report.reenumerationTrail = trail;
          throw new Error(`${why}; the card: ${trail.join(" → ")}`);
        };
        for (let turn = 1; turn <= TURNS; turn += 1) {
          const killed = JSON.parse(await driver.evaluate(KILL_PORT));
          if (!killed.open) throw new Error(`turn ${turn}: the board's port was not open (${JSON.stringify(killed)})`);
          await pause(1_000);
          await driver.evaluate(PORT_BACK);
          await driver
            .waitFor(ON_FRESH_PORT, {
              timeoutMs: STEP_DEADLINE_MS,
              what: `turn ${turn}: Studio to open the re-enumerated port`,
            })
            .catch((error) => fail(error.message));
          await driver
            .waitFor(`${SURFACE} === ${JSON.stringify(report.startSurface)}`, {
              timeoutMs: STEP_DEADLINE_MS,
              what: `turn ${turn}: the card to read ${report.startSurface} again on the new port`,
            })
            .catch((error) => fail(error.message));
          const held = JSON.parse(await driver.evaluate(CARD_HELD("hardware")));
          turns.push(held);
          if (!held.same) await fail(`turn ${turn}: the card was remounted`);
          if (!held.open) await fail(`turn ${turn}: the Hardware details closed`);
          if (held.section !== "online") await fail(`turn ${turn}: the card is under ${held.section}`);
        }
        const trail = JSON.parse(await driver.evaluate(TRAIL));
        report.reenumerationTrail = trail;
        report.turns = turns;
        const away = trail.filter((entry) => !entry.startsWith("same online "));
        if (away.length) await fail("the card moved or remounted on the way");
        const banned = ["offline", "disconnected", "not-answering"];
        const said = trail.map((entry) => entry.split(" ")[2]);
        const wrong = said.filter((state) => banned.includes(state));
        if (wrong.length) await fail(`the card's top said ${wrong.join(", ")} on the way`);
        await driver.closeDetails();
        return `same card, under Online boards at every change, details open after each turn; the card: ${trail.join(" → ")}`;
      },
    );

    await step("gone", "the open port dies and does not come back: the card is under Offline boards", async () => {
      const killed = JSON.parse(await driver.evaluate(KILL_PORT));
      if (!killed.open) throw new Error(`the board's port was not open (${JSON.stringify(killed)})`);
      await driver.waitFor(`${SECTION} === 'offline'`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the board's card to be under Offline boards",
      });
      return `under Offline boards; its top says ${await driver.evaluate(SURFACE)}`;
    });

    await step("back", "the port back: the card is under Online boards again", async () => {
      await driver.evaluate(PORT_BACK);
      await driver.waitFor(`${SECTION} === 'online' && ${ON_FRESH_PORT}`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the board's card to be under Online boards again",
      });
      await driver.waitFor(`${SURFACE} === ${JSON.stringify(report.startSurface)}`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: `the card to read ${report.startSurface} again`,
      });
      return `under Online boards; its top says ${await driver.evaluate(SURFACE)}`;
    });
  } catch (error) {
    fatal = error;
  }

  const consoleErrors = driver
    .consoleLines()
    .filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  const panics = consoleErrors.filter((l) => l.includes("panicked at"));
  report.consoleErrors = consoleErrors;
  writeFileSync(path.join(out, "walk-reenumerate-emu.json"), JSON.stringify(report, null, 2));
  writeFileSync(path.join(out, "page-console.log"), driver.consoleLines().join("\n") + "\n");

  console.log("\n=== the re-enumeration walk, step by step");
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${s.name.padEnd(12)} ${path.basename(s.shot)}`);
  if (consoleErrors.length) {
    console.log("\n  page console errors:");
    for (const line of consoleErrors.slice(-8)) console.log(`    ${line.slice(0, 300)}`);
  }
  console.log(`\n  report → ${path.join(out, "walk-reenumerate-emu.json")}`);
  console.log(`  page console → ${path.join(out, "page-console.log")}`);
  console.log(`  the board's console → ${path.join(out, "console")}`);

  await driver.close();
  await stopDoor(door);
  bundle?.close();
  sink.close();

  if (fatal) {
    console.error(`\nThe re-enumeration walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  if (panics.length) {
    console.error(`\nThe walk's steps passed, but the page panicked ${panics.length} time(s).`);
    process.exit(1);
  }
  console.log(`\n✓ the re-enumeration walk finished: ${TURNS} re-enumerations of an open port left the same card under Online boards, its details open; a port that stayed gone went offline and came back.`);
}

await main();
