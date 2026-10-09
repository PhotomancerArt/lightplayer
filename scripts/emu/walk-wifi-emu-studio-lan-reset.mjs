#!/usr/bin/env node
// THE EMULATED WALK OF RESET ON A BOARD REACHED OVER WI‑FI
// (`just walk-wifi-emu studio-lan-reset`).
//
// One emulated ESP32-C6, `c6-a`, running the packaged firmware ROM-up on the
// `lan` lane's virtual LAN (`lan=home`). Real Studio, headless, on its
// release bundle, with no `?emu=` and no `?lan=`: the board is reached the
// way a person reaches it, by typing its address into the Network row of
// Connect a board.
//
//   R1  the board joins the fixture's network over its USB door; the walk
//       then holds that door as the board's console
//   R2  Studio connects to it by address: the board says a secure LAN
//       session opened, and the card says "Ready" — with Reset ENABLED
//       (it used to be drawn disabled, "Reset needs USB")
//   R3  Reset is pressed, once, and nothing else is: the board's console
//       says it acked the restart and the ROM boots it again (a software
//       reset), it rejoins the network,
//       and a NEW secure LAN session opens from the page's own redial; the
//       card is "Ready" again with no click
//
// ⚠️ THE `studio-lan` LANE'S STAND-IN (ruling DD193): an emulated board's
// LAN lease is not reachable from the host, so the walk types the board's
// loopback forward (`127.0.0.1:<port>`) where a person would type its IP.
// The forward survives a reset (the `lan` lane's W9), so the redial dials
// the same address a person's page would.
//
// THE BOARD'S WORDS DECIDE EVERY STEP: its console (its USB link held by
// `lp-cli link capture`). Studio's words only say when to look — except the
// card's Reset being enabled, which is the page's claim.
//
// NOT CI. Made-up test values only. Headless Chrome. Needs what
// `studio-lan` needs (`just walk-wifi-emu studio-lan-reset --dry-run` lists
// them). The times it prints are WALL time on this host around an emulated
// board: the emulator's clock is a model, so they say where the time went,
// never what silicon would take.

import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { findChrome, StudioDriver } from "./studio-driver.mjs";
import {
  PACKAGED_C6_MERGED,
  RELEASE_BUNDLE,
  SERVED_FIRMWARE,
  boardRegistry,
  openNetworkRow,
  serveStudioBundle,
  startDoor,
  startRecordSink,
  stopDoor,
  walkPort,
} from "./emulated-lane.mjs";
import {
  FIXTURE,
  LAN,
  MAIN_TEXT,
  NET,
  Page,
  STEP_MS,
  awaitStatus,
  cardOf,
  forwardOf,
  holdConsole,
  lpCli,
  lpEmuCommit,
  releaseConsole,
  stationOf,
} from "./walk-wifi-emu-lan.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const LP_CLI = path.join(ROOT, "target/debug/lp-cli");
const A = "c6-a";

/// The Network row's address field and its Connect (`wifi_address_entry.rs`),
/// as the `studio-lan` lane finds them; the row opens when the Network square
/// is pressed (`openNetworkRow`).
const ADDRESS_FIELD = `document.querySelector('#main input[placeholder^="192.168.1.40"]')`;
const ADDRESS_ENTRY = `${ADDRESS_FIELD}?.closest('label')?.parentElement?.parentElement`;
const PRESS_ADDRESS_CONNECT = `(() => {
  const entry = ${ADDRESS_ENTRY};
  const button = [...(entry?.querySelectorAll('button') ?? [])].find((b) => !b.disabled && (b.innerText || '').trim() === 'Connect');
  if (!button) return false;
  button.click();
  return true;
})()`;

/// The card's Reset button: its whole text is "Reset" (Factory reset is
/// another button).
const resetButton = (forward) =>
  `[...(${cardOf(forward)}?.querySelectorAll('button') ?? [])].find((b) => (b.innerText || '').trim() === 'Reset')`;

/// The board's words: the server acking the request and resetting
/// (`lpa-server`'s `tick_and_send`; the firmware's own `[REBOOT]` line is
/// queued behind the reset and never reaches the console), the ROM's
/// banner for a software reset, then the station task and the LAN link
/// server on the new boot.
const BOARD = {
  asked: "tick_and_send: reboot acked, resetting",
  booted: /^rst:0x3 \(LP_SW_HPSYS\)/,
  address: /\[wifi\] address \d+\.\d+\.\d+\.\d+/,
  session: /\[lan\] link \S+ .*secure session opening/,
};

function usage() {
  return "usage: node scripts/emu/walk-wifi-emu.mjs studio-lan-reset [--out <dir>] [--keep-open] [--dry-run]";
}

function parseArgs(argv) {
  const rest = argv[0] === "studio-lan-reset" ? argv.slice(1) : argv;
  const options = { out: path.join(ROOT, "target/walk-wifi-emu/studio-lan-reset"), keepOpen: false, dryRun: false };
  for (let i = 0; i < rest.length; i += 1) {
    const arg = rest[i];
    if (arg === "--out") options.out = path.resolve(rest[++i] ?? "");
    else if (arg === "--keep-open") options.keepOpen = true;
    else if (arg === "--dry-run") options.dryRun = true;
    else if (arg === "--help" || arg === "-h") options.help = true;
    else throw new Error(`unknown argument ${JSON.stringify(arg)}\n${usage()}`);
  }
  return options;
}

function prerequisites() {
  return [
    ["a debug lp-cli (cargo build -p lp-cli)", LP_CLI],
    ["the release Studio bundle (just studio-web-story-build)", path.join(ROOT, RELEASE_BUNDLE)],
    ["the packaged firmware (just studio-firmware-package-served)", path.join(ROOT, SERVED_FIRMWARE)],
    ["the packaged merged C6 image (just studio-firmware-package-served)", path.join(ROOT, PACKAGED_C6_MERGED)],
  ].map(([what, at]) => ({ what, at, present: existsSync(at) }));
}

async function main() {
  let options;
  try {
    options = parseArgs(process.argv.slice(2));
  } catch (error) {
    console.error(error.message);
    process.exit(2);
  }
  if (options.help) {
    console.log(usage());
    return;
  }
  const out = options.out;
  const shots = path.join(out, "shots");
  const consoleDir = path.join(out, "console");
  const stateDir = path.join(out, "state");
  mkdirSync(out, { recursive: true });
  const fixturePath = path.join(out, "virtual_lan.toml");
  writeFileSync(fixturePath, FIXTURE);
  const boardsSpec = [`${A}={merged},kind=rom-up,lan=${LAN}`];
  const extraArgs = ["--lan", `${LAN}=${fixturePath}`];
  const needs = prerequisites();
  const chrome = findChrome();

  if (options.dryRun) {
    const steps = [
      `R1 lp-cli wifi add <usb door> ${NET.ssid} → status connected; hold the USB door as the console`,
      `R2 Studio, no flag: Network row <forward> → Connect → console: secure session opening → card Ready, Reset enabled`,
      `R3 press Reset once → console: "${BOARD.asked}", the ROM's reset banner, an address, a new secure session → card Ready again, no click`,
    ];
    writeFileSync(path.join(out, "walk-plan.json"), JSON.stringify({ out, boards: boardsSpec, extraArgs, steps, prerequisites: needs, chrome }, null, 2));
    console.log("THE EMULATED STUDIO-LAN RESET WALK — dry run: nothing started");
    for (const line of steps) console.log(`  ${line}`);
    for (const need of needs) console.log(`  ${need.present ? "✓" : "✗"} ${need.what}: ${need.at}`);
    console.log(`  ${chrome ? "✓" : "✗"} Chrome: ${chrome ?? "none found (CHROME_BIN)"}`);
    return;
  }
  const missing = needs.filter((need) => !need.present);
  if (missing.length || !chrome) {
    for (const need of missing) console.error(`missing ${need.what}: ${need.at}`);
    if (!chrome) console.error("missing Chrome (set CHROME_BIN)");
    process.exit(1);
  }
  rmSync(shots, { recursive: true, force: true });
  mkdirSync(shots, { recursive: true });
  mkdirSync(consoleDir, { recursive: true });

  const commit = lpEmuCommit();
  const port = walkPort(ROOT, "walk-wifi-emu-studio-lan-reset");
  const server = await serveStudioBundle({ root: ROOT, port });
  const sink = startRecordSink();
  const sinkUrl = await sink.listen();
  const door = await startDoor({
    root: ROOT,
    id: "walk-wifi-studio-lan-reset",
    boards: boardsSpec,
    extraArgs,
    stateDir,
    consoleDir,
    logFile: path.join(out, "serve.log"),
    fresh: true,
  });
  const usb = `serial:ws://${door.addr}/board/${A}/bytes`;
  const registry = await boardRegistry(door.addr);
  const entry = registry.find((b) => b.id === A);
  const fwd = forwardOf(entry);
  const mac = String(entry.mac).toLowerCase();
  let configuration = entry.configuration ?? "unknown";
  const query = new URLSearchParams({ record: sinkUrl });
  const url = `http://localhost:${port}/?${query.toString()}`;

  console.log("\nTHE EMULATED STUDIO-LAN RESET WALK");
  console.log(`  board    ${A} ${mac} → forward ${fwd}`);
  console.log(`  door     http://${door.addr}/boards   (pid ${door.pid})`);
  console.log(`  page     ${url}\n`);

  const report = { lane: "studio-lan-reset", lpEmu: commit, door: door.addr, board: { id: A, mac, fwd }, steps: [] };
  let hold = null;
  let driver = null;
  let page = null;
  let fatal = null;

  const step = async (id, describe, body) => {
    console.log(`— ${id}: ${describe}`);
    let error = null;
    let note = null;
    const seen = {};
    try {
      note = await body(seen);
    } catch (failure) {
      error = failure;
    }
    const shot = path.join(shots, `studio-lan-reset-${report.steps.length + 1}-${id}.png`);
    let shotTaken = null;
    if (driver) {
      try {
        await driver.screenshot(shot);
        shotTaken = shot;
      } catch {
        // the page may be gone
      }
    }
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"}${note ? `  ${JSON.stringify(note)}` : ""}`);
    report.steps.push({ id, describe, ok: !error, error: error?.message ?? null, note, seen, shot: shotTaken });
    if (error) throw error;
    return note;
  };

  try {
    await step("R1", `${A} joins ${NET.ssid} over its USB door`, async (seen) => {
      let added = null;
      for (let attempt = 1; attempt <= 4; attempt += 1) {
        added = await lpCli(["wifi", "add", usb, NET.ssid, "--password-stdin", "--json"], { stdin: NET.password });
        if (added.code === 0 || !added.stderr.includes("did not become ready")) break;
      }
      if (added.code !== 0) throw new Error(`wifi add → exit ${added.code}: ${added.stderr.trim().split("\n").slice(-3).join(" | ")}`);
      const status = await awaitStatus(usb, (s) => ["connected", "failed"].includes(stationOf(s).kind), { what: "connected" });
      const station = stationOf(status);
      if (station.kind !== "connected") throw new Error(`${A} did not join: ${JSON.stringify(status.station)}`);
      seen.station = station;
      configuration = (await boardRegistry(door.addr)).find((b) => b.id === A)?.configuration ?? configuration;
      hold = await holdConsole({ doorAddr: door.addr, board: A, file: path.join(consoleDir, `${A}.link.log`) });
      await hold.console.waitFor("[link] up (session", { what: "its USB link up for the capture" });
      return `${station.ip} (${station.host}) · ${configuration}`;
    });

    await step("R2", `Studio, no flag, connects to ${A} by address; Reset is enabled`, async (seen) => {
      driver = await StudioDriver.launch({ width: 1100, height: 900 });
      await driver.cdp.send("Emulation.setDeviceMetricsOverride", { width: 1100, height: 900, deviceScaleFactor: 2, mobile: false }, driver.sessionId);
      page = new Page(driver);
      await page.load(url);
      const from = hold.console.mark();
      await openNetworkRow(driver, { timeoutMs: STEP_MS });
      await driver.waitFor(`Boolean(${ADDRESS_FIELD})`, { timeoutMs: STEP_MS, what: "the Network row's address field" });
      await driver.type("192.168.1.40", fwd, { scope: "document.querySelector('#main')" });
      await driver.waitFor(PRESS_ADDRESS_CONNECT, { timeoutMs: 30_000, what: "the Network row's Connect" });
      seen.opened = (await hold.console.waitFor(BOARD.session, { from, what: "a secure LAN session opening" })).trim();
      await page.cardSays(fwd, "Ready");
      const shown = await page.cardMac(fwd);
      if (shown !== mac) throw new Error(`the Wi‑Fi card shows ${shown}, not ${A}'s ${mac}`);
      seen.reset = await driver.waitFor(
        `(() => { const b = ${resetButton(fwd)}; return b && !b.disabled ? (b.title || b.getAttribute('aria-label') || 'enabled') : false; })()`,
        { timeoutMs: STEP_MS, what: "the card's Reset to be enabled" },
      );
      return { opened: seen.opened, reset: seen.reset };
    });

    await step("R3", "Reset, pressed once: the board restarts and the card comes back with no click", async (seen) => {
      const from = hold.console.mark();
      const pressedAt = Date.now();
      const clicked = await driver.evaluate(`(() => { const b = ${resetButton(fwd)}; if (!b || b.disabled) return false; b.click(); return true; })()`);
      if (!clicked) throw new Error("the card's Reset was not there to press");
      const at = () => `${((Date.now() - pressedAt) / 1000).toFixed(1)} s`;
      const timeline = {};
      seen.asked = (await hold.console.waitFor(BOARD.asked, { from, what: "it was asked to restart" })).trim();
      timeline.asked = at();
      const afterAck = from + hold.console.since(from).indexOf(seen.asked);
      seen.booted = (await hold.console.waitFor(BOARD.booted, { from: afterAck, what: "the ROM's software-reset banner" })).trim();
      timeline.booted = at();
      // The card leaves "Ready" while the board is away (the link dropped).
      seen.away = await driver
        .waitFor(`!${cardOf(fwd)} || !(${cardOf(fwd)}?.innerText || '').includes('Ready')`, { timeoutMs: 60_000, what: "the card to notice" })
        .then(() => true)
        .catch(() => false);
      timeline.cardAway = seen.away ? at() : "not seen (the redial beat the poll)";
      const afterAsked = afterAck;
      seen.address = (await hold.console.waitFor(BOARD.address, { from: afterAsked, what: "a network address after the restart" })).trim();
      timeline.rejoined = at();
      const afterAddress = afterAsked + hold.console.since(afterAsked).indexOf(seen.address);
      seen.relinked = (await hold.console.waitFor(BOARD.session, { from: afterAddress, what: "a new secure LAN session after the restart" })).trim();
      timeline.sessionOpened = at();
      await page.cardSays(fwd, "Ready", "the card to say Ready again");
      timeline.cardReady = at();
      seen.resetAgain = await driver.waitFor(
        `(() => { const b = ${resetButton(fwd)}; return Boolean(b && !b.disabled); })()`,
        { timeoutMs: STEP_MS, what: "Reset enabled again on the new link" },
      );
      seen.timeline = timeline;
      return { asked: seen.asked, booted: seen.booted, address: seen.address, relinked: seen.relinked, timeline };
    });
  } catch (error) {
    fatal = error;
  }

  const lines = driver ? driver.consoleLines() : [];
  report.pageErrors = lines.filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  report.configuration = configuration;
  writeFileSync(path.join(out, "walk-wifi-emu-studio-lan-reset.json"), JSON.stringify(report, null, 2));
  writeFileSync(path.join(out, "walk.jsonl"), sink.records.map((r) => JSON.stringify(r)).join("\n"));
  console.log("\n=== the studio-lan-reset walk, step by step");
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${s.id}  ${s.describe}`);
  console.log(`  configuration ${configuration} · lp-emu ${commit}`);
  console.log(`  summary → ${path.join(out, "walk-wifi-emu-studio-lan-reset.json")}`);
  console.log(`  console → ${consoleDir}`);

  if (!options.keepOpen) {
    if (driver) await driver.close();
    if (hold) await releaseConsole(hold);
    await stopDoor(door);
    sink.server.close();
    server.close();
  }
  if (fatal) {
    console.error(`\nThe studio-lan-reset walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  console.log(`\n✓ the studio-lan-reset walk finished R1–R3 (${configuration}, lp-emu ${commit}).`);
}

await main();
