#!/usr/bin/env node
// THE WALK WITH NO BOARD (emulator plan two, M6; `just walk-no-board`).
//
// One continuous Studio session, in a headless browser, against an emulated
// ESP32-C6 that nothing on the desk is plugged into:
//
//     flash → connect → identify → upload a project
//       → card-connect → card-panel → card-all-controls → card-done
//       → card-edit → card-back → card-edit-again
//       → detach → re-attach
//
// The first four and the last two are `plan.md`'s acceptance criterion 6
// verbatim, and this is the artefact G2 looks at. It is deliberately NOT six
// scenarios in a row: the point is that one board, one page and one grant
// carry all the way through, because that is where the defects this
// instrument exists for live.
//
// The seven `card-*` steps are the connected card (roadmap M4,
// `lp2025/2026-10-08-2330-connected-in-the-card` P7): Connect on the card
// turns its bars into the board's panel with the page staying on `/`; a
// control written there; All controls (the play page) and back; Done; Edit
// on the watched card connects first; Back keeps the session; Edit on the
// All controls row reopens nothing; Done on the editor's docked card. Each
// one waits on the board's words, never on a string Studio could satisfy
// itself: a panel control turns held at once from Studio's own echo, so
// "held" is read in a FRESH session (after Done), where the only source is
// the board's panel state; and "nothing reopened" is the absence of a new
// `pool install` device event.
//
// It is NOT a CI job and must not become one (PD9, pre-ruled E-cost): it
// wants Chrome, a dev server, a packaged firmware and an emulator. It is a
// recipe an agent runs. `--serve-release` takes the dev server out: the walk
// serves the release bundle (`just studio-web-story-build`) and the packaged
// firmware itself, so a session that cannot keep a server running in the
// background can still run it as one foreground command.
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
//
// A Rust panic in the page fails the walk even when all six steps pass: a
// step only sees the surface it drives, so the code that panics is by
// definition somewhere none of them is looking.

import { createServer } from "node:http";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";
import { execSync } from "node:child_process";

import { PANEL, StudioDriver, boardPath, macOfPath } from "./studio-driver.mjs";
import { liveRegistry, serveStudioBundle, startDoor, stopDoor, studioUrlFor, walkPort } from "./emulated-lane.mjs";

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
/// Studio…`, and the verdict said the card never offered `It's connected` (today's USB square)
/// (measured 2026-09-10 on the tab lane, with the machine otherwise busy).
/// Wait for the app to exist, then start timing the board.
const STUDIO_LOAD_DEADLINE_MS = 420_000;

function args() {
  const tabByDefault = (process.env.WALK_BACKING ?? "") === "tab";
  const out = {
    shots: null,
    out: null,
    keepOpen: false,
    serveRelease: false,
    board: tabByDefault ? "tab-c6" : "c6-a",
    tab: tabByDefault,
  };
  let boardNamed = false;
  const argv = process.argv.slice(2);
  for (let i = 0; i < argv.length; i += 1) {
    if (argv[i] === "--shots") out.shots = argv[++i];
    else if (argv[i] === "--out") out.out = argv[++i];
    else if (argv[i] === "--keep-open") out.keepOpen = true;
    else if (argv[i] === "--serve-release") out.serveRelease = true;
    else if (argv[i] === "--tab") out.tab = true;
    else if (argv[i] === "--board") {
      out.board = argv[++i];
      boardNamed = true;
    } else {
      console.error(
        `usage: node scripts/emu/walk-no-board.mjs [--tab] [--serve-release] [--shots <dir>] [--out <dir>] [--keep-open]`,
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

/// Which section of the home page holds the board right now, REPORTED and
/// never asserted: whether a cable-out leaves a card under Online boards
/// ("Reconnect…") or moves it to Offline boards depends on the board's state
/// (`walk-ota-emu.mjs` says so where it waits for the same thing), so a claim
/// here would be a claim about the roster's timing. Finds the board's card
/// by its hook (`data-board-card="devices/mac-<hex>"`) inside
/// `#home-offline-boards` / `#home-online-boards`; with one board in the
/// walk and no MAC known, the only section on the page is where it sits.
/// Answers `offline`, `online`, `neither` (no section on the page) or
/// `unmatched` (both are there and neither holds the card).
///
/// Waits, bounded, for Offline boards to appear before it answers, so a card
/// still on its way there is not reported as staying: a page-side wait that
/// ends the moment the section exists, or at the deadline with whatever is
/// there.
async function sectionOf(driver, mac) {
  await driver
    .waitFor(`Boolean(document.querySelector('#home-offline-boards'))`, {
      timeoutMs: 10_000,
      what: "Offline boards to appear",
    })
    .catch(() => null);
  const card = mac ? `[data-board-card="${boardPath(mac)}"]` : null;
  return driver.evaluate(`(() => {
    const card = ${JSON.stringify(card)};
    const found = { offline: document.querySelector('#home-offline-boards'),
                    online: document.querySelector('#home-online-boards') };
    const present = Object.keys(found).filter((name) => found[name]);
    const named = present.find((name) => card && found[name].querySelector(card));
    if (named) return named;
    if (present.length === 0) return 'neither';
    return present.length === 1 ? present[0] : 'unmatched';
  })()`);
}

async function reportSection(driver, mac, when) {
  const section = await sectionOf(driver, mac);
  console.log(`  the board's section ${when}: ${section}   (reported, not asserted)`);
  return section;
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

  // `--serve-release`: no dev server — this walk serves the release bundle
  // (`just studio-web-story-build`) and the packaged firmware itself, on its
  // own stable slot, so it runs as one foreground command. It never looks
  // at, or adopts, any other listener.
  const bundle = options.serveRelease
    ? await serveStudioBundle({ root: ROOT, port: walkPort(ROOT, "walk-no-board") })
    : null;
  const port = bundle ? bundle.address().port : studioPort();
  if (!bundle && !(await studioUp(port))) {
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
  // The walk opens Studio at the LEGACY address on purpose: `/devices` is no
  // page any more, it parses as Home and heals to `/` with the whole query
  // kept (PAC2), and this is the one place that is proven live. `wire=packed`
  // is the documented default spelled out, a no-op, and it is NOT one of the
  // page-load flags (`record`, `emu`, `ble`), so a heal that filtered the
  // query through `with_page_flags` would drop it and the check below would
  // say so.
  const url =
    studioUrlFor({ studioPort: port, doorAddr: door?.addr ?? null, sinkUrl, route: "/devices" }) +
    "&wire=packed";

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
  /// The address the page healed to (the alias proof), and the MAC the
  /// identify step read; the section reports below match on it.
  let healed = null;
  let boardMac = null;
  /// Which section of the home page the board sat in after the cable came
  /// out, and after it went back in: REPORTED, not asserted (see `sectionOf`).
  const sections = {};
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
    // THE ALIAS (PAC2): the legacy address must have healed to `/`, with the
    // query it was opened with. A page-side wait, not a timer: the router
    // rewrites the address once the app has mounted, which it has.
    healed = await driver
      .waitFor(
        `(() => { const q = new URLSearchParams(location.search);
                  return location.pathname === '/' && q.get('wire') === 'packed'
                    && q.get('emu') !== null && q.get('record') !== null
                    ? location.pathname + location.search : false; })()`,
        { timeoutMs: 30_000, what: "the legacy address to heal to `/`" },
      )
      .catch(async () => {
        const now = await driver.evaluate(`location.pathname + location.search`).catch(() => "(unreadable)");
        throw new Error(`the legacy address did not heal to \`/\` with its flags: the page is at ${now}`);
      });
    console.log(`  the legacy address healed: /devices?… → ${healed.split("?")[0]} with wire=packed, emu and record kept`);

    // An instrument that lies about its own conditions is worse than none:
    // a hidden page throttles its timers and its Workers, so a walk run in
    // one measures the throttle. Say which it was, every time.
    const visibility = await driver.evaluate(
      `JSON.stringify({ hidden: document.hidden, state: document.visibilityState })`,
    );
    console.log(`  studio is up — page visibility: ${visibility}\n`);

    // 1. FLASH — Studio's own esptool-js flow, into a chip with nothing on it.
    await step("flash", "Studio flashes the packaged firmware into a blank board", async () => {
      await driver.pressConnect("USB", { timeoutMs: STEP_DEADLINE_MS });
      await driver.pickBoard(options.board, { timeoutMs: STEP_DEADLINE_MS });
      // The blank board's card: its firmware bar says "No firmware", and
      // core offers it `flash` — the name bar's Install, drawn as the board
      // pick. The flash is the firmware bar's work, and it has to START and
      // then FINISH: "the bar stopped saying `No firmware`" alone is
      // satisfied the instant the work starts, which screenshotted a
      // progress bar and called it a flash (`StudioDriver.flashBlank`).
      await driver.flashBlank(BOARD_MODEL, { timeoutMs: STEP_DEADLINE_MS, flashTimeoutMs: FLASH_DEADLINE_MS });
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
      // Identifying is the connection bar's work; once the board has said
      // hello, core offers it `push` (an empty board's "Add a project").
      await driver.waitFor(
        `(() => { const card = document.querySelector('[data-board-card]'); if (!card) return false;
                  return card.querySelector('[data-bar="connection"]')?.getAttribute('data-bar-work') === 'running'
                    || Boolean(card.querySelector('[data-offer-path$="/push"]')); })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the card to reconnect" },
      );
    });

    await step("identify", "the board says what it is, in its own hello", async () => {
      // Ready, as core reads it: the board is offered a project.
      await driver.waitOffer("push", { timeoutMs: STEP_DEADLINE_MS });
      // The firmware's identity is the firmware details' Version: the label
      // the board's own hello carried (the bar's summary may be saying
      // something more pressing about the board's files). The MAC is the
      // card's ref, which core keys by it.
      const identity = await driver.detailsFact("firmware", "Version", { timeoutMs: STEP_DEADLINE_MS });
      const ref = await driver.waitCard();
      const mac = macOfPath(ref);
      console.log(`  identity: ${identity}   card: ${ref}   mac: ${mac}`);
      if (!identity || !identity.startsWith("fw-esp32c6")) {
        throw new Error(`the card's firmware bar never named the firmware: ${JSON.stringify(identity)}`);
      }
      steps.identity = identity;
      boardMac = mac;
    });

    // 4. UPLOAD — through Studio, as the criterion says, not the CLI.
    await step("upload", `push ${WALK_PROJECT} onto the board from the gallery`, async () => {
      // `push`: the empty board's project bar action, "Add a project", drawn
      // as the project pick.
      await driver.pressOffer("push", { board: boardMac, timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`Boolean(${PANEL})`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the project popover",
      });
      const before = sink.records.length;
      await driver.click(WALK_PROJECT, { scope: PANEL });
      await driver.clickWhenReady("Put it on the board", { scope: PANEL, timeoutMs: STEP_DEADLINE_MS });
      // NOT "the page mentions the project", which the CHOOSER's own button
      // already satisfies the moment it is picked — a predicate written that
      // way passed instantly and screenshotted a card that still read
      // "Nothing loaded" with an empty `loaded_projects` in the board's next
      // heartbeat. And not "Nothing loaded went away" either: that happens
      // when the push STARTS. The push's own end, in the device model's
      // journal, is the only thing that carries an outcome.
      // THE BOARD'S OWN WORDS decide this step, and they are on the board's
      // terminal, in the status corner's details, streamed off the wire:
      // `Project loaded` is the firmware saying it. That is the gate.
      await driver.boardSaid("Project loaded", { board: boardMac, timeoutMs: STEP_DEADLINE_MS });

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

    // THE CONNECTED CARD (M4). One session on the card, carried through the
    // editor and back. `pool` device events are the sessions themselves: an
    // `install` is a session opening, a `remove` one closing.
    const card = `document.querySelector(${JSON.stringify(`[data-board-card="${boardPath(boardMac)}"]`)})`;
    const poolCount = (action) =>
      sink.records.filter((record) => record.kind === "pool" && record.action === action).length;
    const poolRecord = (action) => (record) => record.kind === "pool" && record.action === action;
    /// The control the walk writes on the card, by its identity (scope and
    /// channel): read held in the next session, then let go.
    let written = null;
    const writtenSelector = () =>
      `[data-panel-scope=${JSON.stringify(written.scope)}][data-panel-channel=${JSON.stringify(written.channel)}]`;
    /// The editor, as a page shows it: a lens address, and the workbench's
    /// Play toggle mounted.
    const waitEditor = (what) =>
      driver.waitFor(
        `(location.pathname.startsWith('/p/') || location.pathname.startsWith('/device/'))
          && Boolean(document.querySelector('a[title^="Play mode"]'))
          ? location.pathname + location.search : false`,
        { timeoutMs: STEP_DEADLINE_MS, what },
      );
    const cardPanel = (what) =>
      driver.waitFor(
        `(() => { const c = ${card}; return location.pathname === '/'
                  && Boolean(c?.querySelector('[data-board-panel] [data-panel-channel]')); })()`,
        { timeoutMs: STEP_DEADLINE_MS, what },
      );
    const sessions = {};

    await step("card-connect", "Connect on the board's card: its bars become its panel, on the home page", async () => {
      const installs = poolCount("install");
      const before = sink.records.length;
      // Connect, the card's primary on a ready board running a project.
      await driver.pressOffer("connect", { board: boardMac, timeoutMs: STEP_DEADLINE_MS });
      // Its controls come from the board's own project read.
      await cardPanel("the board's panel on its card, the page still at `/`");
      await sink.awaitRecord(poolRecord("install"), 30_000, before, "the session's `pool install`");
      const controls = await driver.evaluate(
        `[...${card}.querySelectorAll('[data-board-panel] [data-panel-channel]')]
          .map((el) => el.getAttribute('data-panel-scope') + ' ' + el.getAttribute('data-panel-channel') + ' ' + el.getAttribute('data-panel-state'))`,
      );
      const bars = await driver.evaluate(`${card}.querySelectorAll('[data-bar]').length`);
      if (bars !== 0) throw new Error(`the connected card still draws ${bars} bar(s)`);
      sessions.connect = poolCount("install") - installs;
      if (sessions.connect !== 1) throw new Error(`Connect opened ${sessions.connect} session(s), not one`);
      console.log(`  the card's panel: ${controls.join(" · ")}   (pool install +1, the address still /)`);
      // AC6: the connected card's picture is the session's own frames, live
      // — the corner reads a frame rate, not an age. Bounded, not timed.
      const corner = `${card}?.querySelector('[data-board-corner]')`;
      const live = await driver
        .waitFor(
          `(() => { const p = ${card}?.querySelector('[data-picture]'); const c = ${corner};
                    const reading = (c?.textContent || '').replace(/\\s+/g, ' ').trim();
                    return p?.getAttribute('data-picture') === 'lens' && /fps/.test(reading) ? reading : false; })()`,
          { timeoutMs: 60_000, what: "the card's picture to be the session's own, live (the corner reading fps)" },
        )
        .catch(async (error) => {
          const now = await driver.evaluate(
            `(() => { const p = ${card}?.querySelector('[data-picture]');
                      return (p?.getAttribute('data-picture') ?? 'none') + ' · ' + ((${corner})?.textContent || '').replace(/\\s+/g, ' ').trim(); })()`,
          );
          throw new Error(`${error.message.split("\n")[0]} — the picture reads ${now}`);
        });
      console.log(`  the card's picture: the session's own, live — the corner reads ${live}`);
    });

    await step("card-panel", "write the card's first control to its far end", async () => {
      const before = sink.records.length;
      written = await driver.evaluate(`(() => {
        const control = ${card}.querySelector('[data-board-panel] [data-panel-channel]');
        const out = { scope: control.getAttribute('data-panel-scope'), channel: control.getAttribute('data-panel-channel') };
        const knob = control.querySelector('[role="slider"]');
        const range = control.querySelector('input[type="range"]');
        const toggle = control.querySelector('button[role="switch"]');
        if (knob) {
          knob.focus();
          out.how = Number(knob.getAttribute('aria-valuenow')) >= Number(knob.getAttribute('aria-valuemax')) ? 'Home' : 'End';
          knob.dispatchEvent(new KeyboardEvent('keydown', { key: out.how, bubbles: true, cancelable: true }));
        } else if (range) {
          const to = Number(range.value) >= Number(range.max) ? range.min : range.max;
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(range, to);
          range.dispatchEvent(new Event('input', { bubbles: true }));
          out.how = 'to ' + to;
        } else if (toggle) {
          toggle.click();
          out.how = 'flipped';
        }
        return out;
      })()`);
      if (!written.how) throw new Error(`the card's first control (${written.channel}) has no widget the walk can turn`);
      // The write going out and the board answering it (the dispatched
      // action awaits the board's reply). That it HOLDS is read in the next
      // session (card-edit): here the control reads held from Studio's own
      // echo, which proves nothing.
      const record = await sink.awaitRecord(
        (r) => r.kind === "action" && /PanelWrite/.test(r.name ?? ""),
        60_000,
        before,
        "the panel write's dispatch",
      );
      if (record.outcome !== "ok") throw new Error(`the panel write failed: ${record.error}`);
      console.log(`  wrote ${written.scope} ${written.channel} (${written.how}); ${record.name} ${record.outcome}`);
    });

    await step("card-all-controls", "All controls: the board's play page on the same session; Back to its card", async () => {
      const installs = poolCount("install");
      const href = await driver.evaluate(`${card}.querySelector('[data-all-controls] a')?.getAttribute('href') ?? null`);
      if (!href) throw new Error("the All controls row is not a link");
      await driver.evaluate(`${card}.querySelector('[data-all-controls] a').click()`);
      const play = await driver.waitFor(
        `location.pathname.endsWith('/play') && Boolean(document.querySelector('#main [data-panel-channel]'))
          ? location.pathname + location.search : false`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the play page and its panel" },
      );
      if (poolCount("install") !== installs) throw new Error("All controls opened a second session");
      await driver.evaluate("history.back()");
      await cardPanel("the card's panel again, back at `/`");
      if (poolCount("install") !== installs) throw new Error("going back home opened a session");
      sessions.allControls = { href, play };
      console.log(`  All controls → ${play}  (href ${href}); back at /, no new session`);
    });

    await step("card-done", "Done: the session closes and the card shows its five bars", async () => {
      const before = sink.records.length;
      await driver.pressOffer("done", { board: boardMac, timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(
        `(() => { const c = ${card}; return Boolean(c) && !c.querySelector('[data-board-panel]')
                  && c.querySelectorAll('[data-bar]').length === 5; })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the card's five bars" },
      );
      await sink.awaitRecord(poolRecord("remove"), 30_000, before, "the session's `pool remove`");
    });

    await step("card-edit", "Edit on the watched card connects first and opens the editor; the control written before Done reads held", async () => {
      const installs = poolCount("install");
      const before = sink.records.length;
      // Edit, the project bar's action on a watched board.
      await driver.pressOffer("edit", { board: boardMac, timeoutMs: STEP_DEADLINE_MS });
      const at = await waitEditor("the editor on the board's project");
      await sink.awaitRecord(poolRecord("install"), 30_000, before, "the new session's `pool install`");
      if (poolCount("install") !== installs + 1) throw new Error("Edit did not open exactly one session");
      // THE BOARD'S WORD: a fresh session has no echo of the write, so a
      // control that reads held here is the board's panel state saying it
      // kept the write.
      await driver.waitFor(
        `[...document.querySelectorAll(${JSON.stringify(writtenSelector())})]
          .some((el) => el.getAttribute('data-panel-state') === 'engaged')`,
        { timeoutMs: STEP_DEADLINE_MS, what: `${written.channel} to read held in the fresh session (the board's own read)` },
      );
      console.log(`  the editor at ${at}; ${written.scope} ${written.channel} reads held in the new session`);
    });

    await step("card-back", "Back from the editor: the card holds the same session, its panel again", async () => {
      const installs = poolCount("install");
      await driver.evaluate("history.back()");
      await cardPanel("the card's panel, back at `/`");
      if (poolCount("install") !== installs) throw new Error("going home opened a new session");
      const state = await driver.evaluate(
        `${card}.querySelector('[data-board-panel] ' + ${JSON.stringify(writtenSelector())})?.getAttribute('data-panel-state') ?? null`,
      );
      if (state !== "engaged") throw new Error(`${written.channel} reads ${state} on the card, not held`);
      console.log(`  the session kept (no pool install); ${written.channel} still held on the card`);
    });

    await step("card-edit-again", "Edit on the All controls row: the editor with nothing reopened; let go; Done on the docked card", async () => {
      const installs = poolCount("install");
      await driver.evaluate(`(() => {
        window.__lpWalkOpeningSeen = Boolean(document.querySelector('[data-opening-frame]'));
        window.__lpWalkOpeningObserver?.disconnect();
        window.__lpWalkOpeningObserver = new MutationObserver(() => {
          if (document.querySelector('[data-opening-frame]')) window.__lpWalkOpeningSeen = true;
        });
        window.__lpWalkOpeningObserver.observe(document.body, { subtree: true, childList: true });
      })()`);
      const pressed = await driver.evaluate(`(() => {
        const mark = ${card}.querySelector('[data-all-controls] [data-offer-path$="/edit"]');
        const buttons = mark ? mark.querySelectorAll('button') : [];
        const button = buttons[buttons.length - 1];
        if (!button || button.disabled) return null;
        button.click();
        return (button.textContent || '').trim();
      })()`);
      if (!pressed) throw new Error("the All controls row draws no Edit to press");
      const at = await waitEditor("the editor, on the session already open");
      const opened = await driver.evaluate("window.__lpWalkOpeningSeen === true");
      if (opened) throw new Error("an opening frame was drawn: Edit reopened the session");
      if (poolCount("install") !== installs) throw new Error("Edit on the connected board opened a new session");
      // Let go of the written control (its let-go glyph), so the cable steps
      // start from a board holding nothing; the board's next read says so.
      await driver.evaluate(`(() => {
        const reset = [...document.querySelectorAll(${JSON.stringify(writtenSelector())})]
          .map((el) => el.querySelector('button[aria-label^="Reset "]')).find(Boolean);
        reset.click();
      })()`);
      await driver.waitFor(
        `[...document.querySelectorAll(${JSON.stringify(writtenSelector())})]
          .every((el) => el.getAttribute('data-panel-state') !== 'engaged')`,
        { timeoutMs: STEP_DEADLINE_MS, what: `${written.channel} to be let go` },
      );
      // Done on the editor's docked card ends the session; the page goes home.
      await driver.pressOffer("done", { board: boardMac, timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(
        `(() => { const c = ${card}; return location.pathname === '/' && Boolean(c)
                  && !c.querySelector('[data-board-panel]') && c.querySelectorAll('[data-bar]').length === 5; })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: "home, the card's five bars" },
      );
      console.log(`  ${pressed} on the row → ${at}: no opening frame, no pool install; let go; Done → / and the card's facts`);
    });
    steps.sessions = sessions;

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
      sections.afterDetach = await reportSection(driver, boardMac, "after the cable came out");
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
      // M3's G1 packet flagged "replug → Attached, not listening" as a
      // product question. It was a shim bug instead: the unplug left the dead
      // port's byte channel open, so the replugged port's open() was refused
      // (docs/defects/2026-09-24-emulated-replug-leaves-the-old-byte-channel-open.md).
      // Since that fix the card's connection bar should read the port open
      // ("USB · connected", "USB · live") once its work is over; "USB · not
      // connected" here is a regression worth chasing, not the expected
      // answer.
      const settled = await driver
        .waitWork("connection", ["none", "failed"], { timeoutMs: 60_000 })
        .then(() => driver.barText("connection"))
        .catch(() => "(the card never settled)");
      console.log(`  after the replug the card reads: ${JSON.stringify(settled)}`);
      steps.replugSettled = settled;
      sections.afterReattach = await reportSection(driver, boardMac, "after the cable went back in");
    });
  } catch (error) {
    fatal = error;
  }

  const registry = await liveRegistry({ doorAddr: door?.addr ?? null, driver }).catch(() => null);
  const consoleErrors = driver.consoleLines().filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  // A Rust panic in the page is a failure of the walk even when every step
  // passed: the panicking code is by definition somewhere no step looks. The
  // first one found was a popover measuring itself from a `setTimeout` that
  // outlived the component
  // (docs/defects/2026-09-13-a-stabilization-timer-outlived-the-popover.md),
  // twelve times over, under a green 6/6. Matched narrowly on the panic
  // preamble so ordinary console noise — a 404, a warning — still only
  // reports.
  const panics = consoleErrors.filter((l) => l.includes("panicked at"));
  // The whole trace's sessions, after every record has landed (the sink is
  // fed in batches, so a step's own count can see a record late): the card
  // steps open exactly two (Connect, and Edit on the watched card) and
  // close both (Done, Done on the docked card). More is a reopen.
  const pool = {
    install: sink.records.filter((r) => r.kind === "pool" && r.action === "install").length,
    remove: sink.records.filter((r) => r.kind === "pool" && r.action === "remove").length,
  };
  const cardStepsRan = steps.some((s) => s.name === "card-edit-again" && s.ok);
  const poolWrong = cardStepsRan && (pool.install !== 2 || pool.remove !== 2);

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
        aliasHealedTo: healed,
        project: WALK_PROJECT,
        model: BOARD_MODEL,
        boardMac,
        sections,
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
    console.log(`  ${s.ok ? "✓" : "✗"} ${s.name.padEnd(17)} ${s.recordCount ?? s.records.length} record(s)   ${path.basename(s.shot)}`);
  }
  if (healed) console.log(`\n  the alias:   /devices → ${healed.split("?")[0]}, its query kept (wire, emu, record)`);
  if (sections.afterDetach) {
    console.log(
      `  the section: after detach ${sections.afterDetach} · after re-attach ${sections.afterReattach ?? "(not reached)"}   (reported, not asserted)`,
    );
  }
  if (cardStepsRan) {
    console.log(
      `  the sessions: pool install ×${pool.install}, remove ×${pool.remove} over the whole trace ` +
        `(Connect and Edit open one each; Done twice) ${poolWrong ? "✗" : "✓"}`,
    );
  }
  if (registry) {
    console.log(`\n  the ${door ? "door" : "tab"}'s live registry: ${registry.map((b) => `${b.id} flash=${b.flash} boot=${b.boot} reboots=${b.reboots} state=${b.state}`).join(" · ")}`);
  }
  if (consoleErrors.length) {
    console.log("\n  page console errors:");
    for (const line of consoleErrors.slice(-8)) console.log(`    ${line}`);
  }
  if (panics.length) {
    console.log(`\n  ✗ ${panics.length} panic(s) in the page:`);
    for (const line of panics.slice(0, 4)) console.log(`    ${line.split("\n")[0]}`);
  }
  console.log(`\n  trace  → ${path.join(options.out, "walk.jsonl")}  (${sink.records.length} records)`);
  console.log(`  steps  → ${path.join(options.out, "walk-steps.json")}`);
  if (door) {
    console.log(`  board console → ${path.join(options.out, "console", `${options.board}.console.log`)}`);
  }

  if (!options.keepOpen) {
    await driver.close();
    if (door) await stopDoor(door);
    sink.server.close();
    bundle?.close();
  } else if (door) {
    console.log(`\n  --keep-open: the door (pid ${door.pid}) is still up; the browser is still attached.`);
  } else {
    console.log(`\n  --keep-open: the browser is still attached, and the board is still in it.`);
  }

  if (fatal) {
    console.error(`\nThe walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  if (panics.length) {
    console.error(
      `\nThe walk's steps passed, but the page panicked ${panics.length} time(s). ` +
        `The full text is in walk-steps.json (consoleErrors).`,
    );
    process.exit(1);
  }
  if (poolWrong) {
    console.error(
      `\nThe walk's steps passed, but the trace holds ${pool.install} session open(s) and ${pool.remove} close(s), not two of each: something reopened.`,
    );
    process.exit(1);
  }
  console.log(
    `\n✓ the walk finished: flash → connect → identify → upload → card-connect → card-panel → card-all-controls → card-done → card-edit → card-back → card-edit-again → detach → re-attach, ` +
      `with no board${door ? "" : " and no server"}.`,
  );
}

await main();
