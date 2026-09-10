#!/usr/bin/env node
// The EMULATED lane of the device-scenario runner (emulator plan two, M6).
//
// A device scenario has two halves. On hardware, `setup:` is espflash putting
// a board into a known state and `manual:` is a person reading steps and
// clicking. With no board:
//
//   setup:   becomes `lp-cli emu serve` — a fresh `--state-dir` and a board
//            spelled the way the scenario needs it (`blank,kind=rom-up` for
//            an erased chip; the packaged firmware ELF for a board already
//            running LightPlayer). "Erase the flash" is not a command any
//            more; it is a board that was never written.
//   manual:  becomes `emulated.steps` — the same clicks, driven through
//            `studio-driver.mjs` in headless Chrome.
//
// WHAT DOES NOT CHANGE, and must not:
//
//   * The `expect` matchers. The whole claim of M6 is that an emulated run
//     satisfies the SAME list a board satisfied. Loosening one to make the
//     emulator pass is the failure this lane exists to make visible.
//   * The device-event record shape. The trace is Studio's own output,
//     streamed to the same `?capture-sink=`; this lane never writes a record
//     Studio did not emit, with the single exception of the provenance
//     `journal` line the runner prepends (OQ3) — which is the kind's own
//     existing shape, is written by the runner and says so, and is skipped by
//     every consumer that reads `rx`/`state`.
//   * The port. There is no OS serial port here at all: the "port" is a board
//     id on a door, so `hardware list` is never called in this lane and no
//     `lsof` guard applies.
//
// ONE `emu serve` PER SCENARIO, on an ephemeral port, with its own state
// directory — that is what makes `blank` mean blank. The PAGE still comes
// from the worktree's own canonical `just studio-dev` (the runner's standing
// rule: never a substitute server), and the two are joined by `?emu=<url>` in
// the query string, which composes with `?capture-sink=` because nothing
// reads anything else's flag.

import { spawn } from "node:child_process";
import { existsSync, mkdirSync, openSync, readFileSync, rmSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { StudioDriver } from "./studio-driver.mjs";

/// The packaged C6 firmware ELF `studio-dev-emu` boots its boards from — the
/// same build `studio-firmware-package-served` leaves behind, so a board that
/// starts "already running LightPlayer" is running the build this Studio
/// serves. `{fw}` in a spec's board line substitutes to this.
export const PACKAGED_C6_ELF = "target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6";

/// How long a wedged step may hang before the run is failed. NEVER an
/// assertion: nothing in this milestone concludes anything from elapsed time,
/// and an agent-driven tab is throttled to ~1 Hz anyway.
const STEP_DEADLINE_MS = 180_000;

// --- the door -------------------------------------------------------------

/// Start an `lp-cli emu serve` holding this scenario's boards and return its
/// address. Ephemeral port, printed and read — never computed.
export async function startDoor({ root, id, boards, stateDir, consoleDir, logFile, fresh = true }) {
  if (fresh && existsSync(stateDir)) {
    // "Erase the entire flash" in the emulated lane. A blank board is a board
    // whose flash file does not exist, which is a fact about a directory
    // rather than a command against a chip.
    rmSync(stateDir, { recursive: true, force: true });
  }
  mkdirSync(stateDir, { recursive: true });
  mkdirSync(consoleDir, { recursive: true });
  const binary = path.join(root, "target/debug/lp-cli");
  if (!existsSync(binary)) {
    throw new Error(`no ${binary} — run \`cargo build -p lp-cli\` first`);
  }
  const args = ["emu", "serve"];
  for (const board of boards) args.push("--board", board.replaceAll("{fw}", PACKAGED_C6_ELF));
  args.push("--listen", "127.0.0.1:0", "--state-dir", stateDir, "--console-dir", consoleDir);
  const fd = openSync(logFile, "a");
  const child = spawn(binary, args, { cwd: root, detached: true, stdio: ["ignore", fd, fd] });
  child.unref();
  const addr = await readListenAddress(logFile, child, id);
  return { addr, pid: child.pid, log: logFile, args, stateDir, consoleDir };
}

/// The door prints `emu serve: listening on http://<addr>`. That printed
/// address is the source of truth — the same rule `studio-dev-emu` states.
function readListenAddress(logFile, child, id) {
  return new Promise((resolve, reject) => {
    const deadline = Date.now() + 30_000;
    const look = () => {
      let text = "";
      try {
        text = readFileSync(logFile, "utf8");
      } catch {
        // not created yet
      }
      const match = text.match(/emu serve: listening on http:\/\/(\S+)/);
      if (match) return resolve(match[1]);
      if (child.exitCode !== null) {
        return reject(new Error(`emu serve for ${id} exited before listening:\n${text.slice(-2000)}`));
      }
      if (Date.now() > deadline) {
        return reject(new Error(`emu serve for ${id} never printed a listen address:\n${text.slice(-2000)}`));
      }
      setTimeout(look, 100).unref?.();
    };
    look();
  });
}

export function stopDoor(door) {
  // By pid, only what this lane started. Never `pkill -f`.
  try {
    process.kill(door.pid, "SIGTERM");
  } catch {
    // already gone
  }
}

/// The door's LIVE registry. `describeBoards()` on the page is a page-load
/// cache and goes stale after a flash (M5 finding); this is the truth.
export async function boardRegistry(addr) {
  const response = await fetch(`http://${addr}/boards`, { signal: AbortSignal.timeout(5_000) });
  if (!response.ok) throw new Error(`GET /boards → ${response.status}`);
  return (await response.json()).boards;
}

// --- the steps ------------------------------------------------------------

/// One scenario's `emulated.steps`, executed against a live Studio. `ctx`
/// carries the driver, the shot directory, and the sink's `awaitRecord`.
export async function runSteps(steps, ctx) {
  const done = [];
  for (const [index, step] of steps.entries()) {
    const label = `${index + 1}/${steps.length} ${step.do}${step.board ? ` ${step.board}` : ""}`;
    process.stdout.write(`  — step ${label}${step.describe ? `: ${step.describe}` : ""}\n`);
    const note = await runStep(step, ctx);
    if (note) process.stdout.write(`      ${note}\n`);
    done.push({ step, note });
  }
  return done;
}

async function runStep(step, ctx) {
  const { driver } = ctx;
  switch (step.do) {
    case "connect": {
      // The one call `browser_esp32_device_controller.js` makes, answered by
      // the page's own chooser. Studio never learns anything is different.
      await driver.clickWhenReady("It's connected", { timeoutMs: STEP_DEADLINE_MS });
      const picked = await driver.pickBoard(step.board, { timeoutMs: STEP_DEADLINE_MS });
      return `picked ${picked} in the in-page chooser`;
    }
    case "cancel-connect": {
      await driver.clickWhenReady("It's connected", { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`Boolean(document.querySelector('#lp-emu-picker'))`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the chooser",
      });
      await driver.click("Cancel");
      return "chooser closed with nothing (NotFoundError, as Chrome's would)";
    }
    case "settle": {
      // The card's own words, not a duration.
      const words = step.words ?? ["Ready", "Blank flash", "Incompatible", "needs firmware", "Gone"];
      const alternatives = words.map((word) => JSON.stringify(word)).join(", ");
      const seen = await driver.waitFor(
        `(() => { const t = document.querySelector('#main')?.innerText || "";
                  const hit = [${alternatives}].find((w) => t.includes(w));
                  return hit || false; })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: `the card to say one of ${alternatives}` },
      );
      return `card says ${JSON.stringify(seen)}`;
    }
    case "mark":
      // Draw a line under everything captured so far, so a later `await`
      // with `"fresh": true` cannot be satisfied by a record from before the
      // step that was supposed to cause it. s8 is the reason this exists: its
      // `expect` is `flow:connecting`, which the FIRST connect already
      // produces, so an emulated lane without this would pass the spec while
      // proving nothing about the re-pick.
      ctx.mark();
      return "later `await fresh` steps ignore everything captured so far";
    case "await": {
      // Event-driven on the SINK, not on the page: the record arriving IS the
      // event. This is the only wait in the lane that is about the trace.
      const record = await ctx.awaitRecord(step.match, STEP_DEADLINE_MS, step.fresh === true);
      return `trace: ${JSON.stringify(record)}`;
    }
    case "flash": {
      const verb = step.verb ?? "Flash firmware";
      await driver.clickWhenReady(verb, { timeoutMs: STEP_DEADLINE_MS });
      return `clicked ${JSON.stringify(verb)}`;
    }
    case "project": {
      await driver.clickWhenReady("to choose from", { timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`Boolean(document.querySelector('[id^="ux-popover-panel"]'))`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the project popover",
      });
      const chosen = await driver.click(step.name, { scope: `document.querySelector('[id^="ux-popover-panel"]')` });
      return `chose ${JSON.stringify(chosen)}`;
    }
    case "push": {
      await driver.clickWhenReady("Put it on the board", { timeoutMs: STEP_DEADLINE_MS });
      return "clicked Put it on the board";
    }
    case "detach":
      return await driver.detach(step.board);
    case "attach":
      return await driver.attach(step.board);
    case "card": {
      // Any other card control, by its visible text (Reset, Disconnect, …).
      const clicked = await driver.clickWhenReady(step.text, { timeoutMs: STEP_DEADLINE_MS });
      return `clicked ${JSON.stringify(clicked)}`;
    }
    case "text": {
      const seen = await driver.waitFor(
        `(document.body.innerText || "").includes(${JSON.stringify(step.contains)})`,
        { timeoutMs: STEP_DEADLINE_MS, what: `the page to say ${JSON.stringify(step.contains)}` },
      );
      return seen ? `page says ${JSON.stringify(step.contains)}` : null;
    }
    case "shot": {
      if (!ctx.shotDir) return "(no shot directory — skipped)";
      const file = path.join(ctx.shotDir, `${step.name}.png`);
      await driver.screenshot(file);
      return `screenshot → ${file}`;
    }
    case "registry": {
      const boards = await boardRegistry(ctx.doorAddr);
      return `door: ${boards.map((b) => `${b.id} flash=${b.flash} boot=${b.boot} reboots=${b.reboots}`).join(" · ")}`;
    }
    default:
      throw new Error(`unknown emulated step \`${step.do}\``);
  }
}

/// Open Studio on the canonical dev server with BOTH flags. They compose:
/// `index.html`'s reader and `device_events_io.rs`'s are two separate parsers
/// over the same query string and neither reads the other's parameter.
export function studioUrlFor({ studioPort, doorAddr, sinkUrl, route = "/devices" }) {
  const query = new URLSearchParams();
  query.set("emu", `ws://${doorAddr}`);
  query.set("capture-sink", sinkUrl);
  return `http://localhost:${studioPort}${route}?${query.toString()}`;
}

export { StudioDriver };
