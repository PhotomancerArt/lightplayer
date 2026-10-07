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
//     streamed to the same `?record=`; this lane never writes a record
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
// the query string, which composes with `?record=` because nothing
// reads anything else's flag.

import { execFileSync, spawn } from "node:child_process";
import { createReadStream, existsSync, mkdirSync, openSync, readFileSync, rmSync, statSync } from "node:fs";
import { createServer } from "node:http";
import { createServer as createNetServer } from "node:net";
import path from "node:path";
import process from "node:process";

import { StudioDriver } from "./studio-driver.mjs";

/// The packaged C6 firmware ELF `studio-dev-emu` boots its boards from — the
/// same build `studio-firmware-package-served` leaves behind, so a board that
/// starts "already running LightPlayer" is running the build this Studio
/// serves. `{fw}` in a spec's board line substitutes to this.
export const PACKAGED_C6_ELF = "target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6";

/// The same build as the whole chip `studio-firmware-package-served` packs:
/// bootloader, table and the image. Locally that is a single image by
/// default (`LP_FW_IMAGE=split` for the product's split one, whose ELF
/// above, direct-loaded over a blank chip, boots its core only — "engine
/// does not fit": its engine lives in flash).
/// `{merged}` in a board line substitutes to this; spell it
/// `<id>={merged},kind=rom-up` for a writable chip seeded with it, booted
/// from the reset vector the way a flashed board is.
export const PACKAGED_C6_MERGED = "target/studio-web-assets/firmware/esp32c6-4mb/fw-esp32c6-merged.bin";

/// How long a wedged step may hang before the run is failed. NEVER an
/// assertion: nothing in this milestone concludes anything from elapsed time,
/// and an agent-driven tab is throttled to ~1 Hz anyway.
const STEP_DEADLINE_MS = 90_000;

// --- the door -------------------------------------------------------------

/// Start an `lp-cli emu serve` holding this scenario's boards and return its
/// address. Ephemeral port, printed and read — never computed.
///
/// `extraArgs` go on the command line after the boards (the Wi‑Fi LAN walk's
/// `--lan <name>=<fixture>`); every other caller passes none.
export async function startDoor({ root, id, boards, stateDir, consoleDir, logFile, fresh = true, extraArgs = [] }) {
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
  for (const board of boards) {
    args.push("--board", board.replaceAll("{fw}", PACKAGED_C6_ELF).replaceAll("{merged}", PACKAGED_C6_MERGED));
  }
  args.push(...extraArgs);
  args.push("--listen", "127.0.0.1:0", "--state-dir", stateDir, "--console-dir", consoleDir);
  // TRUNCATED, not appended. The listen address is read back out of this
  // file, and an appended log still holds the PREVIOUS run's address — which
  // reads as a door that is up and answers every fetch with a dead socket.
  const fd = openSync(logFile, "w");
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
      // Deliberately NOT unref'd: this is the only thing keeping the event
      // loop alive in a caller that has not started a server yet, and an
      // unref'd timer here let node exit before the door had spoken.
      setTimeout(look, 100);
    };
    look();
  });
}

export async function stopDoor(door, { timeoutMs = 30_000 } = {}) {
  // By pid, only what this lane started. Never `pkill -f`.
  //
  // SIGINT, and wait for the exit: the door writes every board's flash and
  // console back on Ctrl-C (the shutdown path of `emu serve`) and NOT on
  // SIGTERM, which kills it where it stands. A caller that reads the chip
  // file after a SIGTERM reads the last two-second write-back instead — a
  // snapshot that can sit in the middle of a write the board finished (the
  // migration walk's W7b read a board-manifest stamp half done that way).
  const alive = () => {
    try {
      process.kill(door.pid, 0);
      return true;
    } catch {
      return false;
    }
  };
  try {
    process.kill(door.pid, "SIGINT");
  } catch {
    return; // already gone
  }
  const deadline = Date.now() + timeoutMs;
  while (alive()) {
    if (Date.now() > deadline) {
      console.error(`stopDoor: emu serve (pid ${door.pid}) did not exit on SIGINT; killing it`);
      try {
        process.kill(door.pid, "SIGTERM");
      } catch {
        // gone in between
      }
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
}

// --- the release bundle, served by the walk itself -----------------------

const BUNDLE_TYPES = { ".html": "text/html", ".js": "text/javascript", ".mjs": "text/javascript", ".wasm": "application/wasm", ".css": "text/css", ".json": "application/json", ".svg": "image/svg+xml", ".bin": "application/octet-stream", ".woff2": "font/woff2", ".png": "image/png" };

/// Where `just studio-web-story-build` leaves the release Studio bundle (its
/// sidecars included — the emulator module the tab lane runs), and where
/// `just studio-firmware-package-served` leaves the firmware it flashes.
export const RELEASE_BUNDLE = "target/dx/lpa-studio-web/release/web/public";
export const SERVED_FIRMWARE = "target/studio-web-assets/firmware";

/// This worktree's stable port for `slot` (`scripts/dev-port.sh`): stable,
/// not ephemeral, because a browser's OPFS belongs to the ORIGIN and a walk
/// that comes back in a new Chrome expects the same one.
export function walkPort(root, slot) {
  return Number(execFileSync("bash", ["scripts/dev-port.sh", slot], { cwd: root, encoding: "utf8" }).trim());
}

/// Serve the release Studio bundle and the packaged firmware on 127.0.0.1:
/// `port`, the way the dev server serves them — no `dx serve` process, so a
/// walk can run as one foreground command and never adopts a sibling
/// worktree's listener. `route(request, response, url)` answers first and
/// returns true for anything it handled (a walk's own endpoints).
export function serveStudioBundle({ root, port, route = null }) {
  const publicDir = path.join(root, RELEASE_BUNDLE);
  const firmwareDir = path.join(root, SERVED_FIRMWARE);
  for (const [what, at] of [["the release Studio bundle (just studio-web-story-build)", publicDir], ["the packaged firmware (just studio-firmware-package-served)", firmwareDir]]) {
    if (!existsSync(at)) throw new Error(`missing ${what}: ${at}`);
  }
  const server = createServer((request, response) => {
    const url = new URL(request.url, "http://x");
    if (route?.(request, response, url)) return;
    let file = null;
    const firmware = url.pathname.match(/^\/firmware\/([^/]+)\/([^/]+)$/);
    if (firmware) file = path.join(firmwareDir, firmware[1], firmware[2]);
    else {
      const candidate = path.join(publicDir, decodeURIComponent(url.pathname));
      file = candidate.startsWith(publicDir) && existsSync(candidate) && statSync(candidate).isFile()
        ? candidate
        : path.join(publicDir, "index.html"); // the SPA's routes
    }
    if (!existsSync(file)) {
      response.writeHead(404);
      response.end();
      return;
    }
    response.writeHead(200, { "content-type": BUNDLE_TYPES[path.extname(file)] ?? "application/octet-stream" });
    createReadStream(file).pipe(response);
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(port, "127.0.0.1", () => resolve(server));
  });
}

/// The door's LIVE registry. `describeBoards()` on the page is a page-load
/// cache and goes stale after a flash (M5 finding); this is the truth.
export async function boardRegistry(addr) {
  const response = await fetch(`http://${addr}/boards`, { signal: AbortSignal.timeout(5_000) });
  if (!response.ok) throw new Error(`GET /boards → ${response.status}`);
  return (await response.json()).boards;
}

/// A board's USB door as a plain TCP socket on 127.0.0.1:<ephemeral>, for
/// the tools that speak `tcp://` and not the door's WebSocket (`lp-cli link
/// capture tcp://…`, which hosts the link and writes the board's DECODED
/// console: its log records ride the link, so a raw read holds none).
///
/// A TCP connect opens `/board/<id>/bytes` and a disconnect closes it, so
/// the door's "a connect IS the port's open" rule carries through. Bytes
/// both ways and nothing else; the door still admits one client per board.
export function bridgeDoorBytes({ doorAddr, board }) {
  const url = `ws://${doorAddr}/board/${board}/bytes`;
  const server = createNetServer((socket) => {
    const ws = new WebSocket(url);
    ws.binaryType = "arraybuffer";
    const pending = [];
    ws.addEventListener("open", () => {
      for (const chunk of pending.splice(0)) ws.send(chunk);
    });
    ws.addEventListener("message", (event) => {
      socket.write(Buffer.from(event.data)); // an ArrayBuffer (binaryType), or a text frame
    });
    ws.addEventListener("close", () => socket.destroy());
    ws.addEventListener("error", () => socket.destroy());
    socket.on("data", (chunk) => {
      if (ws.readyState === WebSocket.OPEN) ws.send(chunk);
      else pending.push(chunk);
    });
    socket.on("close", () => ws.close());
    socket.on("error", () => ws.close());
  });
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", () => resolve({ server, port: server.address().port, url }));
  });
}

/// The `?record=` sink: every device-event line Studio streams, in order, so
/// each step can say which records IT produced (walk-no-board's sink, shared).
/// `awaitRecord(test, timeoutMs, from, what)` resolves on the first record
/// from index `from` on that passes `test` — the record arriving is the event.
export function startRecordSink() {
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
    request.on("data", (chunk) => {
      body += chunk;
    });
    request.on("end", () => {
      for (const line of body.split("\n")) {
        if (!line.trim()) continue;
        raw.push(line);
        try {
          const record = JSON.parse(line);
          records.push(record);
          offer(record);
        } catch {
          // keep the raw line
        }
      }
      response.writeHead(204, { "access-control-allow-origin": "*" });
      response.end();
    });
  });
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
      waiter.resolve = (record) => {
        clearTimeout(timer);
        resolve(record);
      };
    });
  const listen = () =>
    new Promise((resolve) =>
      server.listen(0, "127.0.0.1", () => resolve(`http://127.0.0.1:${server.address().port}/ingest`)),
    );
  return { server, records, raw, awaitRecord, listen };
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
      await driver.clickWhenReady("via USB", { timeoutMs: STEP_DEADLINE_MS });
      const picked = await driver.pickBoard(step.board, { timeoutMs: STEP_DEADLINE_MS });
      return `picked ${picked} in the in-page chooser`;
    }
    case "cancel-connect": {
      await driver.clickWhenReady("via USB", { timeoutMs: STEP_DEADLINE_MS });
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
///
/// `doorAddr: null` is the TAB backing (`?emu=tab`): the emulator runs in a
/// Worker in the page and there is no address to name. Everything else about
/// the lane is unchanged, which is the point of the spelling.
export function studioUrlFor({ studioPort, doorAddr = null, sinkUrl, route = "/devices" }) {
  const query = new URLSearchParams();
  query.set("emu", doorAddr ? `ws://${doorAddr}` : "tab");
  query.set("record", sinkUrl);
  return `http://localhost:${studioPort}${route}?${query.toString()}`;
}

/// The LIVE registry, whichever backing is holding the boards.
///
/// The door answers `GET /boards`; the tab answers `listBoards()` on its
/// backing, which reads the worker's own `stats` row. `describeBoards()` on
/// the page is neither — it is a page-load cache and goes stale after a
/// flash (M5 finding), which is exactly what this exists to avoid.
export async function liveRegistry({ doorAddr = null, driver = null }) {
  if (doorAddr) return boardRegistry(doorAddr);
  if (!driver) throw new Error("liveRegistry needs a door address or a driver");
  const json = await driver.evaluate(
    `window.__lpEmuSerial?.tabBacking
       ? window.__lpEmuSerial.tabBacking.listBoards().then((b) => JSON.stringify(b))
       : Promise.resolve("null")`,
    { awaitPromise: true },
  );
  const boards = JSON.parse(json ?? "null");
  if (!boards) throw new Error("the page is not holding a tab backing");
  return boards;
}

export { StudioDriver };
