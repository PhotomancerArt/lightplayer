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
//     connect → Factory reset is under Hardware (not Firmware)
//       → Factory reset → the card is the blank board, Install firmware
//         (no Retry, and the card's top never says "stopped responding")
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
  const url = studioUrlFor({ studioPort: port, doorAddr: door.addr, sinkUrl: "http://127.0.0.1:9/none" }).replace(
    /&record=[^&]*/,
    "",
  );

  console.log("\nTHE FACTORY RESET WALK");
  console.log(`  emulated board   ${BOARD} (the packaged fw-esp32c6, ROM-up), door http://${door.addr}/boards`);
  console.log(`  the page         ${url}\n`);

  const driver = await StudioDriver.launch();
  const report = { board: BOARD, url, steps: [] };
  const step = async (name, describe, body) => {
    console.log(`— ${name}: ${describe}`);
    let error = null;
    let note = null;
    try {
      note = await body();
    } catch (failure) {
      error = failure;
    }
    const shot = path.join(shots, `reset-${report.steps.length + 1}-${name}.png`);
    try {
      await driver.screenshot(shot);
    } catch {
      /* the page may be gone */
    }
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"}${note ? `  ${note}` : ""}`);
    console.log(`  screenshot → ${shot}`);
    report.steps.push({ name, describe, ok: !error, error: error?.message ?? null, note, shot });
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

    await step("where", "Factory reset is in the Hardware row's details, and not the Firmware row's", async () => {
      await driver.openBar("hardware", { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitOffer("erase", { bar: "hardware", timeoutMs: STEP_DEADLINE_MS });
      const words = await driver.offerWords("erase", { inDetails: true });
      if (words !== "Factory reset") throw new Error(`the Hardware details' erase reads ${JSON.stringify(words)}`);
      const shot = path.join(shots, "reset-hardware-details.png");
      await driver.screenshot(shot);
      await driver.openBar("firmware", { timeoutMs: STEP_DEADLINE_MS });
      if (await driver.offered("erase", { inDetails: true })) {
        throw new Error("Factory reset is still in the Firmware row's details");
      }
      await driver.closeDetails();
      return `Hardware: "${words}" (danger, beside Forget); Firmware: none; the details → ${shot}`;
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

    await step("words", "the board's own words: the erased chip boots its blank flash", async () => {
      const line = await driver.boardSaid(/invalid header/, { timeoutMs: STEP_DEADLINE_MS });
      return `the terminal: ${line}`;
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
