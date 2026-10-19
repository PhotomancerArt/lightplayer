#!/usr/bin/env node
// THE FACTORY RESET WALK (`just walk-factory-reset-emu`).
//
// Yona's C6 over USB, 2026-10-19: Factory reset was in the Firmware row's
// details, and after it the card read "This board stopped responding over
// USB." with a Retry that timed out; only a replug reached the blank board
// (docs/defects/2026-10-19-a-board-erased-over-webusb-reads-not-responding-until-a-reload.md).
// This walks the fixed flow on an emulated C6, in headless Chrome, through
// Studio's real device stack (`?emu=`: esptool-js over the emulated
// USB-Serial-JTAG, the ROM and the stub the board ships with):
//
//     connect → a running board's card holds for a minute (same card,
//         its details open)
//       → Factory reset is under Hardware (not Firmware)
//       → Factory reset → the card is the blank board, Install firmware
//         (no Retry, and the card's top never says "stopped responding")
//       → three re-enumerations (the cable out and in, standing in for the
//         erased chip's watchdog): the same card, its details open, still
//         the blank board
//       → Install firmware, one press → the board says hello on what
//         Studio wrote
//
// The board is the packaged whole chip, booted ROM-up from a writable copy
// (`{merged},kind=rom-up`), so an erase empties the chip the ROM boots.
//
// Every wait is on the page's own state or the board's words, never a
// timer, and nothing here is a duration: an agent-driven tab is throttled.
// What the card's top said along the way is RECORDED by a MutationObserver
// in the page (every `data-surface` it showed, in order), so "it never said
// stopped responding" is read off the page, not inferred.
//
// It proves Studio's flow against the emulated chip; it does NOT prove a
// real C6's re-enumeration after the reset, nor Chromium's serial backend
// across it. Those are the desk's (the defect says what to re-walk).
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
/// The model the install's board pick offers for an emulated C6 (it reports
/// `seeed/xiao-esp32-c6`).
const BOARD_MODEL = process.env.WALK_BOARD_MODEL ?? "XIAO ESP32-C6";
/// Wedged-run deadlines, never measurements.
const STEP_DEADLINE_MS = 180_000;
const ERASE_DEADLINE_MS = 600_000;
const FLASH_DEADLINE_MS = 900_000;
const STUDIO_LOAD_DEADLINE_MS = 420_000;

const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;
const SURFACE = `(document.querySelector('[data-board-card] [data-surface]')?.getAttribute('data-surface') ?? null)`;
const SURFACE_WORDS = `(() => { const card = document.querySelector('[data-board-card]'); if (!card) return null;
  const words = (sel) => (card.querySelector(sel)?.textContent || '').replace(/\\s+/g, ' ').trim() || null;
  return JSON.stringify({ state: card.querySelector('[data-surface]')?.getAttribute('data-surface') ?? null,
                          first: words('[data-surface-first]'), second: words('[data-surface-second]'),
                          primary: words('[data-name-bar] button:last-of-type') }); })()`;
/// Pin the card's element and whether a bar's details are open, so a
/// remount (a new element) or a closed popover is read off the page.
const PIN_CARD = `(() => { window.__lpCardPin = document.querySelector('[data-board-card]'); return Boolean(window.__lpCardPin); })()`;
const CARD_HELD = (layer) => `(() => { const card = document.querySelector('[data-board-card]');
  const trigger = card?.querySelector('[data-bar="${layer}"] button[aria-label$=" details"]');
  return JSON.stringify({ same: Boolean(card) && card === window.__lpCardPin,
                          open: trigger?.getAttribute('aria-expanded') === 'true' }); })()`;
const UNHELD = (layer) => `(() => { const held = JSON.parse(${CARD_HELD(layer)}); return !held.same || !held.open; })()`;
const pause = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/// Every `data-surface` the card's top shows, in order, from now on.
const WATCH_SURFACE = `(() => {
  window.__lpSurfaceTrail = [];
  const note = () => {
    const state = ${SURFACE};
    const trail = window.__lpSurfaceTrail;
    if (state && trail[trail.length - 1] !== state) trail.push(state);
  };
  note();
  new MutationObserver(note).observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ['data-surface'] });
  return true;
})()`;

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
  const out = path.join(ROOT, "target", "walk-factory-reset-emu");
  const shots = path.join(out, "shots");
  mkdirSync(shots, { recursive: true });

  const bundle = process.argv.includes("--serve-release")
    ? await serveStudioBundle({ root: ROOT, port: walkPort(ROOT, "walk-factory-reset-emu") })
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
    id: "walk-factory-reset",
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

  console.log("\nTHE FACTORY RESET WALK");
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
    const shot = path.join(shots, `reset-${report.steps.length + 1}-${name}.png`);
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
  const surface = async () => JSON.parse((await driver.evaluate(SURFACE_WORDS)) ?? "null");

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
      report.firmwareBefore = await driver.barText("firmware");
      return `the board says it runs ${runs === "empty" ? "nothing" : "a project"}; ${report.firmwareBefore}`;
    });

    // A healthy board's card, watched with its details open: nothing a
    // board does on its own may remount it or close them.
    await step("steady", "a running board's card holds: the same card, its Hardware details open, for a minute", async () => {
      await driver.openBar("hardware", { timeoutMs: STEP_DEADLINE_MS });
      await driver.evaluate(PIN_CARD);
      await driver.waitFor(UNHELD("hardware"), { timeoutMs: 60_000, what: "the card to remount or close" }).catch(() => null);
      const held = JSON.parse(await driver.evaluate(CARD_HELD("hardware")));
      report.runningCardHeld = held;
      if (!held.same) throw new Error("the running board's card was remounted");
      if (!held.open) throw new Error("the running board's Hardware details closed by themselves");
      await driver.closeDetails();
      return "same card, details still open after 60 s";
    });

    await step("where", "Factory reset is in the Hardware row's details, and not the Firmware row's", async () => {
      await driver.openBar("hardware", { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitOffer("erase", { bar: "hardware", timeoutMs: STEP_DEADLINE_MS });
      // A Lasting button holds its resting and its armed reading in one
      // cell ("Factory reset", "Confirm reset"): the resting one leads.
      const words = await driver.offerWords("erase", { inDetails: true });
      if (!words?.startsWith("Factory reset")) throw new Error(`the Hardware details' erase reads ${JSON.stringify(words)}`);
      const shot = path.join(shots, "reset-hardware-details.png");
      await pause(600); // the popover's entrance, for the picture only
      await driver.screenshot(shot);
      await driver.openBar("firmware", { timeoutMs: STEP_DEADLINE_MS });
      // Read inside the Firmware bar's own popover: the Hardware one may
      // still be fading out over it, Factory reset and all.
      const inFirmware = await driver.evaluate(`(() => { const bar = document.querySelector('[data-board-card] [data-bar="firmware"]');
        const panel = bar?.querySelector('[id^="ux-popover-panel"]');
        if (!panel) return 'no panel';
        return Boolean(panel.querySelector('[data-offer-path$="/erase"]')); })()`);
      if (inFirmware === "no panel") throw new Error("the Firmware details did not open");
      if (inFirmware) throw new Error("Factory reset is still in the Firmware row's details");
      await driver.closeDetails();
      return `Hardware: Factory reset (danger, beside Forget); Firmware: none; the details → ${shot}`;
    });

    await step("reset", "Factory reset: esptool-js erases the chip, then the card is the blank board", async () => {
      await driver.evaluate(WATCH_SURFACE);
      // Lasting: the first press arms, the second erases.
      await driver.pressOffer("erase", { bar: "hardware", confirm: true, timeoutMs: STEP_DEADLINE_MS });
      await driver.closeDetails().catch(() => {});
      await driver.waitFor(`${SURFACE} === 'erasing'`, { timeoutMs: STEP_DEADLINE_MS, what: "the erase to start" });
      await driver.waitFor(`${SURFACE} !== 'erasing'`, { timeoutMs: ERASE_DEADLINE_MS, what: "the erase to end" });
      const blank = await driver
        .waitFor(`${SURFACE} === 'no-firmware'`, { timeoutMs: STEP_DEADLINE_MS, what: "the blank board's card" })
        .catch(async () => {
          throw new Error(`after the erase the card's top says ${JSON.stringify(await surface())}`);
        });
      void blank;
      const said = await surface();
      report.afterReset = said;
      if (said.first !== "This board doesn't have LightPlayer firmware.") throw new Error(`the surface says ${JSON.stringify(said)}`);
      if (said.second !== "Install the firmware to use it.") throw new Error(`the surface says ${JSON.stringify(said)}`);
      const install = await driver.offerWords("flash");
      if (!install || !install.includes("Install")) throw new Error(`the primary is not Install firmware: ${JSON.stringify(install)}`);
      if (await driver.offered("retry")) throw new Error("the card offers Retry on the blank board");
      report.firmwareAfterReset = await driver.barText("firmware");
      report.linkAfterReset = await driver.barText("connection");
      return `"${said.first}" "${said.second}", primary ${JSON.stringify(install)}; ${report.firmwareAfterReset}; ${report.linkAfterReset}`;
    });

    await step("trail", "the card's top never said the board stopped responding", async () => {
      const trail = JSON.parse(await driver.evaluate("JSON.stringify(window.__lpSurfaceTrail)"));
      report.surfaceTrail = trail;
      if (trail.includes("not-answering")) throw new Error(`the card said "stopped responding" on the way: ${trail.join(" → ")}`);
      // Held a little: a board that re-enumerates or a late identify must
      // not turn it back. The page's own wait, ended by a change or the
      // bound; either way the trail is what is read.
      await driver
        .waitFor(`${SURFACE} !== 'no-firmware'`, { timeoutMs: 15_000, what: "the blank card to change" })
        .catch(() => null);
      const held = JSON.parse(await driver.evaluate("JSON.stringify(window.__lpSurfaceTrail)"));
      report.surfaceTrail = held;
      const now = await surface();
      if (now.state !== "no-firmware") throw new Error(`the blank card did not hold: ${held.join(" → ")}`);
      return held.join(" → ");
    });

    // The erased chip's watchdog re-enumerates its USB every few seconds on
    // a real C6. The emulator does not re-enumerate on a reset (the shim's
    // ruling), so the cable stands in: out, and back in a moment later,
    // three times — the same disconnect/connect edges and new SerialPort
    // under the same grant. The card must not move, change or close.
    await step("bootloop", "the blank card holds through three re-enumerations (the cable out and in), its details open", async () => {
      await driver.openBar("hardware", { timeoutMs: STEP_DEADLINE_MS });
      await driver.evaluate(PIN_CARD);
      for (let turn = 1; turn <= 3; turn += 1) {
        await driver.detach(BOARD);
        await pause(1_000);
        await driver.attach(BOARD);
        await pause(4_000);
        const held = JSON.parse(await driver.evaluate(CARD_HELD("hardware")));
        const now = await surface();
        if (!held.same) throw new Error(`turn ${turn}: the card was remounted`);
        if (!held.open) throw new Error(`turn ${turn}: the Hardware details closed`);
        if (now.state !== "no-firmware") throw new Error(`turn ${turn}: the card's top says ${JSON.stringify(now)}`);
      }
      const trail = JSON.parse(await driver.evaluate("JSON.stringify(window.__lpSurfaceTrail)"));
      report.surfaceTrail = trail;
      // From the first blank card on, the trail has one entry: nothing else
      // was ever shown (the trail records each change).
      const since = trail.slice(trail.indexOf("no-firmware"));
      if (since.length !== 1) throw new Error(`the card's top changed on the way: ${trail.join(" → ")}`);
      await driver.closeDetails();
      return `same card, details open, still the blank board; the card's top: ${trail.join(" → ")}`;
    });

    // REPORTED, not asserted: a C6 boots its blank flash after the erase's
    // reset and loops "invalid header"; the emulated chip, as of this walk,
    // stays in the flasher's stub after esptool-js's hard reset (its console
    // ends at "OHAI"), so the card is held by what Studio knows, not by
    // what the board says. Either way the card is the blank board.
    await step("words", "the board's terminal after the erase (reported)", async () => {
      const lines = await driver.terminalLines({ timeoutMs: STEP_DEADLINE_MS }).catch(() => []);
      await driver.closeDetails().catch(() => {});
      const loop = lines.find((line) => line.includes("invalid header"));
      report.bootLoopHeard = Boolean(loop);
      return loop ? `the boot loop: ${loop}` : `no boot loop heard; the terminal ends: ${JSON.stringify(lines.slice(-2))}`;
    });

    await step("install", `Install firmware, one press: Studio's flash writes the packaged build (${BOARD_MODEL})`, async () => {
      await driver.flashBlank(BOARD_MODEL, { timeoutMs: STEP_DEADLINE_MS, flashTimeoutMs: FLASH_DEADLINE_MS });
      const runs = await driver.boardRuns({ timeoutMs: STEP_DEADLINE_MS });
      report.firmwareAfterInstall = await driver.barText("firmware");
      return `the board says hello on what Studio wrote (${report.firmwareAfterInstall}), and runs ${runs === "empty" ? "nothing" : "a project"}`;
    });
  } catch (error) {
    fatal = error;
  }

  const consoleErrors = driver
    .consoleLines()
    .filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  const panics = consoleErrors.filter((l) => l.includes("panicked at"));
  report.consoleErrors = consoleErrors;
  writeFileSync(path.join(out, "walk-factory-reset-emu.json"), JSON.stringify(report, null, 2));
  writeFileSync(path.join(out, "page-console.log"), driver.consoleLines().join("\n") + "\n");

  console.log("\n=== the Factory reset walk, step by step");
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${s.name.padEnd(8)} ${path.basename(s.shot)}`);
  if (consoleErrors.length) {
    console.log("\n  page console errors:");
    for (const line of consoleErrors.slice(-8)) console.log(`    ${line.slice(0, 300)}`);
  }
  console.log(`\n  report → ${path.join(out, "walk-factory-reset-emu.json")}`);
  console.log(`  page console → ${path.join(out, "page-console.log")}`);
  console.log(`  the board's console → ${path.join(out, "console")}`);

  await driver.close();
  await stopDoor(door);
  bundle?.close();
  sink.close();

  if (fatal) {
    console.error(`\nThe Factory reset walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  if (panics.length) {
    console.error(`\nThe walk's steps passed, but the page panicked ${panics.length} time(s).`);
    process.exit(1);
  }
  console.log("\n✓ the Factory reset walk finished: connect → Factory reset under Hardware → the blank board with Install firmware, no Retry → Install firmware → hello, with no board.");
}

await main();
