#!/usr/bin/env node
// THE BLUETOOTH WALK WITH NO BOARD (M5 of the BLE remote-control plan;
// `just walk-ble-emu`).
//
// One Studio session, headless, against an emulated ESP32-C6 reached over
// `?ble=emu` — the `navigator.bluetooth` polyfill whose NUS service pipes
// bytes to the SAME board `?emu=` holds:
//
//     add over Bluetooth → identify (flash disabled, with its reason)
//       → push a project over Bluetooth → open Play → idle → turn a knob
//
// and it states the one number M5 owes: the bytes per second an idle,
// connected Studio in Play mode puts on a `ble:` link, both directions,
// counted by the polyfill at the characteristics (`stats()`).
//
// ⚠️ WHAT THIS PROVES: the transport, the UI and Play mode. NOT access
// enforcement — the emulated firmware sees its USB link, which it trusts, so
// every request is answered at the edit tier (the polyfill's header says
// why). Emulated time is not silicon time: the board's own heartbeat period
// is guest time on an emulator that runs slower than a C6.
//
// Like `walk-no-board`, it is not a CI job and must not become one, and it
// needs a Studio already serving on this worktree's canonical port (it never
// adopts a sibling's). Every wait is the page's or the polyfill's; the one
// deliberate duration is the idle window, and it is a counting window, not a
// claim about how long anything took.

import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { execSync } from "node:child_process";

import { StudioDriver } from "./studio-driver.mjs";
import { startDoor, stopDoor, studioUrlFor } from "./emulated-lane.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const BOARD = process.env.WALK_BOARD ?? "c6-a";
const PROJECT = process.env.WALK_PROJECT ?? "Peach (1D)";
/// The counting window for the idle measurement. Long enough to hold at
/// least one of the Play lens's once-a-minute reads.
const IDLE_WINDOW_MS = Number(process.env.BLE_IDLE_WINDOW_MS ?? 75_000);
const STEP_DEADLINE_MS = 180_000;
const STUDIO_LOAD_DEADLINE_MS = 420_000;

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

const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;

/// A digest of every canvas under #main — the Play preview is one — so a
/// changed render reads as a changed digest.
const CANVAS_DIGEST = `(() => {
  let h = 0;
  for (const c of document.querySelectorAll('#main canvas')) {
    let s = '';
    try { s = c.toDataURL(); } catch { s = 'tainted'; }
    for (let i = 0; i < s.length; i += 7) h = (h * 31 + s.charCodeAt(i)) | 0;
  }
  return String(h);
})()`;

async function main() {
  const out = path.join(ROOT, "target", "walk-ble-emu");
  const shots = path.join(out, "shots");
  mkdirSync(shots, { recursive: true });

  const port = studioPort();
  if (!(await studioUp(port))) {
    console.error(
      `No Studio on this worktree's canonical port ${port}. Start one first ` +
        "(`just studio-dev`) and re-run; this walk never adopts a sibling's listener.",
    );
    process.exit(1);
  }

  const door = await startDoor({
    root: ROOT,
    id: "walk-ble",
    boards: [`${BOARD}={fw}`],
    stateDir: path.join(out, "state"),
    consoleDir: path.join(out, "console"),
    logFile: path.join(out, "serve.log"),
    fresh: true,
  });
  const url =
    studioUrlFor({ studioPort: port, doorAddr: door.addr, sinkUrl: "http://127.0.0.1:9/none" }).replace(
      /&capture-sink=[^&]*/,
      "",
    ) + "&ble=emu";

  console.log("\nTHE BLUETOOTH WALK WITH NO BOARD");
  console.log(`  emulated board   ${BOARD} (packaged fw-esp32c6), door http://${door.addr}/boards`);
  console.log(`  the page         ${url}\n`);

  const driver = await StudioDriver.launch();
  const report = { board: BOARD, project: PROJECT, url, steps: [] };
  const shot = async (name) => {
    const file = path.join(shots, `ble-${report.steps.length + 1}-${name}.png`);
    try {
      await driver.screenshot(file);
    } catch {
      /* the page may be gone */
    }
    return file;
  };
  const stats = () =>
    driver.evaluate(`JSON.stringify(window.__lpEmuBluetooth?.stats(${JSON.stringify(BOARD)}) ?? null)`).then(JSON.parse);
  const step = async (name, describe, body) => {
    console.log(`— ${name}: ${describe}`);
    let error = null;
    let note = null;
    try {
      note = await body();
    } catch (failure) {
      error = failure;
    }
    const file = await shot(name);
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"}${note ? `  ${note}` : ""}`);
    report.steps.push({ name, describe, ok: !error, error: error?.message ?? null, note, shot: file });
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
    await driver.waitFor("Boolean(window.__lpEmuBluetooth)", { what: "the Bluetooth polyfill" });
    const visibility = await driver.evaluate(`document.visibilityState`);
    console.log(`  studio is up — page visibility: ${visibility}\n`);

    await step("add", "Add over Bluetooth, and pick the board in the pairing chooser", async () => {
      await driver.clickWhenReady("Add over Bluetooth", { timeoutMs: STEP_DEADLINE_MS });
      const picked = await driver.pickBoard(BOARD, { timeoutMs: STEP_DEADLINE_MS });
      return `paired with ${picked}`;
    });

    await step("identify", "the card comes up Ready, flashing drawn disabled with its reason", async () => {
      await driver.waitFor(`${MAIN_TEXT}.includes('Ready')`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "Ready",
      });
      const reason = await driver.evaluate(`${MAIN_TEXT}.includes('Firmware updates need USB')`);
      if (!reason) throw new Error("the card does not say `Firmware updates need USB`");
      const endpoint = await driver.evaluate(
        `JSON.stringify(window.__lpEmuBluetooth.describe(${JSON.stringify(BOARD)}))`,
      );
      return `reason shown; ${endpoint}`;
    });

    await step("push", `push ${PROJECT} over Bluetooth`, async () => {
      const before = await stats();
      await driver.clickWhenReady("to choose from", { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`Boolean(document.querySelector('[id^="ux-popover-panel"]'))`, {
        what: "the project popover",
      });
      await driver.click(PROJECT, { scope: `document.querySelector('[id^="ux-popover-panel"]')` });
      await driver.clickWhenReady("Put it on the board", { timeoutMs: STEP_DEADLINE_MS });
      // The board's own words, streamed off the Bluetooth link.
      await driver.waitFor(`${MAIN_TEXT}.includes('Project loaded')`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the board to say `Project loaded`",
      });
      const after = await stats();
      return `the board said Project loaded; the push wrote ${after.written - before.written} B in ${after.writes - before.writes} writes`;
    });

    await step("play", "open the board in the editor, then Play", async () => {
      await driver.clickWhenReady("Open in editor", { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`Boolean(document.querySelector('a[title^="Play mode"]'))`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the Play toggle",
      });
      await driver.evaluate(`document.querySelector('a[title^="Play mode"]').click()`);
      await driver.waitFor(`Boolean(document.querySelector('#main [role="slider"]'))`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the Play panel's knobs",
      });
      return `at ${await driver.evaluate("location.pathname + location.search")}`;
    });

    await step("idle", `Play, untouched, for ${IDLE_WINDOW_MS / 1000} s — what goes over the air`, async () => {
      const t0 = Date.now();
      const s0 = await stats();
      const d0 = await driver.evaluate(CANVAS_DIGEST);
      await new Promise((resolve) => setTimeout(resolve, IDLE_WINDOW_MS));
      const s1 = await stats();
      const d1 = await driver.evaluate(CANVAS_DIGEST);
      const seconds = (Date.now() - t0) / 1000;
      const idle = {
        seconds: Number(seconds.toFixed(1)),
        studioToBoardBytes: s1.written - s0.written,
        studioToBoardWrites: s1.writes - s0.writes,
        boardToStudioBytes: s1.notified - s0.notified,
        boardToStudioNotifications: s1.notifications - s0.notifications,
        studioToBoardBytesPerSecond: Number(((s1.written - s0.written) / seconds).toFixed(1)),
        boardToStudioBytesPerSecond: Number(((s1.notified - s0.notified) / seconds).toFixed(1)),
        previewChangedWhileIdle: d0 !== d1,
      };
      report.idle = idle;
      return JSON.stringify(idle);
    });

    await step("knob", "turn the first knob to its end, and watch the board's render change", async () => {
      const s0 = await stats();
      const d0 = await driver.evaluate(CANVAS_DIGEST);
      const before = await driver.evaluate(
        `document.querySelector('#main [role="slider"]').getAttribute('aria-valuenow')`,
      );
      await driver.evaluate(`(() => {
        const knob = document.querySelector('#main [role="slider"]');
        knob.focus();
        const key = Number(knob.getAttribute('aria-valuenow')) >= Number(knob.getAttribute('aria-valuemax')) ? 'Home' : 'End';
        knob.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
        return key;
      })()`);
      await driver.waitFor(
        `(() => { const s = window.__lpEmuBluetooth.stats(${JSON.stringify(BOARD)}); return s.writes > ${s0.writes}; })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the panel write to go out over Bluetooth" },
      );
      await driver.waitFor(`${CANVAS_DIGEST} !== ${JSON.stringify(d0)}`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the Play preview (the board's own output frame) to change",
      });
      const after = await driver.evaluate(
        `document.querySelector('#main [role="slider"]').getAttribute('aria-valuenow')`,
      );
      const s1 = await stats();
      report.knob = {
        valueBefore: before,
        valueAfter: after,
        studioToBoardBytes: s1.written - s0.written,
        boardToStudioBytes: s1.notified - s0.notified,
      };
      return JSON.stringify(report.knob);
    });
  } catch (error) {
    fatal = error;
  }

  const consoleErrors = driver
    .consoleLines()
    .filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  const panics = consoleErrors.filter((l) => l.includes("panicked at"));
  report.consoleErrors = consoleErrors;
  writeFileSync(path.join(out, "walk-ble-emu.json"), JSON.stringify(report, null, 2));

  console.log("\n=== the Bluetooth walk, step by step");
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${s.name.padEnd(9)} ${path.basename(s.shot)}`);
  if (report.idle) {
    console.log(
      `\n  idle Play over ble: Studio→board ${report.idle.studioToBoardBytesPerSecond} B/s ` +
        `(${report.idle.studioToBoardBytes} B / ${report.idle.seconds} s), board→Studio ` +
        `${report.idle.boardToStudioBytesPerSecond} B/s (${report.idle.boardToStudioBytes} B)`,
    );
  }
  if (consoleErrors.length) {
    console.log("\n  page console errors:");
    for (const line of consoleErrors.slice(-8)) console.log(`    ${line.slice(0, 300)}`);
  }
  console.log(`\n  report → ${path.join(out, "walk-ble-emu.json")}`);

  await driver.close();
  stopDoor(door);

  if (fatal) {
    console.error(`\nThe Bluetooth walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  if (panics.length) {
    console.error(`\nThe walk's steps passed, but the page panicked ${panics.length} time(s).`);
    process.exit(1);
  }
  console.log("\n✓ the Bluetooth walk finished: add → identify → push → Play → idle → knob, with no board.");
}

await main();
