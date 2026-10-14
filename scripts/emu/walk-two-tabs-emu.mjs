#!/usr/bin/env node
// THE TWO-TABS WALK WITH NO BOARD (M5 of the boards-and-projects roadmap,
// "one tab holds a board"; `just walk-two-tabs-emu`).
//
// Two tabs of ONE headless Chrome — one profile, so one OPFS, one Web Locks
// manager and one `BroadcastChannel` — against ONE emulated ESP32-C6 on an
// `emu serve` door, over `?emu=`:
//
//     tab A holds the board (open, hello, lock, announce; a project on it,
//       its picture in the library)
//     → tab B watches (it never opens the port; its card for the board is
//       the board's own, under Online boards, "Open in another tab", the
//       picture A saved, Connect)
//     → B presses Connect (the take-over): A lets go ("Taken by another
//       tab"), B opens, the board says hello to B
//     → A presses Connect: the mirror image
//     → A's tab closes while it holds: B's card loses "Open in another
//       tab", nothing opens by itself, and Connect opens the board.
//
// WHAT THE SHIM DOES FOR THIS (`?emu-second-tab=1`, `emulator_port.js`): the
// door admits ONE `/control` client per board, and tab A has it, so tab B's
// install would fail (409) as it always has. With the flag, B's port is
// CABLE-LESS: it holds the board's bytes, never its cable — it can list,
// grant, `open()` and `close()` the port (all the hold protocol and the card
// need), and no cable verb works. Real Chrome gives both tabs the cable; this
// walk never asks tab B for one (no detach, attach, reset or D0).
//
// EVERY WAIT IS THE PAGE'S OR THE BOARD'S: a MutationObserver predicate on
// the card, an offer drawn (`data-offer-path`), a bar's words, the board's
// own words in the card's terminal (its hello), a note heard on the hold
// channel, a sidecar appearing in OPFS. No sleeps, and no duration is a
// claim.
//
// ⚠️ WHAT THIS PROVES: the hold protocol, the card and the take-over, end to
// end, against the emulated board (`lp-emu:esp32c6:...`). NOT Chrome's real
// exclusive `open()` across tabs (the door's one-client rule stands in for
// it), not a hidden tab's behaviour (both pages are visible), not a taker
// with a cable. It is not a hardware result; the desk check is queued
// (`docs/adr/2026-10-08-the-board-card-and-one-home-page.md`, section 5
// amendment).
//
// Not a CI job and must not become one. `--serve-release` serves the release
// bundle (`just studio-web-story-build`) and the packaged firmware
// (`just studio-firmware-package-served`) itself, so the walk is one
// foreground command; without it a Studio must already be serving on this
// worktree's canonical port (it never adopts a sibling's).
//
// `--steps 1-3` / `--steps 4-6` run part of the walk: the steps before the
// range run as unreported setup, the steps after it do not run.

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { StudioDriver, boardPath, PANEL } from "./studio-driver.mjs";
import { boardRegistry, serveStudioBundle, startDoor, stopDoor, walkPort } from "./emulated-lane.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const ARGS = process.argv.slice(2);
const BOARD = process.env.WALK_BOARD ?? "c6-a";
const PROJECT = process.env.WALK_PROJECT ?? "Peach (1D)";
const STEP_DEADLINE_MS = 180_000;
const STUDIO_LOAD_DEADLINE_MS = 420_000;
const OUT = path.join(ROOT, "target", "walk-two-tabs-emu");

const STEP_NAMES = ["a-holds", "b-watches", "b-takes-over", "a-takes-it-back", "a-closes", "report"];

function argOf(name) {
  const at = ARGS.indexOf(name);
  return at >= 0 ? ARGS[at + 1] : null;
}

/// `--steps 4-6` → [4, 6]; no flag → [1, 6].
function stepRange() {
  const text = argOf("--steps");
  if (!text) return [1, STEP_NAMES.length];
  const match = /^(\d+)-(\d+)$/.exec(text);
  if (!match || Number(match[1]) < 1 || Number(match[2]) > STEP_NAMES.length || Number(match[1]) > Number(match[2])) {
    console.error(`--steps wants a range like 1-3 or 4-6 (steps are 1..${STEP_NAMES.length}), not ${JSON.stringify(text)}`);
    process.exit(2);
  }
  return [Number(match[1]), Number(match[2])];
}

function git(args) {
  return execFileSync("git", args, { cwd: ROOT, encoding: "utf8" }).trim();
}

function studioPort() {
  return execFileSync("bash", ["scripts/dev-port.sh", "--query", "studio-dev", process.env.STUDIO_WEB_PORT ?? ""], {
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

// --- what each page is told, once it is up ---------------------------------
//
// A page-side log of what the walk needs to order two tabs' doings:
//
//  * every `open()` / `close()` of a shim port (start and end, with the time
//    and the outcome) — so "A closed before B opened" is a fact, and a count
//    of B's `open()`s is `bus.openAttemptsFor`;
//  * every note on the hold channel (`lp-board-holds`), with the time it was
//    heard: a second `BroadcastChannel` object in the same page hears the
//    page's own posts too, so each page's log holds both tabs' notes;
//  * the sidecars in OPFS (`device-frames/<uid>.json`), re-scanned whenever
//    the library says it changed (`lp-library`), which is when a picture
//    lands.
//
// `__lpWalkWait(source, ms)` re-tests a predicate on every log entry and
// every DOM change, so a wait on the log is event-driven like every other.
const INSTRUMENT = `
(() => {
  if (window.__lpWalkLog) return true;
  const log = (window.__lpWalkLog = []);
  const poke = () => window.dispatchEvent(new Event('lp-walk-log'));
  const push = (entry) => { log.push({ ...entry, at: Date.now() }); poke(); };
  const bus = window.__lpEmuSerial.bus;
  const proto = Object.getPrototypeOf(bus.generations[0]);
  for (const verb of ['open', 'close']) {
    const original = proto[verb];
    proto[verb] = async function (...args) {
      push({ ev: verb + '-start', board: this.boardId });
      try {
        const result = await original.apply(this, args);
        push({ ev: verb + '-end', board: this.boardId, ok: true });
        return result;
      } catch (error) {
        push({ ev: verb + '-end', board: this.boardId, ok: false, error: String(error && error.name || error) });
        throw error;
      }
    };
  }
  const notes = new BroadcastChannel('lp-board-holds');
  notes.onmessage = (event) => {
    let parsed = null;
    try { parsed = JSON.parse(event.data); } catch { /* not ours */ }
    if (parsed) push({ ev: 'note', note: parsed.note, tab: parsed.tab });
  };
  // How alive this page's frame loop is: a headless page whose window the
  // browser treats as hidden or occluded stops getting animation frames,
  // and a Studio waiting on one reads as a tab that never answered.
  window.__lpWalkFrames = 0;
  const frame = () => { window.__lpWalkFrames += 1; requestAnimationFrame(frame); };
  requestAnimationFrame(frame);
  window.__lpWalkSidecars = [];
  const scan = async (dir, prefix, found, depth) => {
    for await (const [name, handle] of dir.entries()) {
      const at = prefix + '/' + name;
      if (handle.kind === 'directory') { if (depth < 6) await scan(handle, at, found, depth + 1); }
      else if (/device-frames\\/[^/]+\\.json$/.test(at)) found.push(at);
    }
  };
  const rescan = async () => {
    const found = [];
    try { await scan(await navigator.storage.getDirectory(), '', found, 0); } catch { /* no OPFS yet */ }
    window.__lpWalkSidecars = found;
    poke();
  };
  new BroadcastChannel('lp-library').onmessage = () => { rescan(); };
  rescan();
  window.__lpWalkWait = (source, timeoutMs) => new Promise((resolve, reject) => {
    const test = new Function('return (' + source + ')');
    let done = false;
    const settle = (ok, value) => {
      if (done) return;
      done = true;
      observer.disconnect();
      window.removeEventListener('lp-walk-log', check);
      clearTimeout(timer);
      ok ? resolve(JSON.stringify(value)) : reject(new Error(value));
    };
    const check = () => {
      let value;
      try { value = test(); } catch { return; }
      if (value) settle(true, value === true ? true : value);
    };
    const observer = new MutationObserver(check);
    observer.observe(document.documentElement, { subtree: true, childList: true, characterData: true, attributes: true });
    window.addEventListener('lp-walk-log', check);
    const timer = setTimeout(() => settle(false, 'wait deadline'), timeoutMs);
    check();
  });
  return true;
})()`;

async function instrument(driver) {
  await driver.waitFor("Boolean(window.__lpEmuSerial?.bus?.generations?.length)", { what: "the shim's ports" });
  await driver.evaluate(INSTRUMENT);
}

/// Wait on the page's log, a `/lp-walk-log`-driven predicate.
async function waitLog(driver, source, what, timeoutMs = STEP_DEADLINE_MS) {
  try {
    const json = await driver.evaluate(`window.__lpWalkWait(${JSON.stringify(source)}, ${timeoutMs})`, {
      awaitPromise: true,
      timeoutMs: timeoutMs + 5_000,
    });
    return JSON.parse(json);
  } catch (error) {
    throw new Error(`waiting for ${what}: ${error.message}`);
  }
}

const logOf = async (driver) => (await driver.evaluate(`JSON.stringify(window.__lpWalkLog ?? [])`).then(JSON.parse));

/// The names of the Web Locks held in this browser, as the page sees them.
const locksHeld = (driver) =>
  driver
    .evaluate(`navigator.locks.query().then((q) => JSON.stringify(q.held.map((l) => l.name)))`, { awaitPromise: true })
    .then(JSON.parse);

const openAttempts = (driver) =>
  driver.evaluate(`window.__lpEmuSerial.bus.openAttemptsFor(${JSON.stringify(BOARD)})`);

/// Whether the door's `/bytes` for the board can be claimed right now: a
/// client that connects IS the machine's open (and a disconnect its close),
/// so the answer is the door's own — 101, or 409 — and the probe lets go at
/// once.
function probeBytes(addr) {
  return new Promise((resolve) => {
    const socket = new WebSocket(`ws://${addr}/board/${BOARD}/bytes`);
    let opened = false;
    socket.addEventListener("open", () => {
      opened = true;
      socket.close();
    });
    // Node can raise `error` between its own close() and the close event,
    // so only an error BEFORE the upgrade means the door said 409.
    socket.addEventListener("error", () => {
      if (!opened) resolve(false);
    });
    socket.addEventListener("close", () => resolve(opened));
  });
}

/// The door lets a departed page's claim go when it hears its sockets close,
/// which is not the instant the page's card stops saying "another tab". So:
/// ask again, bounded, and say how long it took. A claim that outlasts the
/// bound is REPORTED (the door's state, printed), never hidden — the
/// 2026-09-24 defect says a departed page's claim is freed; this is where
/// that would show if it were not.
async function awaitBytesUnclaimed(addr, boundMs = 30_000) {
  const t0 = Date.now();
  for (let tries = 1; ; tries += 1) {
    if (await probeBytes(addr)) return { free: true, afterMs: Date.now() - t0, tries };
    if (Date.now() - t0 > boundMs) return { free: false, afterMs: Date.now() - t0, tries };
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}

async function main() {
  mkdirSync(path.join(OUT, "shots"), { recursive: true });
  const [fromStep, toStep] = stepRange();

  const bundle = ARGS.includes("--serve-release")
    ? await serveStudioBundle({ root: ROOT, port: walkPort(ROOT, "walk-two-tabs-emu") })
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
    id: "walk-two-tabs",
    // The packaged whole chip, booted ROM-up from a writable copy: a board
    // that already runs LightPlayer, so there is no flash step.
    boards: [`${BOARD}={merged},kind=rom-up`],
    stateDir: path.join(OUT, "state"),
    consoleDir: path.join(OUT, "console"),
    logFile: path.join(OUT, "serve.log"),
    fresh: true,
  });
  const urlA = `http://localhost:${port}/?emu=${encodeURIComponent(`ws://${door.addr}`)}`;
  const urlB = `${urlA}&emu-second-tab=1`;
  const head = git(["rev-parse", "--short", "HEAD"]);
  const lpEmu = git(["log", "-1", "--format=%h", "--", "lp-emu"]);
  // The emulator names itself: the door's registry row carries its configuration.
  const configuration = (await boardRegistry(door.addr)).find((b) => b.id === BOARD)?.configuration ?? "lp-emu:esp32c6:t1";

  console.log("\nTHE TWO-TABS WALK WITH NO BOARD");
  console.log(`  emulated board   ${BOARD} (packaged fw-esp32c6, ROM-up), door http://${door.addr}/boards`);
  console.log(`  this tree        ${head} (lp-emu ${lpEmu}, ${configuration})`);
  console.log(`  tab A            ${urlA}`);
  console.log(`  tab B            ${urlB}`);
  console.log("  ⚠️  one headless Chrome, two visible windows, one profile; tab B holds the board's bytes, not its cable.");
  console.log(`  steps            ${fromStep}-${toStep} of ${STEP_NAMES.join(", ")}\n`);

  const driverA = await StudioDriver.launch();
  let driverB = null;
  const report = {
    tree: head,
    lpEmu,
    configuration,
    board: BOARD,
    backing: "door (?emu=ws://…), one cable (tab A), tab B cable-less (?emu-second-tab=1)",
    trust:
      "emulated board (lp-emu:esp32c6:…), headless Chrome: proves the hold protocol, the card and the take-over; " +
      "not Chrome's real exclusive open() across tabs, a hidden tab's behaviour, or a taker with a cable. Not a hardware result.",
    steps: [],
    notes: [],
  };
  const state = { mac: null, path: null };
  let alive = { a: true, b: false };

  const shot = async (index, name) => {
    const files = [];
    for (const [tab, driver, up] of [["a", driverA, alive.a], ["b", driverB, alive.b]]) {
      if (!driver || !up) continue;
      const file = path.join(OUT, "shots", `${index}-${name}-${tab}.png`);
      try {
        await driver.screenshot(file);
        files.push(path.relative(ROOT, file));
      } catch {
        /* the page may be gone */
      }
    }
    return files;
  };

  const step = async (index, body) => {
    const name = STEP_NAMES[index - 1];
    if (index < fromStep) {
      console.log(`— ${index} ${name}: (setup, not reported)`);
      await body();
      return;
    }
    if (index > toStep) return;
    console.log(`— ${index} ${name}`);
    let error = null;
    let note = null;
    try {
      note = await body();
    } catch (failure) {
      error = failure;
    }
    const shots = await shot(index, name);
    const frames = {};
    for (const [tab, driver, up] of [["a", driverA, alive.a], ["b", driverB, alive.b]]) {
      if (driver && up) frames[tab] = await driver.evaluate(`window.__lpWalkFrames`).catch(() => null);
    }
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"}${note ? `  ${note}` : ""}`);
    report.steps.push({ index, name, ok: !error, error: error?.message ?? null, note, shots, frames, at: Date.now() });
    if (error) throw error;
  };

  let fatal = null;
  try {
    // ---------------------------------------------------------------- 1
    await step(1, async () => {
      await driverA.navigate(urlA);
      await driverA.awaitShim();
      await driverA.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: STUDIO_LOAD_DEADLINE_MS, what: "Studio (tab A) to load" });
      await instrument(driverA);
      await driverA.pressConnect("USB", { timeoutMs: STEP_DEADLINE_MS });
      await driverA.pickBoard(BOARD, { timeoutMs: STEP_DEADLINE_MS });
      const registry = await boardRegistry(door.addr);
      const mac = registry.find((board) => board.id === BOARD)?.mac;
      if (!mac) throw new Error(`the door's registry has no MAC for ${BOARD}`);
      state.mac = mac;
      state.path = boardPath(mac);
      await driverA.waitCard({ board: mac, timeoutMs: STEP_DEADLINE_MS });
      // Ready, as core reads it; and a project on it so frames flow.
      const face = await driverA.boardRuns({ board: mac, timeoutMs: STEP_DEADLINE_MS });
      if (face === "empty") {
        await driverA.pressOffer("push", { board: mac, timeoutMs: STEP_DEADLINE_MS });
        await driverA.waitFor(`Boolean(${PANEL})`, { what: "the project popover" });
        await driverA.click(PROJECT, { scope: PANEL });
        await driverA.clickWhenReady("Put it on the board", { scope: PANEL, timeoutMs: STEP_DEADLINE_MS });
        await driverA.boardSaid("Project loaded", { board: mac, timeoutMs: STEP_DEADLINE_MS });
      }
      const lockName = `lp-board:usb:303a:1001:${mac.replaceAll(":", "").toLowerCase()}`;
      // The hold, announced on the channel (after open, hello, lock).
      await waitLog(driverA, `window.__lpWalkLog.some((e) => e.ev === 'note' && e.note && e.note.holds)`, "tab A to announce its hold");
      const held = await locksHeld(driverA);
      if (!held.includes(lockName)) throw new Error(`tab A does not hold ${lockName} (locks: ${held.join(", ")})`);
      // The picture A saved, in the library: B shows it from there.
      await waitLog(driverA, `window.__lpWalkSidecars.length > 0`, "the board's picture in the library (device-frames/<uid>.json)");
      const [sidecar] = await driverA.evaluate(`window.__lpWalkSidecars`);
      report.lockName = lockName;
      report.sidecar = sidecar;
      return `${mac} ready (${face}); A holds ${lockName}; picture saved at ${sidecar}`;
    });

    // ---------------------------------------------------------------- 2
    await step(2, async () => {
      driverB = await driverA.openTab();
      alive.b = true;
      await driverB.navigate(urlB);
      await driverB.awaitShim();
      await driverB.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: STUDIO_LOAD_DEADLINE_MS, what: "Studio (tab B) to load" });
      const visible = [await driverA.tabVisible(), await driverB.tabVisible()];
      if (visible.some((v) => v !== "visible")) {
        throw new Error(`both tabs must be visible (the frame feed and the sidecar write only run on a visible page): A ${visible[0]}, B ${visible[1]}`);
      }
      await instrument(driverB);
      const boards = await driverB.boards();
      if (boards?.[0]?.cable !== false) throw new Error(`tab B's port should be cable-less: ${JSON.stringify(boards)}`);
      // Loaded beside a holder, B has not touched the port.
      const untouched = await openAttempts(driverB);
      if (untouched !== 0) throw new Error(`tab B tried the port ${untouched} time(s) before anyone asked it to`);
      // Tab B shares tab A's locks, channels and OPFS: it reads A's lock.
      const held = await locksHeld(driverB);
      if (!held.includes(report.lockName)) throw new Error(`tab B cannot see tab A's lock ${report.lockName} (locks: ${held.join(", ")})`);

      await driverB.pressConnect("USB", { timeoutMs: STEP_DEADLINE_MS });
      await driverB.pickBoard(BOARD, { timeoutMs: STEP_DEADLINE_MS });
      await driverB.waitCard({ board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      await driverB.waitBar("connection", "Open in another tab", { board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      // The port B picked was a chooser grant, which registers it without
      // the sweep's gate (it may be another board of the kind): B asks the
      // door for it and is refused, and reads that refusal against A's claim
      // the moment it arrives, folding the port onto the board's own card.
      // Nothing here waits for that reading: step 3 presses Connect at once,
      // the order a person in a hurry presses it in (defect
      // 2026-10-09-a-chooser-pick-of-a-held-usb-port-is-not-gated).
      const section = await driverB.boardSection({ board: state.mac });
      if (section !== "online") throw new Error(`tab B's card is under ${section ?? "no section"}, not Online boards`);
      await driverB.waitOffer("take-over", { board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      const words = await driverB.offerWords("take-over", { board: state.mac });
      if (words !== "Connect") throw new Error(`the held card's primary is ${JSON.stringify(words)}, not Connect`);
      const picture = await driverB.pictureOf({ board: state.mac });
      if (picture?.source !== "saved" || !picture.frame || !picture.dim) {
        throw new Error(`the held card's picture is not A's saved one, dimmed: ${JSON.stringify(picture)}`);
      }
      // What B's pick cost: the chooser registers the port it picks without
      // the sweep's gate (`request_grant` in `device_effects.rs`), so B may
      // have asked the door for the held port and been refused (or may still
      // be asking). Step 3 checks that none of its opens SUCCEEDED while A
      // held the board.
      const attempts = await openAttempts(driverB);
      report.bAttemptsAfterPick = attempts;
      return `B: Online boards, "Open in another tab", Connect = take-over, picture ${picture.source}${picture.dim ? " (dimmed)" : ""}; B's open() ran ${untouched} time(s) before the pick and ${attempts} after it; B reads A's lock`;
    });

    // ---------------------------------------------------------------- 3
    await step(3, async () => {
      await driverB.pressOffer("take-over", { board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      await driverA.waitBar("connection", "Taken by another tab", { board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      await driverB.waitBar("connection", /^USB/, { board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      const face = await driverB.boardRuns({ board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      if (face !== "running") throw new Error(`the board B opened should still run its project; B reads ${face}`);
      const hello = await driverB.boardSaid(/hello/, { board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      const paths = await driverB.cardPaths();
      if (!paths.includes(state.path)) throw new Error(`tab B's cards ${paths.join(", ")} do not include ${state.path}`);
      // The port B picked ended on the board's own card: no second, "new
      // device" card for the same board.
      if (paths.length !== 1) throw new Error(`tab B shows ${paths.length} cards for one board: ${paths.join(", ")}`);

      // The order, from the pages' own logs: A closed its port, then the
      // release was said on the channel, then B opened.
      const a = await logOf(driverA);
      const b = await logOf(driverB);
      const closed = a.filter((e) => e.ev === "close-end" && e.board === BOARD && e.ok).at(-1);
      const released = b.find((e) => e.ev === "note" && e.note?.answer?.outcome === "released");
      const opens = b.filter((e) => e.board === BOARD && (e.ev === "open-start" || e.ev === "open-end"));
      const firstOk = opens.find((e) => e.ev === "open-end" && e.ok);
      const opened = firstOk && opens.filter((e) => e.ev === "open-start" && e.at <= firstOk.at).at(-1);
      if (!closed || !released || !opened) {
        throw new Error(`the order could not be read: A closed ${Boolean(closed)}, released note ${Boolean(released)}, B open ${Boolean(opened)}`);
      }
      // A closed its port before it said so, and before B opened. (B's page
      // hears the release on two channel objects — its edge and this log's —
      // in no promised order, so B's open may start a millisecond before the
      // log hears the note: the claim is A's close first, not the log's.)
      if (!(closed.at <= released.at && closed.at <= opened.at)) {
        throw new Error(`wrong order: A closed @${closed.at}, released @${released.at}, B opened @${opened.at}`);
      }
      // While A held the board, none of B's opens got it: each was refused.
      const early = opens.filter((e) => e.ev === "open-end" && e.at < released.at);
      if (early.some((e) => e.ok)) throw new Error("one of tab B's opens SUCCEEDED while tab A held the board");
      report.order = { aClosedAt: closed.at, releasedAt: released.at, bOpenedAt: opened.at };
      report.bRefusedBeforeRelease = early.map((e) => e.error);
      // How long the holder took to answer (the asker's patience is 5 s).
      const asked = b.find((e) => e.ev === "note" && e.note?.ask);
      report.answerMs = { ...report.answerMs, bTookFromA: asked ? released.at - asked.at : null };
      noteSlowAnswer(report, "A", report.answerMs.bTookFromA);
      return `A "Taken by another tab"; B ready (${face}), the board said "${hello.slice(0, 60)}"; asked → released in ${report.answerMs.bTookFromA} ms; A closed → released (+${released.at - closed.at} ms) → B opened (${opened.at - released.at >= 0 ? "+" : ""}${opened.at - released.at} ms from the release heard); B's ${early.length} earlier open(s) all refused${early.length ? ` (${early.map((e) => e.error).join(", ")})` : ""}`;
    });

    // ---------------------------------------------------------------- 4
    await step(4, async () => {
      await driverA.pressOffer("take-over", { board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      await driverB.waitBar("connection", "Taken by another tab", { board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      await driverA.waitBar("connection", /^USB/, { board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      const face = await driverA.boardRuns({ board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      if (face !== "running") throw new Error(`A has the board back but reads ${face}`);
      const held = await locksHeld(driverA);
      if (!held.includes(report.lockName)) throw new Error(`tab A does not hold ${report.lockName} again`);
      const a = await logOf(driverA);
      const asked = a.filter((e) => e.ev === "note" && e.note?.ask).at(-1);
      const answered = a.filter((e) => e.ev === "note" && e.note?.answer?.outcome === "released").at(-1);
      report.answerMs = { ...report.answerMs, aTookFromB: asked && answered ? answered.at - asked.at : null };
      noteSlowAnswer(report, "B", report.answerMs.aTookFromB);
      return `B "Taken by another tab"; A live again (${face}) and holds the lock; asked → released in ${report.answerMs.aTookFromB} ms`;
    });

    // ---------------------------------------------------------------- 5
    await step(5, async () => {
      const before = await openAttempts(driverB);
      await driverA.closeTab();
      alive.a = false;
      // The sentinel: B hears the lock go and the fact clears. It does not
      // open anything by itself.
      await driverB.waitFor(
        `(() => { const card = document.querySelector(${JSON.stringify(`[data-board-card="${state.path}"]`)}); if (!card) return false;
                  const el = card.querySelector('[data-bar="connection"] button[aria-label="Connection details"]'); if (!el) return false;
                  const text = (el.textContent || '').replace(/\\s+/g, ' ').trim();
                  return !/another tab/i.test(text) ? text : false; })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: "B's connection bar to lose \"another tab\"" },
      );
      const after = await openAttempts(driverB);
      if (after !== before) throw new Error(`tab B opened the port on its own (open attempts ${before} → ${after})`);
      const connect = await driverB.offerWords("connect", { board: state.mac });
      const takeOver = await driverB.offerWords("take-over", { board: state.mac });
      if (connect !== "Connect" || takeOver !== null) {
        throw new Error(`B's primary should be the ordinary Connect: connect=${JSON.stringify(connect)} take-over=${JSON.stringify(takeOver)}`);
      }
      const door409 = await awaitBytesUnclaimed(door.addr);
      report.bytesFreedAfterMs = door409.afterMs;
      if (!door409.free) {
        const registry = await boardRegistry(door.addr);
        report.notes.push(`the door's /bytes stayed claimed ${door409.afterMs} ms after tab A closed: ${JSON.stringify(registry.find((b) => b.id === BOARD))}`);
        throw new Error(`the door's /bytes is still claimed ${door409.afterMs} ms after tab A closed (a stuck 409, reported — docs/defects/2026-09-24-a-departed-page-kept-the-doors-boards.md)`);
      }
      if (door409.tries > 1) {
        report.notes.push(`the door freed /bytes ${door409.afterMs} ms after tab A's card went (${door409.tries} probes)`);
      }
      await driverB.pressOffer("connect", { board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      const face = await driverB.boardRuns({ board: state.mac, timeoutMs: STEP_DEADLINE_MS });
      const opened = await openAttempts(driverB);
      if (opened !== after + 1) throw new Error(`pressing Connect should open the port once (open attempts ${after} → ${opened})`);
      const held = await locksHeld(driverB);
      if (!held.includes(report.lockName)) throw new Error(`tab B does not hold ${report.lockName} after Connect`);
      return `A closed; B's fact cleared with openAttempts ${before} → ${after} (nothing opened); /bytes unclaimed after ${door409.afterMs} ms; Connect opened it once (${face}) and B holds the lock`;
    });

    // ---------------------------------------------------------------- 6
    await step(6, async () => {
      // The emulator names itself once its seams have engaged (the network
      // seam is soft and on by default): read the name now, not at boot.
      const row = (await boardRegistry(door.addr)).find((b) => b.id === BOARD);
      if (row?.configuration) report.configuration = row.configuration;
      return `report written (${report.configuration})`;
    });
  } catch (error) {
    fatal = error;
  }

  // A failed run says what each tab's card heard from the board and from
  // Studio's own journal (the card's terminal), so a flake is read off the
  // report and not re-run to be understood.
  if (fatal && state.mac) {
    report.terminals = {};
    for (const [tab, driver, up] of [["a", driverA, alive.a], ["b", driverB, alive.b]]) {
      if (!driver || !up) continue;
      report.terminals[tab] = await driver.terminalLines({ board: state.mac, timeoutMs: 15_000 }).catch((e) => [`(no terminal: ${e.message.split("\n")[0]})`]);
      report.terminals[`${tab}Log`] = await logOf(driver).catch(() => []);
      report.terminals[`${tab}Page`] = await driver
        .evaluate(`JSON.stringify({ visibility: document.visibilityState, focused: document.hasFocus(), frames: window.__lpWalkFrames, now: Date.now() })`)
        .then(JSON.parse)
        .catch(() => null);
    }
  }

  const lines = (driver) => (driver ? driver.consoleLines() : []);
  const consoleOf = { a: lines(driverA), b: lines(driverB) };
  const errors = (all) => all.filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  const panics = [...errors(consoleOf.a), ...errors(consoleOf.b)].filter((l) => l.includes("panicked at"));
  report.consoleErrors = { a: errors(consoleOf.a), b: errors(consoleOf.b) };
  report.verdict = fatal ? `FAILED: ${fatal.message.split("\n")[0]}` : panics.length ? `steps passed, ${panics.length} page panic(s)` : "PASSED";
  writeFileSync(path.join(OUT, "report.json"), JSON.stringify(report, null, 2));
  writeFileSync(path.join(OUT, "page-console-a.log"), consoleOf.a.join("\n") + "\n");
  writeFileSync(path.join(OUT, "page-console-b.log"), consoleOf.b.join("\n") + "\n");
  writeFileSync(path.join(OUT, "report.md"), markdown(report));

  console.log("\n=== the two-tabs walk, step by step");
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${String(s.index)} ${s.name}`);
  console.log(`\n  ${report.configuration}, lp-emu ${lpEmu}, tree ${head}: emulated, not hardware`);
  console.log(`  report → ${path.relative(ROOT, path.join(OUT, "report.md"))}`);
  for (const [tab, list] of [["A", report.consoleErrors.a], ["B", report.consoleErrors.b]]) {
    if (list.length) {
      console.log(`\n  tab ${tab} console errors:`);
      for (const line of list.slice(-6)) console.log(`    ${line.slice(0, 240)}`);
    }
  }

  await driverA.close();
  await stopDoor(door);
  bundle?.close();

  if (fatal) {
    console.error(`\nThe two-tabs walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  if (panics.length) {
    console.error(`\nThe walk's steps passed, but a page panicked ${panics.length} time(s).`);
    process.exit(1);
  }
  console.log(
    `\n✓ the two-tabs walk finished (steps ${fromStep}-${toStep}): one tab held the board, the other watched, took it over, gave it back, and picked it up when the holder closed — with no board.`,
  );
}

/// A holder's answer should come well inside a second (the asker waits 5 s;
/// docs/defects/2026-10-09-a-holders-release-can-outlast-the-askers-five-seconds.md).
/// Page wall-clock on an emulated board is never a gate, so a slow answer is
/// a note in the report, not a failure.
function noteSlowAnswer(report, holder, ms) {
  if (ms !== null && ms > 1_000) {
    report.notes.push(`tab ${holder}'s answer took ${ms} ms (over 1 s; the asker waits 5 s)`);
  }
}

function markdown(report) {
  const rows = report.steps.map((s) => `| ${s.index} | ${s.name} | ${s.ok ? "pass" : "FAIL"} | ${(s.error ?? s.note ?? "").replace(/\|/g, "\\|")} |`);
  return `# The two-tabs walk (one tab holds a board)

**Verdict: ${report.verdict}**

- Configuration: \`${report.configuration}\` (emulated ESP32-C6, \`${report.board}\`), lp-emu \`${report.lpEmu}\`, tree \`${report.tree}\`
- Backing: ${report.backing}
- Trust: ${report.trust}

| # | step | result | what it saw |
|---|---|---|---|
${rows.join("\n")}

${report.order ? `Order in step 3 (page clocks, ms): A closed its port at ${report.order.aClosedAt}, the release was heard at ${report.order.releasedAt}, B opened at ${report.order.bOpenedAt}.\n` : ""}${report.answerMs ? `Holder's answer time (ask → released, ms; the asker waits 5000): ${JSON.stringify(report.answerMs)}.\n` : ""}${report.bAttemptsAfterPick !== undefined ? `Tab B's open() ran ${report.bAttemptsAfterPick} time(s) after its chooser pick, all refused (${(report.bRefusedBeforeRelease ?? []).join(", ") || "n/a"}): a chooser grant registers a port without the sweep's gate. The sweep's gate (a port granted at load) is core's T1/T3: zero opens.\n` : ""}
${report.notes.length ? "Notes:\n" + report.notes.map((n) => `- ${n}`).join("\n") + "\n" : ""}`;
}

await main();
