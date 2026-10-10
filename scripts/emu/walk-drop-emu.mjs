#!/usr/bin/env node
// THE DROPPED-LINK WALK WITH NO BOARD (`just walk-drop-emu`).
//
// Defect 2026-10-02-a-dropped-link-sends-the-editor-to-devices: a link that
// goes away under the editor or Play and comes straight back must keep the
// page where it was, behind a quiet "Reconnecting…" strip, and resume the
// SAME session once the board is back on its new link. Before the fix every
// such drop sent the page to /devices.
//
// One Studio session, headless, against an emulated ESP32-C6 over `?emu=`
// (Studio's real Web Serial stack over the shim's virtual USB):
//
//     connect over USB → push a project → open it → the cable comes out
//       under the editor → back in → Play → out under Play → back in
//       → turn a knob on the resumed session
//       → (a power-button project) the banner's D0 switch off → cable out
//         → the board powers itself off
//
// The cable is the shim's own detach/attach (the banner's buttons). The
// Bluetooth twin of the same two steps lives in `walk-ble-emu`'s `drop` and
// `phantom` steps.
//
// The default project is the PLAYFUL Choker, whose switch-mode power button
// reads D0: with the switch off, a cable pull powers the board off, which is
// the firmware's rule. The page holds D0 ON (the banner's switch), so the
// cable pulls above are ridden out; the last step flips it off and checks the
// board does power down. `WALK_PROJECT="Peach (1D)"` walks a project with no
// power button, and skips that step.
//
// Like `walk-no-board`, it is not a CI job and must not become one, and it
// needs a Studio: one already serving on this worktree's canonical port, or
// `--serve-release`, which serves the release bundle itself so the walk runs
// as one foreground command. Every wait is the page's; nothing here sleeps
// for a fixed time.

import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { execSync } from "node:child_process";

import { StudioDriver } from "./studio-driver.mjs";
import { serveStudioBundle, startDoor, stopDoor, studioUrlFor, walkPort } from "./emulated-lane.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
/// `WALK_BACKING=tab` holds the board in the page (`?emu=tab`, a Worker,
/// no server) instead of an `emu serve` door. The tab's board starts blank,
/// so that lane flashes it first.
const TAB = (process.env.WALK_BACKING ?? "") === "tab";
const BOARD = process.env.WALK_BOARD ?? (TAB ? "tab-c6" : "c6-a");
const BOARD_MODEL = process.env.WALK_BOARD_MODEL ?? "XIAO ESP32-C6";
const FLASH_DEADLINE_MS = 900_000;
/// How long the cable stays out before it goes back in. The one deliberate
/// duration here: a person re-seating a cable takes seconds, not the
/// instant the walk would otherwise take.
const DETACHED_MS = Number(process.env.WALK_DETACHED_MS ?? 0);
const PROJECT = process.env.WALK_PROJECT ?? "PLAYFUL Choker";
/// Whether the project carries a switch-mode power button on D0, so the walk
/// ends by switching it off and pulling the cable (`WALK_POWER_OFF=0|1`).
const POWER_OFF = (process.env.WALK_POWER_OFF ?? (/^PLAYFUL Choker/.test(PROJECT) ? "1" : "0")) === "1";
/// `WALK_VIEWPORT=390x844` walks it at a phone's width (where the report
/// came from); the default is the desk's.
const [VIEW_W, VIEW_H] = (process.env.WALK_VIEWPORT ?? "1440x1100").split("x").map(Number);
const STEP_DEADLINE_MS = 180_000;
const STUDIO_LOAD_DEADLINE_MS = 420_000;

const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;
const ROUTE = "location.pathname + location.search";

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
  const out = path.join(ROOT, "target", "walk-drop-emu", `${TAB ? "tab-" : ""}${VIEW_W}x${VIEW_H}`);
  const shots = path.join(out, "shots");
  mkdirSync(shots, { recursive: true });

  // `--serve-release`: no dev server. This walk serves the release bundle
  // (`just studio-web-story-build`) and the packaged firmware itself, on its
  // own stable slot, and never looks at another listener.
  const bundle = process.argv.includes("--serve-release")
    ? await serveStudioBundle({ root: ROOT, port: walkPort(ROOT, "walk-drop-emu") })
    : null;
  const port = bundle ? bundle.address().port : studioPort();
  if (!bundle && !(await studioUp(port))) {
    console.error(
      `No Studio on this worktree's canonical port ${port}. Start one first ` +
        "(`just studio-dev`), or run with --serve-release; this walk never adopts a sibling's listener.",
    );
    process.exit(1);
  }

  const door = TAB ? null : await startDoor({
    root: ROOT,
    id: "walk-drop",
    boards: [`${BOARD}={fw}`],
    stateDir: path.join(out, "state"),
    consoleDir: path.join(out, "console"),
    logFile: path.join(out, "serve.log"),
    fresh: true,
  });
  const url = studioUrlFor({
    studioPort: port,
    doorAddr: door?.addr ?? null,
    sinkUrl: "http://127.0.0.1:9/none",
  }).replace(/&record=[^&]*/, "");

  console.log("\nTHE DROPPED-LINK WALK WITH NO BOARD");
  console.log(
    door
      ? `  emulated board   ${BOARD} (packaged fw-esp32c6), door http://${door.addr}/boards`
      : `  emulated board   ${BOARD}, a Worker in the page (?emu=tab), flashed by Studio first`,
  );
  console.log(`  the page         ${url}\n`);

  const driver = await StudioDriver.launch({ width: VIEW_W, height: VIEW_H });
  const report = { board: BOARD, project: PROJECT, url, steps: [] };
  const shot = async (name) => {
    const file = path.join(shots, `drop-${report.steps.length + 1}-${name}.png`);
    try {
      await driver.screenshot(file);
    } catch {
      /* the page may be gone */
    }
    return file;
  };
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

  /// One cable pull under whatever the page shows: the route never moves,
  /// the strip shows over the same page, then goes once the board is back.
  const pullTheCable = async (name, where, stillThere) => {
    await step(`${name}-out`, `the cable comes out under ${where}: the page stays and says Reconnecting`, async () => {
      const route = await driver.evaluate(ROUTE);
      await driver.detach(BOARD);
      await driver.waitFor(`Boolean(document.querySelector('[data-reconnecting="true"]'))`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the Reconnecting strip",
      });
      // Settled, not mid-fade: the curtain has faded all the way in.
      await driver.waitFor(
        `getComputedStyle(document.querySelector('[data-reconnecting="true"]')).opacity === '1'`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the curtain to finish fading in" },
      );
      const now = await driver.evaluate(ROUTE);
      if (now !== route) throw new Error(`the route moved: ${route} → ${now}`);
      if (!(await driver.evaluate(stillThere))) throw new Error(`${where} went away`);
      report[name] = { route };
      return `still at ${now}`;
    });
    await step(`${name}-in`, "the cable goes back in: the strip goes, the same page carries on", async () => {
      if (DETACHED_MS > 0) await new Promise((resolve) => setTimeout(resolve, DETACHED_MS));
      await driver.attach(BOARD);
      await driver.waitFor(`!document.querySelector('[data-reconnecting="true"]')`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the strip to go (the board back)",
      });
      await driver.waitFor(
        `[...document.querySelectorAll('[data-reconnecting]')].every((c) => getComputedStyle(c).visibility === 'hidden')`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the curtain to finish fading out" },
      );
      const now = await driver.evaluate(ROUTE);
      if (now !== report[name].route) throw new Error(`the route moved: ${report[name].route} → ${now}`);
      if (!(await driver.evaluate(stillThere))) throw new Error(`${where} went away`);
      return `back, still at ${now}`;
    });
  };

  let fatal = null;
  try {
    await driver.navigate(url);
    await driver.awaitShim();
    await driver.waitFor(`${MAIN_TEXT}.length > 0`, {
      timeoutMs: STUDIO_LOAD_DEADLINE_MS,
      what: "Studio to finish loading",
    });

    await step("connect", "Connect a board via USB, and pick the board in the chooser", async () => {
      await driver.pressConnect("USB", { timeoutMs: STEP_DEADLINE_MS });
      await driver.pickBoard(BOARD, { timeoutMs: STEP_DEADLINE_MS });
      if (TAB) {
        // The tab's board is blank: Studio's own flash flow first.
        await driver.waitFor(`${MAIN_TEXT}.includes('needs firmware')`, {
          timeoutMs: STEP_DEADLINE_MS,
          what: "the blank-flash face",
        });
        await driver.clickWhenReady("boards fit", { timeoutMs: STEP_DEADLINE_MS });
        await driver.waitFor(`Boolean(document.querySelector('[id^="ux-popover-panel"]'))`, {
          timeoutMs: STEP_DEADLINE_MS,
          what: "the board-model picker",
        });
        await driver.click(BOARD_MODEL, { scope: `document.querySelector('[id^="ux-popover-panel"]')` });
        await driver.clickWhenReady("Flash firmware", { timeoutMs: STEP_DEADLINE_MS });
        await driver.waitFor(`${MAIN_TEXT}.includes('Flashing firmware')`, {
          timeoutMs: STEP_DEADLINE_MS,
          what: "the flash to start",
        });
        await driver.waitFor(
          `(() => { const t = ${MAIN_TEXT}; return !t.includes('Flashing firmware') && !t.includes('needs firmware'); })()`,
          { timeoutMs: FLASH_DEADLINE_MS, what: "the flash to finish" },
        );
      }
      await driver.waitFor(`${MAIN_TEXT}.includes('Ready')`, { timeoutMs: STEP_DEADLINE_MS, what: "Ready" });
      return "Ready";
    });

    await step("push", `put ${PROJECT} on the board`, async () => {
      const face = await driver.waitFor(
        `(() => { const t = ${MAIN_TEXT};
                  return t.includes('Remove project') ? 'running' : t.includes('to choose from') ? 'empty' : false; })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the board to say what it runs" },
      );
      if (face === "running") {
        await driver.clickWhenReady("Remove project", { timeoutMs: STEP_DEADLINE_MS });
        await driver.click("Remove project");
      }
      await driver.clickWhenReady("to choose from", { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`Boolean(document.querySelector('[id^="ux-popover-panel"]'))`, {
        what: "the project popover",
      });
      await driver.click(PROJECT, {
        scope: `document.querySelector('[id^="ux-popover-panel"]')`,
        exact: true,
      });
      await driver.clickWhenReady("Put it on the board", { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`${MAIN_TEXT}.includes('Project loaded')`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the board to say `Project loaded`",
      });
      return "the board said Project loaded";
    });

    await step("editor", "open the board in the editor", async () => {
      await driver.clickWhenReady("Open in editor", { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(
        `!${MAIN_TEXT}.includes('Connecting project') && Boolean(document.querySelector('#main [role="slider"]'))`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the project to open on the board" },
      );
      return `at ${await driver.evaluate(ROUTE)}`;
    });

    await pullTheCable("editor-drop", "the editor", `Boolean(document.querySelector('#main [role="slider"]'))`);

    await step("play", "switch the editor to Play", async () => {
      await driver.evaluate(`document.querySelector('a[title^="Play mode"]').click()`);
      await driver.waitFor(`location.pathname.endsWith('/play')`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the Play route",
      });
      await driver.waitFor(`Boolean(document.querySelector('#main [role="slider"]'))`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the Play panel's knobs",
      });
      return `at ${await driver.evaluate(ROUTE)}`;
    });

    await pullTheCable("play-drop", "Play", `Boolean(document.querySelector('#main [role="slider"]'))`);

    await step("knob", "turn the first knob on the resumed session; the board's state comes back", async () => {
      const before = await driver.evaluate(
        `document.querySelector('#main [role="slider"]').getAttribute('aria-valuenow')`,
      );
      await driver.evaluate(`(() => {
        const knob = document.querySelector('#main [role="slider"]');
        knob.focus();
        const key = Number(knob.getAttribute('aria-valuenow')) >= Number(knob.getAttribute('aria-valuemax')) ? 'Home' : 'End';
        knob.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
      })()`);
      // The knob shows what the BOARD holds: it moves only when a read
      // after the write brings the board's panel state back.
      await driver.waitFor(
        `document.querySelector('#main [role="slider"]').getAttribute('aria-valuenow') !== ${JSON.stringify(before)}`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the board's panel state to come back with the new value" },
      );
      const after = await driver.evaluate(
        `document.querySelector('#main [role="slider"]').getAttribute('aria-valuenow')`,
      );
      return `${before} → ${after}`;
    });

    if (POWER_OFF) {
      await step("switch-off", "the banner's D0 switch off, then the cable out: the board powers itself off", async () => {
        const flipped = await driver.evaluate(`(() => {
          const flip = document.querySelector(
            '.lp-emu-banner-switch[data-board-id=' + CSS.escape(${JSON.stringify(BOARD)}) + '][data-pad="0"]');
          if (!flip || flip.getAttribute('aria-checked') !== 'true') return false;
          flip.click();
          return true;
        })()`);
        if (!flipped) throw new Error("the banner shows no D0 switch held ON for this board");
        await driver.waitFor(
          `window.__lpEmuSerial.bus.holdsFor(${JSON.stringify(BOARD)}).some((h) => h.pad === 0 && !h.level)`,
          { timeoutMs: STEP_DEADLINE_MS, what: "D0 to read off" },
        );
        await driver.detach(BOARD);
        // The board's own registry row: the door's `GET /boards` says
        // `stopped`, the tab's Worker `deep-sleep`. Nothing in the page
        // changes when a board sleeps behind a pulled cable, so this one wait
        // asks the registry rather than watching the DOM.
        const deadline = Date.now() + STEP_DEADLINE_MS;
        for (;;) {
          const state = await driver.evaluate(
            `(async () => {
               const emu = window.__lpEmuSerial;
               const rows = emu.tabBacking
                 ? await emu.tabBacking.listBoards()
                 : (await (await fetch(new URL('boards', emu.base), { cache: 'no-store' })).json()).boards;
               return rows.find((r) => r.id === ${JSON.stringify(BOARD)})?.state ?? null;
             })()`,
            { awaitPromise: true },
          );
          if (state === "stopped" || state === "deep-sleep") return `board state: ${state}`;
          if (Date.now() > deadline) throw new Error(`the board is still ${state} with D0 off and the cable out`);
          await new Promise((resolve) => setTimeout(resolve, 250));
        }
      });
    }
  } catch (error) {
    fatal = error;
  }

  const consoleErrors = driver
    .consoleLines()
    .filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  const panics = consoleErrors.filter((l) => l.includes("panicked at"));
  report.consoleErrors = consoleErrors;
  writeFileSync(path.join(out, "walk-drop-emu.json"), JSON.stringify(report, null, 2));

  console.log("\n=== the dropped-link walk, step by step");
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${s.name.padEnd(14)} ${path.basename(s.shot)}`);
  if (consoleErrors.length) {
    console.log("\n  page console errors:");
    for (const line of consoleErrors.slice(-8)) console.log(`    ${line.slice(0, 300)}`);
  }
  console.log(`\n  report → ${path.join(out, "walk-drop-emu.json")}`);

  await driver.close();
  if (door) stopDoor(door);
  bundle?.close();

  if (fatal) {
    console.error(`\nThe dropped-link walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  if (panics.length) {
    console.error(`\nThe walk's steps passed, but the page panicked ${panics.length} time(s).`);
    process.exit(1);
  }
  console.log(
    "\n✓ the dropped-link walk finished: two cable pulls ridden out, under the editor and under Play" +
      (POWER_OFF ? ", and the D0 switch off powered the board down." : "."),
  );
}

await main();
