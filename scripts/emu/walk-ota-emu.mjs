#!/usr/bin/env node
// THE OVER-THE-AIR UPDATE WALK WITH NO BOARD (`just walk-ota-emu`).
//
// Studio's own updates, end to end, on emulated ESP32-C6 boards over the
// `?emu=` door (Studio's real Web Serial stack over the shim's virtual USB,
// lp-link channel 3 underneath), in headless Chrome. The boards boot ROM-up
// from Part B's X image (`scripts/ota/build-image.sh … a0a0a0a0`); this
// Studio's own build Y is the served split package (`lp-cli firmware package
// esp32c6-4mb`), its update files staged into the bundle the way
// `studio-web-build` does (`scripts/studio-copy-firmware.sh`).
//
//   update       E1/D2 · X → Y with one press: backing up, updating over USB,
//                finishing; ends up to date with its project still running
//                (E14); the engine cache then holds X's engine
//   cut-core     E5 · the cable comes out mid-core, goes back in: the update
//                finishes with no click
//   cut-engine   E5 · the same, mid-engine
//   engine-less  E1 · a board whose engine header is erased restores itself on
//                connect with no click, from the cache `update` filled
//   cant-get     E13 · the same board with the cache cleared and no store that
//                has X: "Needs …, which Studio can't get" → Install Y
//   crashing     E10 · NOT walked: no image makes an engine keep crashing yet
//                (Part B's U7 skip, for the same reason) — said so in the report
//   needs-usb    E9 · a pre-update board (today's single image): no update over
//                the air, today's USB flash (Lasting) on the card
//
// Every assertion waits for the BOARD's words (its console, `[OTA]`,
// `[LOADER]`, `[CORE]` lines) as well as the card's; a card line alone proves
// nothing about the board. Each step gets its own door (a fresh board) and a
// page load; the page's origin is the walk's own server, so the browser's
// engine cache (OPFS) carries from one step to the next.
//
// `--tab` runs update / cut-core / engine-less against `?emu=tab` (the board a
// Worker in the page, no door): the tab board's chip is written with the same
// seeded X chip (or its engine-less copy) the door lane boots, and power
// cycled, before Studio is asked to find it; its console is read off the
// page's emulator as it goes.
//
// NOT a CI job (minutes of emulated boards). It serves the release bundle
// itself (no dev server): `just studio-web-story-build`, the images and a
// debug lp-cli first — `just walk-ota-emu` builds what is missing.
//
//   node scripts/emu/walk-ota-emu.mjs [--fresh] [--tab] [--steps update,cut-core,...]
//
// The browser profile (target/walk-ota-emu/chrome-profile) outlives a run, so
// `engine-less` finds the engine `update` backed up even when the steps run
// as separate invocations; `--fresh` starts from a browser that has never
// seen a board.

import { appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import path from "node:path";
import process from "node:process";
import { execFileSync } from "node:child_process";

import { StudioDriver } from "./studio-driver.mjs";
import { serveStudioBundle, startDoor, stopDoor, walkPort } from "./emulated-lane.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const ARGS = process.argv.slice(2);
const TAB = ARGS.includes("--tab");
const STEPS_ARG = ARGS.includes("--steps") ? ARGS[ARGS.indexOf("--steps") + 1].split(",") : null;
const ALL_STEPS = ["update", "cut-core", "cut-engine", "engine-less", "cant-get", "crashing", "needs-usb"];
const TAB_STEPS = ["update", "cut-core", "engine-less"];
const STEPS = STEPS_ARG ?? (TAB ? TAB_STEPS : ALL_STEPS);

const IMAGES = path.join(ROOT, "target/walk-ota-emu/images");
const X = path.join(IMAGES, "x");
const MONO = path.join(IMAGES, "mono");
const PARTS = path.join(ROOT, "target/firmware-parts");
const PACKAGES = path.join(ROOT, "target/studio-web-assets/firmware");
const LP_CLI = path.join(ROOT, "target/debug/lp-cli");
const PROJECT = process.env.WALK_PROJECT ?? "Peach (1D)";

/// Waits. Every one is the page's or the board's, never a sleep: these are
/// only how long the walk lets one stage take before it calls it failed.
const STEP_MS = 180_000;
const UPDATE_MS = Number(process.env.WALK_UPDATE_MS ?? 1_200_000);
const LOAD_MS = 420_000;
/// How long the cable stays out once Studio has seen it go.
const DETACHED_MS = Number(process.env.WALK_DETACHED_MS ?? 3_000);
/// The shim's host serial path (`?emu-tty=`): unset, a page on a Mac runs the
/// Mac tty model (`mac_tty_model.js`), which hands the page at most 255 B a
/// read and reads at most every 16 ms — the bound on every board→host rate
/// this walk measures there (an update's backup: ~12 KB/s). `none` for a
/// lossless pipe, to measure what the model costs.
const EMU_TTY = process.env.WALK_EMU_TTY ?? null;

const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;

// --- what the update shows -------------------------------------------------

/// The card's update lines (the update-states spike's words, P4), for the
/// record: the first time each kind showed.
const CARD_LINES = [
  ["backing up", /Backing up current firmware…[^\n]*/],
  ["updating", /Updating over USB…[^\n]*/],
  ["finishing", /Finishing the update…[^\n]*/],
  ["restoring", /Restoring firmware…[^\n]*/],
  ["up to date", /[^\n]*(up to date|same as this Studio)[^\n]*/],
  ["available", /[^\n]* available[^\n]*/],
  ["cant get", /Needs [^\n]*, which Studio can't get/],
  ["needs usb", /Needs one update over USB/],
];

async function main() {
  for (const [what, at] of [
    ["the release Studio bundle (just studio-web-story-build)", path.join(ROOT, "target/dx/lpa-studio-web/release/web/public")],
    ["X's split image (scripts/ota/build-image.sh target/walk-ota-emu/images/x a0a0a0a0)", path.join(X, "merged.bin")],
    ["the pre-update single image (lp-cli firmware package esp32c6-4mb --single-image --out …/mono/package)", path.join(MONO, "package/manifest.json")],
    ["Y, this Studio's split package (lp-cli firmware package esp32c6-4mb)", path.join(PACKAGES, "esp32c6-4mb/manifest.json")],
    ["a debug lp-cli (cargo build -p lp-cli)", LP_CLI],
  ]) {
    if (!existsSync(at)) {
      console.error(`walk-ota-emu: missing ${what}: ${at}\n  run: just walk-ota-emu (it builds what is missing)`);
      process.exit(1);
    }
  }

  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const out = path.join(ROOT, "target/walk-ota-emu", `${TAB ? "tab-" : ""}${stamp}`);
  const shots = path.join(out, "shots");
  mkdirSync(shots, { recursive: true });
  const trail = path.join(out, "card-trail.log");

  // This Studio's firmware, staged the way the release bundle is (Y's update
  // files beside its manifest.json).
  const stagedY = path.join(out, "firmware-y");
  stageFirmware(stagedY, PACKAGES, PARTS);
  const y = JSON.parse(readFileSync(path.join(stagedY, "esp32c6-4mb/ota-manifest.json"), "utf8"));
  const x = JSON.parse(readFileSync(path.join(X, "ota/ota-manifest.json"), "utf8"));
  const xSplit = JSON.parse(readFileSync(path.join(X, "split.json"), "utf8"));
  // X as a fielded board: its first boot done (`lpfs` formatted and
  // mounted — a fresh chip's first hello says "formatted", a different
  // card), a project uploaded and running, its status light recorded. Then
  // the same chip with its engine header erased: an engine-less board.
  const chips = path.join(ROOT, "target/walk-ota-emu/chips");
  mkdirSync(chips, { recursive: true });
  const xChip = path.join(chips, "x.bin");
  seedChip(path.join(X, "merged.bin"), xChip);
  const monoChip = path.join(chips, "mono.bin");
  seedChip(path.join(MONO, "package/fw-esp32c6-merged.bin"), monoChip, "4s");
  const xEngineLess = path.join(chips, "x-engine-less.bin");
  {
    const bytes = readFileSync(xChip);
    bytes.fill(0xff, xSplit.engine.offset, xSplit.engine.offset + 0x1000);
    writeFileSync(xEngineLess, bytes);
  }
  const chipFiles = { x: xChip, "x-engine-less": xEngineLess };
  const lpEmu = git(["log", "-1", "--format=%h", "--", "lp-emu"]);
  const head = git(["rev-parse", "--short=12", "HEAD"]);

  const served = stagedY;
  const route = (request, response, url) => {
    // An empty page on Studio's origin: where the walk empties the engine
    // cache with no Studio running.
    if (url.pathname === "/walk-blank") {
      response.writeHead(200, { "content-type": "text/html" });
      response.end("<!doctype html><title>walk</title>");
      return true;
    }
    // The tab lane's chips, by name.
    const chip = url.pathname.match(/^\/__walk\/chip\/([a-z-]+)$/);
    if (chip && chipFiles[chip[1]]) {
      response.writeHead(200, { "content-type": "application/octet-stream" });
      response.end(readFileSync(chipFiles[chip[1]]));
      return true;
    }
    if (!url.pathname.startsWith("/firmware/")) return false;
    const file = path.join(served, decodeURIComponent(url.pathname.slice("/firmware/".length)));
    if (!file.startsWith(served) || !existsSync(file)) {
      response.writeHead(404);
      response.end();
      return true;
    }
    response.writeHead(200, { "content-type": file.endsWith(".json") ? "application/json" : "application/octet-stream" });
    response.end(readFileSync(file));
    return true;
  };
  const bundle = await serveStudioBundle({ root: ROOT, port: walkPort(ROOT, "walk-ota-emu"), route });
  const studioPort = bundle.address().port;
  // A firmware store that holds nothing (every lookup 404s): X is a dev
  // build no store would have, and the walk never touches the internet.
  const store = await new Promise((resolve) => {
    const server = createServer((_, response) => {
      response.writeHead(404, { "access-control-allow-origin": "*" });
      response.end();
    });
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
  const storeOrigin = `http://127.0.0.1:${store.address().port}`;

  console.log("\nTHE OVER-THE-AIR UPDATE WALK WITH NO BOARD");
  console.log(`  this tree       ${head} (lp-emu ${lpEmu}, lp-emu:esp32c6:t1)`);
  console.log(`  X (the boards)  ${x.version}+${x.commit.slice(0, 12)}`);
  console.log(`  Y (this Studio) ${y.version}+${y.commit.slice(0, 12)}`);
  console.log(`  Studio          http://127.0.0.1:${studioPort}/ (the release bundle, served by this walk)`);
  console.log(`  host tty        ${EMU_TTY ?? "the page's default (a Mac's model on a Mac)"}`);
  console.log(`  steps           ${STEPS.join(", ")}${TAB ? " (?emu=tab)" : ""}\n`);

  const report = {
    tree: head,
    lpEmu,
    configuration: "lp-emu:esp32c6:t1",
    backing: TAB ? "tab" : "door",
    emuTty: EMU_TTY,
    x: `${x.version}+${x.commit.slice(0, 12)}`,
    y: `${y.version}+${y.commit.slice(0, 12)}`,
    steps: [],
  };
  // One browser profile for every run, so the engine cache (OPFS, by the
  // walk server's origin) carries from `update` to `engine-less` even when
  // the steps run as separate invocations.
  // `--fresh`: a browser that has never seen these boards (no remembered
  // cards, an empty engine cache).
  if (ARGS.includes("--fresh")) rmSync(path.join(ROOT, "target/walk-ota-emu/chrome-profile"), { recursive: true, force: true });
  const driver = await StudioDriver.launch({
    width: 1440,
    height: 1100,
    profileDir: path.join(ROOT, "target/walk-ota-emu/chrome-profile"),
  });
  let door = null;

  const pageUrl = (doorAddr) =>
    `http://localhost:${studioPort}/devices?emu=${doorAddr ? encodeURIComponent(`ws://${doorAddr}`) : "tab"}` +
    `&firmware-store=${encodeURIComponent(storeOrigin)}` +
    (EMU_TTY ? `&emu-tty=${EMU_TTY}` : "");

  /// What the board said: before any host opened its port (the door keeps
  /// that as `<id>.console-untaken.log`), then on the port. The door writes
  /// both back every two seconds.
  const boardConsole = (board) => {
    if (!door) return "";
    return [`${board}.console-untaken.log`, `${board}.console.log`]
      .map((name) => path.join(door.consoleDir, name))
      .filter((file) => existsSync(file))
      .map((file) => readFileSync(file, "utf8"))
      .join("");
  };
  /// The tab lane's board console is the page's emulator's (read into
  /// `tabConsole` as it grows); the door lane's is the door's console file.
  let tabConsole = "";
  const refreshTab = async () => {
    if (!TAB) return;
    try {
      tabConsole += await driver.evaluate(`(window.__walkConsole ?? "").slice(${tabConsole.length})`);
    } catch {
      /* mid-navigation */
    }
  };
  const boardWords = (board) => (TAB ? tabConsole : boardConsole(board));
  const waitBoard = async (board, pattern, what, timeoutMs = STEP_MS, from = 0) => {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      await refreshTab();
      const text = boardWords(board).slice(from);
      const match = text.match(pattern);
      if (match) return match[0];
      if (Date.now() > deadline) throw new Error(`the board never said ${what} (${pattern})`);
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  };
  const cardLines = async () => {
    const text = await driver.evaluate(MAIN_TEXT);
    const seen = {};
    for (const [kind, pattern] of CARD_LINES) {
      const match = text.match(pattern);
      if (match) seen[kind] = match[0].trim();
    }
    return seen;
  };
  /// Watch the card until `done` holds, noting every update line it shows on
  /// the way (the order they first appeared in).
  const watchCard = async (done, what, timeoutMs = UPDATE_MS, onTick = null) => {
    const order = [];
    const began = Date.now();
    let lastTrail = "";
    let pageSeen = 0;
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      await refreshTab();
      const lines = await cardLines();
      for (const [kind, line] of Object.entries(lines)) {
        if (!order.some((entry) => entry.kind === kind)) order.push({ kind, line, atMs: Date.now() });
      }
      if (onTick) await onTick(lines);
      // A live trail for whoever watches the walk: the card's lines as they
      // change, with the time since the watch began.
      const pageLines = driver.consoleLines();
      if (pageLines.length !== pageSeen) {
        pageSeen = pageLines.length;
        writeFileSync(path.join(out, "page-console.log"), pageLines.join("\n"));
      }
      const now = JSON.stringify(lines);
      if (now !== lastTrail) {
        lastTrail = now;
        appendFileSync(trail, `${((Date.now() - began) / 1000).toFixed(1)} s ${now}\n`);
      }
      if (await driver.evaluate(done)) return order;
      if (Date.now() > deadline) throw new Error(`the card never reached ${what}; it showed ${JSON.stringify(order)}`);
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  };
  const shot = async (name) => {
    const file = path.join(shots, `${String(report.steps.length + 1).padStart(2, "0")}-${name}.png`);
    try {
      await driver.screenshot(file);
    } catch {
      /* the page may be gone */
    }
    return path.relative(ROOT, file);
  };
  const step = async (name, describe, body) => {
    console.log(`— ${name}: ${describe}`);
    const started = Date.now();
    let error = null;
    let note = null;
    try {
      note = await body();
    } catch (failure) {
      error = failure;
    }
    const file = await shot(name);
    // The page as it stood (the card, its terminal's lines): what a person
    // reading the record would have seen. Studio's own terminal lines (the
    // update's narration: rates, reconnect times) start with `▸`.
    let terminal = [];
    try {
      const page = await driver.evaluate("document.body.innerText");
      writeFileSync(path.join(out, `${name}-page.txt`), page);
      terminal = page.split("\n").filter((line) => line.startsWith("▸ ")).map((line) => line.slice(2));
    } catch {
      /* the page may be gone */
    }
    const record = {
      name,
      describe,
      ok: !error,
      error: error?.message ?? null,
      wallSeconds: Math.round((Date.now() - started) / 1000),
      ...(note ?? {}),
      terminal,
      shot: file,
    };
    report.steps.push(record);
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"}${note?.summary ? `  ${note.summary}` : ""}`);
    for (const line of record.terminal) console.log(`    · ${line}`);
    return !error;
  };

  /// A fresh door holding `boards`, and the page loaded against it.
  const openDoor = async (id, boards) => {
    if (door) await stopDoor(door);
    door = await startDoor({
      root: ROOT,
      id,
      boards,
      stateDir: path.join(out, "state", id),
      consoleDir: path.join(out, "console", id),
      logFile: path.join(out, `serve-${id}.log`),
      fresh: true,
    });
    await loadPage(door.addr);
  };
  const loadPage = async (doorAddr) => {
    await driver.navigate(pageUrl(doorAddr));
    await driver.awaitShim();
    await driver.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: LOAD_MS, what: "Studio to finish loading" });
  };
  /// The tab lane: write `chip` (a name in `chipFiles`) into the page's
  /// board, power-cycle it, and tap its console — before Studio is asked to
  /// find it, as a board already plugged in would be.
  const seedTab = async (chip) => {
    tabConsole = "";
    await driver.waitFor(`Boolean(window.__lpEmuSerial?.bus?.requireLivePort)`, { what: "the tab backing" });
    const bytes = await driver.evaluate(
      `(async () => {
         const emu = window.__lpEmuSerial.bus.requireLivePort("tab-c6").emulator;
         const decoder = new TextDecoder("utf-8");
         window.__walkConsole = "";
         emu._hub.onBytes((bytes) => { window.__walkConsole += decoder.decode(bytes, { stream: true }); });
         emu._hub.onConsole((text) => { window.__walkConsole += text; });
         const chip = new Uint8Array(await (await fetch("/__walk/chip/${chip}", { cache: "no-store" })).arrayBuffer());
         await emu.putFlash(chip);
         await emu.command("power-cycle");
         return chip.length;
       })()`,
      { awaitPromise: true, timeoutMs: 120_000 },
    );
    // No wait for its boot words here: a board with no host draining its
    // port keeps them (a core-only board says nothing else), as on the door.
    return bytes;
  };
  const connect = async (board) => {
    await driver.clickWhenReady("via USB", { timeoutMs: STEP_MS });
    await driver.pickBoard(board, { timeoutMs: STEP_MS });
  };
  const pushProject = async () => {
    const face = await driver.waitFor(
      `(() => { const t = ${MAIN_TEXT};
                return t.includes('Remove project') ? 'running' : t.includes('to choose from') ? 'empty' : false; })()`,
      { timeoutMs: STEP_MS, what: "the board to say what it runs" },
    );
    if (face === "running") return "already running a project";
    await driver.clickWhenReady("to choose from", { timeoutMs: STEP_MS });
    await driver.waitFor(`Boolean(document.querySelector('[id^="ux-popover-panel"]'))`, { what: "the project popover" });
    await driver.click(PROJECT, { scope: `document.querySelector('[id^="ux-popover-panel"]')`, exact: true });
    await driver.clickWhenReady("Put it on the board", { timeoutMs: STEP_MS });
    await driver.waitFor(`${MAIN_TEXT}.includes('Project loaded')`, { timeoutMs: STEP_MS, what: "`Project loaded`" });
    return `${PROJECT} loaded`;
  };
  /// The card's over-the-air install: `update-firmware`'s button ("Update",
  /// or "Install <Y>" between two dev builds). Never today's "Update
  /// firmware" flash, which is a different offer.
  const pressUpdate = async () => {
    const label = await driver.waitFor(
      `(() => {
         const button = [...document.querySelectorAll('#main button')].find((el) => {
           const t = (el.textContent || '').replace(/\\s+/g, ' ').trim();
           return !el.disabled && (t === 'Update' || t.startsWith('Install '));
         });
         return button ? (button.textContent || '').replace(/\\s+/g, ' ').trim() : false;
       })()`,
      { timeoutMs: STEP_MS, what: "the card's over-the-air Update" },
    );
    await driver.click(label, { scope: `document.querySelector('#main')`, exact: true });
    return label;
  };
  const upToDate = `/(up to date|same as this Studio)/.test(${MAIN_TEXT})`;
  const engineCache = () =>
    driver.evaluate(
      `(async () => {
         try {
           const root = await navigator.storage.getDirectory();
           const dir = await root.getDirectoryHandle('firmware-cache');
           const index = await (await (await dir.getFileHandle('index.json')).getFile()).text();
           const names = [];
           for await (const [name] of (await dir.getDirectoryHandle('engines')).entries()) names.push(name);
           return JSON.stringify([index, ...names]);
         } catch (e) { return JSON.stringify([]); }
       })()`,
      { awaitPromise: true },
    ).then((json) => JSON.parse(json));
  /// Empty the engine cache with no Studio running on the origin (a page
  /// that holds the cache open could still believe its old index).
  const clearEngineCache = async () => {
    await driver.navigate(`http://localhost:${studioPort}/walk-blank`);
    return driver.evaluate(
      `(async () => {
         const root = await navigator.storage.getDirectory();
         try { await root.removeEntry('firmware-cache', { recursive: true }); } catch {}
         return true;
       })()`,
      { awaitPromise: true },
    );
  };

  /// One update, from the press to the end, cutting the cable once when
  /// `cut` says so.
  const runUpdate = async (board, { cut = null } = {}) => {
    const from = boardWords(board).length;
    const label = await pressUpdate();
    let cutAt = null;
    let cutShot = null;
    const order = await watchCard(upToDate, "up to date on Y", UPDATE_MS, async (lines) => {
      if (!cut || cutAt) return;
      const words = boardWords(board).slice(from);
      const line = lines[cut.stage];
      const percent = Number(line?.match(/(\d+)%/)?.[1] ?? -1);
      if (cut.boardSays.test(words) && percent >= cut.atPercent) {
        cutAt = line;
        await driver.detach(board);
        const back = Date.now();
        cutShot = await shot("cable-out");
        writeFileSync(path.join(out, "cable-out-page.txt"), await driver.evaluate("document.body.innerText"));
        // The cable stays out until Studio has seen it go — its own terminal
        // line — then goes back in, as a person re-seating it would.
        // (The card keeps the update's line and offers "Reconnect…" while
        // its link is gone; a core-only board's card may instead leave the
        // roster for "remembered, not connected".)
        await driver.waitFor(`/Reconnect…|remembered boards? not connected/.test(document.body.innerText)`, {
          timeoutMs: STEP_MS,
          what: "Studio to see the cable go",
        });
        // The one deliberate duration in this walk: a person re-seating a
        // cable takes seconds, not the instant the walk would otherwise take.
        await new Promise((resolve) => setTimeout(resolve, DETACHED_MS));
        await driver.attach(board);
        cutAt = `${line} (out for ${Date.now() - back} ms)`;
      }
    });
    return { label, order, cutAt, cutShot, from };
  };

  // The door writes a board's console file every 2 s (`emu serve`'s
  // FLUSH_EVERY), so the card can say "up to date" before the file holds the
  // engine's commit line: give the file one flush to catch up before reading
  // the board's words.
  const settle = async (board, from) => {
    try {
      await waitBoard(board, /\[OTA\] engine verified, committing/, "the engine's commit", 5_000, from);
    } catch {
      // Not said: the assertions below name what is missing.
    }
  };
  const boardSaid = (board, from, patterns) =>
    Object.fromEntries(
      patterns.map(([name, pattern]) => [name, boardWords(board).slice(from).match(pattern)?.[0] ?? null]),
    );
  // The board's own words. The console carries the link's raw frames too,
  // so a capture stops at the first byte that is not text.
  const OTA_WORDS = [
    ["core offer", /\[OTA\] offer \S+ → core @0x[0-9a-f]+/],
    ["core committed", /\[OTA\] core verified, committing/],
    ["core on trial", /\[LOADER\] core @0x[0-9a-f]+ \(trial\)/],
    ["core confirmed", /\[OTA\] core confirmed/],
    ["engine offer", /\[OTA\] offer \S+ → engine @0x[0-9a-f]+/],
    ["engine committed", /\[OTA\] engine verified, committing/],
    ["resumed", /\[OTA\] resuming (core|engine) at \d+/],
    ["light", /\[OTA\] light: GPIO\d+ × \d+ LEDs r=\d+ g=\d+ b=\d+ \([a-z-]+\)/g],
  ];

  let fatal = null;
  try {
    for (const name of STEPS) {
      const board = TAB ? "tab-c6" : `c6-${name}`;
      // Every step's board is its own board: its own MAC, so no step's
      // card is a board Studio remembers from another.
      const mac = `mac=02:4c:50:00:0${ALL_STEPS.indexOf(name) + 1}:0${TAB ? 1 : 0}`;
      const xBoard = `${board}=${xChip},kind=rom-up,${mac}`;
      switch (name) {
        case "update":
          // An empty cache, so the update must back the board up first.
          await clearEngineCache();
          if (!TAB) await openDoor(name, [xBoard]);
          else {
            await loadPage(null);
            await seedTab("x");
          }
          await step("update", "X → Y with one press: back up, update over USB, finish; the project still runs", async () => {
            await connect(board);
            const pushed = await pushProject();
            const ran = await runUpdate(board);
            await settle(board, ran.from);
            const said = boardSaid(board, ran.from, OTA_WORDS);
            for (const need of ["core offer", "core on trial", "core confirmed", "engine offer", "engine committed"]) {
              if (!said[need]) throw new Error(`the board never said ${need}`);
            }
            if (!ran.order.some((entry) => entry.kind === "backing up")) throw new Error("the card never said Backing up current firmware");
            if (!ran.order.some((entry) => entry.kind === "updating")) throw new Error("the card never said Updating over USB");
            // E14: the project survived — the card still runs it.
            await driver.waitFor(`${MAIN_TEXT}.includes('Remove project')`, { timeoutMs: STEP_MS, what: "the board running its project on Y" });
            const cache = await engineCache();
            const backup = cache.some((entry) => entry.includes(x.engine.sha256));
            if (!backup) throw new Error(`the engine cache does not hold X's engine (${x.engine.sha256.slice(0, 12)}…): ${JSON.stringify(cache)}`);
            return { summary: `${ran.label}; ${ran.order.map((e) => e.kind).join(" → ")}; project kept; X's engine cached`, pushed, card: ran.order, board: said, cache };
          });
          break;
        case "cut-core":
        case "cut-engine": {
          const engine = name === "cut-engine";
          if (!TAB) await openDoor(name, [xBoard]);
          else {
            await loadPage(null);
            await seedTab("x");
          }
          await step(name, `the cable comes out mid-${engine ? "engine" : "core"} and goes back in: the update finishes with no click`, async () => {
            await connect(board);
            await driver.waitFor(`${MAIN_TEXT}.includes('Ready')`, { timeoutMs: STEP_MS, what: "Ready on X" });
            const ran = await runUpdate(board, {
              cut: engine
                ? { stage: "finishing", boardSays: /\[OTA\] offer \S+ → engine/, atPercent: 40 }
                : { stage: "updating", boardSays: /\[OTA\] offer \S+ → core/, atPercent: 40 },
            });
            if (!ran.cutAt) throw new Error("the walk never found the moment to cut");
            await settle(board, ran.from);
            const said = boardSaid(board, ran.from, OTA_WORDS);
            if (!said.resumed) throw new Error("the board never said it resumed the transfer");
            if (!ran.order.some((entry) => entry.kind === "finishing")) throw new Error("the card never said Finishing the update");
            return { summary: `cut at ${ran.cutAt}; ${said.resumed}; up to date`, card: ran.order, board: said };
          });
          break;
        }
        case "engine-less":
        case "cant-get": {
          const clear = name === "cant-get";
          if (TAB && clear) {
            report.steps.push({ name, ok: true, skipped: "walked on the door lane only" });
            continue;
          }
          // X as fielded (its project ran, so its status light is
          // recorded), its engine header erased.
          if (clear) await clearEngineCache();
          if (!TAB) await openDoor(name, [`${board}=${xEngineLess},kind=rom-up,${mac}`]);
          else {
            await loadPage(null);
            await seedTab("x-engine-less");
          }
          if (!clear) {
            await step(name, "the engine-less board restores itself on connect, with no click, from the cache", async () => {
              const from = 0;
              await connect(board);
              await waitBoard(board, /\[OTA\] offer \S+ → engine/, "the restore's engine offer");
              await shot("restoring");
              writeFileSync(path.join(out, "restoring-page.txt"), await driver.evaluate("document.body.innerText"));
              const order = await watchCard(`${MAIN_TEXT}.includes('Remove project') || /a0a0a0a0[^\\n]*available/.test(${MAIN_TEXT})`, "X running again");
              if (!order.some((entry) => entry.kind === "restoring")) throw new Error("the card never said Restoring firmware");
              await settle(board, from);
              const said = boardSaid(board, from, OTA_WORDS);
              const lights = boardWords(board).slice(from).match(OTA_WORDS[7][1]) ?? [];
              if (!said["engine committed"]) throw new Error("the board never committed the engine");
              return { summary: `${order.map((e) => e.kind).join(" → ")}; lights: ${lights.map((l) => l.replace(/^.*\(/, "(")).join(" ")}`, card: order, board: said, lights };
            });
          } else {
            await step(name, "the same board, the cache cleared and no store that has X: Studio says it can't get it; Install Y", async () => {
              const from = boardWords(board).length;
              await connect(board);
              // A core-only board says no hello, so it stays a pending link
              // until it is kept: its no-click restore runs (and misses)
              // there, and the row that needs a person is on the kept card.
              await driver.waitFor(`/core only — waiting for its engine/.test(${MAIN_TEXT})`, {
                timeoutMs: STEP_MS,
                what: "the pending card to settle on core-only",
              });
              await shot("cant-get-pending");
              const kept = await driver.clickWhenReady("Set up this device", { timeoutMs: STEP_MS });
              const order = await watchCard(`/which Studio can't get/.test(${MAIN_TEXT})`, "Needs …, which Studio can't get", STEP_MS * 2);
              await shot("cant-get-row");
              const label = await pressUpdate();
              const rest = await watchCard(upToDate, "up to date on Y");
              await settle(board, from);
              const said = boardSaid(board, from, OTA_WORDS);
              if (!said["engine committed"]) throw new Error("the board never committed Y's engine");
              return { summary: `pending core-only; pressed ${kept}; ${order.map((e) => e.kind).join(" → ")}; pressed ${label}; ${rest.map((e) => e.kind).join(" → ")}`, card: [...order, ...rest], board: said };
            });
          }
          break;
        }
        case "crashing":
          report.steps.push({
            name,
            ok: true,
            skipped:
              "E10 not walked: no image makes the engine keep crashing (Part B's U7 is skipped for the same reason; a fixture or a seeded RTC ledger is its follow-up). The row's words and offers are core tests (device_update_standing, device_update_offers) and a story.",
          });
          console.log(`— ${name}: skipped (no engine-crashing image yet; see the report)`);
          break;
        case "needs-usb":
          if (TAB) continue;
          await openDoor(name, [`${board}=${monoChip},kind=rom-up,${mac}`]);
          await step("needs-usb", "a pre-update board (today's single image): no update over the air; the USB flash as today", async () => {
            await connect(board);
            await driver.waitFor(`${MAIN_TEXT}.includes('Ready')`, { timeoutMs: STEP_MS, what: "Ready" });
            const controls = (await driver.controls()).map((control) => control.text);
            const overTheAir = controls.filter((text) => text === "Update" || text.startsWith("Install "));
            if (overTheAir.length) throw new Error(`a pre-update board was offered ${overTheAir.join(", ")}`);
            const words = boardWords(board);
            if (/\[OTA\]/.test(words)) throw new Error("a single image spoke the update protocol");
            const flash = controls.find((text) => text.includes("Update firmware") || text.includes("Flash firmware")) ?? null;
            return { summary: `no over-the-air offer; ${flash ? `today's "${flash}"` : "no flash offered (this Studio's version)"}`, controls };
          });
          break;
        default:
          throw new Error(`unknown step ${name}; the steps are ${ALL_STEPS.join(", ")}`);
      }
    }
  } catch (error) {
    fatal = error;
  }

  const consoleErrors = driver.consoleLines().filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  report.consoleErrors = consoleErrors.slice(-40);
  report.panics = consoleErrors.filter((l) => l.includes("panicked at")).length;
  writeFileSync(path.join(out, "walk-ota-emu.json"), JSON.stringify(report, null, 2));
  writeFileSync(path.join(out, "page-console.log"), driver.consoleLines().join("\n"));

  console.log("\n=== the over-the-air update walk");
  for (const s of report.steps) {
    console.log(`  ${s.skipped ? "–" : s.ok ? "✓" : "✗"} ${s.name.padEnd(22)} ${s.skipped ?? s.summary ?? s.error ?? ""}`);
  }
  console.log(`\n  lp-emu ${lpEmu} (lp-emu:esp32c6:t1); report → ${path.relative(ROOT, path.join(out, "walk-ota-emu.json"))}`);

  await driver.close();
  if (door) await stopDoor(door);
  bundle.close();
  store.close();

  const failed = report.steps.filter((s) => !s.ok);
  if (fatal || failed.length || report.panics) {
    console.error(`\nThe walk did not pass: ${fatal?.message ?? failed.map((s) => s.name).join(", ") ?? ""}${report.panics ? ` (the page panicked ${report.panics} time(s))` : ""}`);
    process.exit(1);
  }
  console.log("\n✓ the over-the-air update walk passed");
}

/// Stage one firmware directory the way a bundle carries it
/// (`scripts/studio-copy-firmware.sh`): every package under `packages`.
function stageFirmware(dest, packages, parts) {
  rmSync(dest, { recursive: true, force: true });
  for (const build of readdirSync(packages)) {
    execFileSync("scripts/studio-copy-firmware.sh", [build, packages, dest, parts], { cwd: ROOT, stdio: "inherit" });
  }
}

function git(args) {
  return execFileSync("git", args, { cwd: ROOT, encoding: "utf8" }).trim();
}

/// A fielded board's chip: `image` booted ROM-up once under lp-cli's own
/// host (`lp-cli emu run --rom-up-flash … --host-link`), its first boot's
/// `lpfs` format done, the walk's project uploaded and running and its
/// status light recorded — Part B's emulator scenarios seed their chips the
/// same way (`lp-cli/tests/emu_ota.rs`, `with_project`).
/// `emulated` bounds the run (a single image never records a status light,
/// so its seeding ends there).
function seedChip(image, chip, emulated = "60s") {
  // Reused while it is newer than its image: seeding is a minute of
  // emulated boot, and the chip never changes for the same image.
  if (existsSync(chip) && statSync(chip).mtimeMs > statSync(image).mtimeMs) return;
  const bytes = readFileSync(image);
  const whole = Buffer.alloc(4 * 1024 * 1024, 0xff);
  bytes.copy(whole, 0);
  writeFileSync(chip, whole);
  execFileSync(
    LP_CLI,
    ["emu", "run", "--rom-up-flash", chip, "--host-link", "--timeout", emulated, "--exit-on", "status light: GPIO",
      "--upload", "projects/test/shader-oracle",
      // A stamped identity, as Studio's flash leaves every board it
      // provisions: a board that mounts with none reads as one that lost
      // its files (`device_layout_view`), a different card.
      "--request", JSON.stringify({ filesystem: { write: { path: "/.lp/device.json", data: JSON.stringify({ uid: "dev00000000000000a1", name: "Walk board" }) } } })],
    { cwd: ROOT, stdio: ["ignore", "ignore", "inherit"] },
  );
}

function hex(n) {
  return `0x${n.toString(16)}`;
}

await main();
