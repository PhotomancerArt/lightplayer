#!/usr/bin/env node
// THE EMULATED WALK OF STUDIO REACHING WI‑FI BOARDS WITH NO FLAG
// (network-transport plan P04; `just walk-wifi-emu studio-lan`).
//
// Two emulated ESP32-C6 boards, `c6-a` and `c6-b`, running the packaged
// firmware ROM-up on ONE virtual LAN (`lan=home`, the `lan` lane's fixture
// and door). Real Studio, headless, on its release bundle — and NO `?lan=`
// anywhere: Studio finds the boards the way a person's browser would.
//
//   S1  both boards join the fixture's network (over their USB doors);
//       Studio over the USB shim (`?emu=`) connects c6-a, reads its Wi‑Fi
//       status, and remembers its address in this browser
//       (`lp.devices.wifi-addresses.v1`, keyed by MAC) — the address the
//       board's own status gave
//   S2  the cable is gone (Studio reloads with no `?emu=`: no USB at all):
//       c6-a is a card under Offline boards offering "Connect over Wi‑Fi"; pressed,
//       the board says a secure LAN session opened, the SAME device comes
//       back as a card saying "Wi‑Fi · …", and a project pushed from it
//       lands (the board's console: `Project loaded`, frames advancing)
//   S3  c6-b, never seen: the Network square's row, with its address →
//       it connects (its console, its MAC on the card); a SECOND browser
//       typing c6-a's address is told "Busy with another connection — try
//       again" (c6-a's console: every LAN link in use), and the first
//       page's link to c6-a stays up
//   S4  an address nothing answers at says so in plain words
//
// ⚠️ ONE STAND-IN (ruling DD193), said where it happens (S2): an emulated board's LAN
// address (the virtual LAN's DHCP lease, e.g. 10.0.0.x) is not reachable
// from the host — the door reaches each board through a loopback forward
// (`127.0.0.1:<port>`). So after S1 proves Studio remembered the board's OWN
// address, the walk rewrites that one entry's `ip` to the board's forward
// before Studio reloads, and every claim after it is about the forward. So
// the lane proves Studio remembered the board's OWN lease, then dials the
// host forward in its place. On a desk the remembered address is dialled as
// it is; making a lease dialable from the host is not this lane's to do. Likewise S3 types each
// board's forward where a person would type its IP.
//
// THE BOARD'S WORDS DECIDE EVERY STEP: each board's console (its USB link
// held by `lp-cli link capture`, as the `lan` lane holds it) and its status
// answers over its USB door. Studio's words only say when to look — except
// S4, where no board is involved and the page's sentence is the claim.
//
// NOT CI. Made-up test values only. Headless Chrome. Serves the release
// Studio bundle itself on this worktree's stable slot. Needs: `just
// studio-web-story-build`, `just studio-firmware-package-served` (the merged
// C6 image), `cargo build -p lp-cli`, Chrome. `--dry-run` writes the plan and
// starts nothing. Emulated numbers name the configuration and the lp-emu
// commit; nothing here is a timing claim.

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
  PANEL,
  Page,
  STEP_MS,
  awaitStatus,
  cardOf,
  delay,
  forwardOf,
  holdConsole,
  lpCli,
  lpEmuCommit,
  releaseConsole,
  stationOf,
} from "./walk-wifi-emu-lan.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const LP_CLI = path.join(ROOT, "target/debug/lp-cli");
const BOARDS = ["c6-a", "c6-b"];
const [A, B] = BOARDS;

/// The address book's key (`WIFI_ADDRESSES_STORAGE_KEY`).
const BOOK_KEY = "lp.devices.wifi-addresses.v1";
/// W3's project (walk-no-board's, for its measured reason).
const WALK_PROJECT = process.env.WALK_PROJECT ?? "Peach (1D)";
/// An address nothing listens at (port 9, discard: never a board's).
const NOWHERE = "127.0.0.1:9";

/// Core's words (`wifi_connect_failure.rs`, `wifi_connect_op.rs`). They say
/// where to look; the boards' consoles say what happened.
const WORDS = {
  connectOverWifi: "Connect over Wi‑Fi",
  busy: "Busy with another connection — try again",
  unreachable: (host) => `Couldn't reach the board at ${host}. Is it on this network?`,
  wifiLine: (address) => `Wi‑Fi · ${address}`,
};

/// The Network row's address field and its Connect (`wifi_address_entry.rs`);
/// the row opens when the Network square is pressed (`openNetworkRow`).
const ADDRESS_FIELD = `document.querySelector('#main input[placeholder^="192.168.1.40"]')`;
/// The grid holding the field, its Connect and its status line.
const ADDRESS_ENTRY = `${ADDRESS_FIELD}?.closest('label')?.parentElement?.parentElement`;

function usage() {
  return "usage: node scripts/emu/walk-wifi-emu.mjs studio-lan [--out <dir>] [--keep-open] [--dry-run]";
}

function parseArgs(argv) {
  const rest = argv[0] === "studio-lan" ? argv.slice(1) : argv;
  const options = { out: path.join(ROOT, "target/walk-wifi-emu/studio-lan"), keepOpen: false, dryRun: false };
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

/// Studio with no `?lan=`: over the USB shim (`emu`), or with nothing at all.
function studioUrl({ studioPort, doorAddr = null, sinkUrl }) {
  const query = new URLSearchParams();
  if (doorAddr) query.set("emu", `ws://${doorAddr}`);
  query.set("record", sinkUrl);
  return `http://localhost:${studioPort}/?${query.toString()}`;
}

/// A MAC as the address book keys it: 12 lowercase hex.
const bookKey = (mac) => String(mac).toLowerCase().replace(/[^0-9a-f]/g, "");

/// The address book as this page's storage holds it.
const READ_BOOK = `(() => { try { return JSON.parse(localStorage.getItem(${JSON.stringify(BOOK_KEY)}) || '{}'); } catch { return {}; } })()`;

/// Type `text` into the Network row's address field, as a keyboard does for
/// Dioxus (`input` events carry the value), and press its Connect.
function typeAddressAndConnect(text) {
  return `(() => {
    const field = ${ADDRESS_FIELD};
    if (!field) return 'no field';
    field.focus();
    field.value = ${JSON.stringify(text)};
    field.dispatchEvent(new Event('input', { bubbles: true }));
    return 'typed';
  })()`;
}

const PRESS_ADDRESS_CONNECT = `(() => {
  const entry = ${ADDRESS_ENTRY};
  const button = [...(entry?.querySelectorAll('button') ?? [])].find((b) => !b.disabled && (b.innerText || '').trim() === 'Connect');
  if (!button) return false;
  button.click();
  return true;
})()`;

const ADDRESS_STATUS = `(${ADDRESS_ENTRY}?.querySelector('[role="status"]')?.innerText || '')`;

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
  const boardsSpec = BOARDS.map((id) => `${id}={merged},kind=rom-up,lan=${LAN}`);
  const extraArgs = ["--lan", `${LAN}=${fixturePath}`];
  const needs = prerequisites();
  const chrome = findChrome();

  if (options.dryRun) {
    const steps = [
      `S1 lp-cli wifi add <usb door> ${NET.ssid} (each board) → status connected; Studio ?emu= → the USB square → ${A} Ready → ${BOOK_KEY}[${A}'s MAC].ip == its status ip`,
      `S2 hold both consoles; book entry ip → ${A}'s forward (the one substitution); Studio with no ?emu= → ${A}'s tile → ${WORDS.connectOverWifi} → console: secure session opening → card "${WORDS.wifiLine("<fwd a>")}" Ready, ${A}'s MAC, one device → push ${WALK_PROJECT} → console: Project loaded, frames advance`,
      `S3 Network row: <fwd b> → Connect → ${B}'s console: secure session opening → card Ready with ${B}'s MAC; a second browser: <fwd a> → "${WORDS.busy}" and ${A}'s console: every LAN link in use; ${A}'s first link not closed`,
      `S4 the second browser: ${NOWHERE} → "${WORDS.unreachable(NOWHERE)}"`,
    ];
    writeFileSync(path.join(out, "walk-plan.json"), JSON.stringify({ out, boards: boardsSpec, extraArgs, steps, prerequisites: needs, chrome }, null, 2));
    console.log("THE EMULATED STUDIO-LAN WALK — dry run: nothing started");
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
  const port = walkPort(ROOT, "walk-wifi-emu-studio-lan");
  const server = await serveStudioBundle({ root: ROOT, port });
  const sink = startRecordSink();
  const sinkUrl = await sink.listen();
  const door = await startDoor({
    root: ROOT,
    id: "walk-wifi-studio-lan",
    boards: boardsSpec,
    extraArgs,
    stateDir,
    consoleDir,
    logFile: path.join(out, "serve.log"),
    fresh: true,
  });
  const usb = (id) => `serial:ws://${door.addr}/board/${id}/bytes`;
  const registry = await boardRegistry(door.addr);
  const entry = (id) => registry.find((b) => b.id === id);
  const fwd = Object.fromEntries(BOARDS.map((id) => [id, forwardOf(entry(id))]));
  const mac = Object.fromEntries(BOARDS.map((id) => [id, String(entry(id).mac).toLowerCase()]));
  let configuration = entry(A).configuration ?? "unknown";
  const usbUrl = studioUrl({ studioPort: port, doorAddr: door.addr, sinkUrl });
  const plainUrl = studioUrl({ studioPort: port, sinkUrl });

  console.log("\nTHE EMULATED STUDIO-LAN WALK (no ?lan=)");
  console.log(`  boards   ${BOARDS.map((id) => `${id} ${mac[id]} → forward ${fwd[id]}`).join(" · ")}`);
  console.log(`  door     http://${door.addr}/boards   (pid ${door.pid})`);
  console.log(`  page     ${plainUrl}\n`);

  const report = { lane: "studio-lan", lpEmu: commit, door: door.addr, boards: { fwd, mac }, steps: [] };
  const holds = {};
  let driver = null;
  let second = null;
  let fatal = null;

  const step = async (id, describe, body, { fatal: stops = false } = {}) => {
    console.log(`— ${id}: ${describe}`);
    let error = null;
    let note = null;
    const seen = {};
    try {
      note = await body(seen);
    } catch (failure) {
      error = failure;
    }
    const shot = path.join(shots, `studio-lan-${report.steps.length + 1}-${id}.png`);
    let shotTaken = null;
    for (const [suffix, from] of [["", driver], ["-second", second]]) {
      if (!from) continue;
      try {
        await from.screenshot(shot.replace(/\.png$/, `${suffix}.png`));
        shotTaken ??= shot.replace(/\.png$/, `${suffix}.png`);
      } catch {
        // the page may be gone
      }
    }
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"}${note ? `  ${typeof note === "string" ? note : JSON.stringify(note)}` : ""}`);
    report.steps.push({ id, describe, ok: !error, error: error?.message ?? null, note, seen, shot: shotTaken });
    if (error && stops) throw error;
    return note;
  };

  const joined = {};
  try {
    await step("S1", `both boards join; Studio over USB meets ${A} and remembers its address`, async (seen) => {
      for (const id of BOARDS) {
        let added = null;
        for (let attempt = 1; attempt <= 4; attempt += 1) {
          added = await lpCli(["wifi", "add", usb(id), NET.ssid, "--password-stdin", "--json"], { stdin: NET.password });
          if (added.code === 0 || !added.stderr.includes("did not become ready")) break;
        }
        if (added.code !== 0) throw new Error(`wifi add on ${id} → exit ${added.code}: ${added.stderr.trim().split("\n").slice(-3).join(" | ")}`);
      }
      for (const id of BOARDS) {
        const status = await awaitStatus(usb(id), (s) => ["connected", "failed"].includes(stationOf(s).kind), { what: "connected" });
        const station = stationOf(status);
        if (station.kind !== "connected") throw new Error(`${id} did not join: ${JSON.stringify(status.station)}`);
        joined[id] = { ip: station.ip, host: station.host };
      }
      seen.joined = joined;
      configuration = (await boardRegistry(door.addr)).find((b) => b.id === A)?.configuration ?? configuration;

      driver = await StudioDriver.launch({ width: 1100, height: 900 });
      await driver.cdp.send("Emulation.setDeviceMetricsOverride", { width: 1100, height: 900, deviceScaleFactor: 2, mobile: false }, driver.sessionId);
      await driver.navigate(usbUrl);
      await driver.awaitShim();
      await driver.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: 420_000, what: "Studio to finish loading" });
      // c6-b never meets this browser: its cable stays out for this page.
      await driver.detach(B);
      await driver.pressConnect("USB", { timeoutMs: STEP_MS });
      await driver.pickBoard(A, { timeoutMs: STEP_MS });
      await driver.waitFor(`${MAIN_TEXT}.includes('Ready')`, { timeoutMs: STEP_MS, what: `${A} Ready over USB` });
      // The board's own address (its status, read by lp-cli above) is what
      // Studio remembered, by the board's MAC — and nothing for c6-b.
      const book = await driver.waitFor(
        `(() => { const b = ${READ_BOOK}; const e = b[${JSON.stringify(bookKey(mac[A]))}]; return e && e.ip === ${JSON.stringify(joined[A].ip)} ? b : false; })()`,
        { timeoutMs: STEP_MS, what: `Studio to remember ${A} at ${joined[A].ip}` },
      );
      seen.book = book;
      if (book[bookKey(mac[B])]) throw new Error(`Studio remembered ${B}, which it never met: ${JSON.stringify(book)}`);
      // Put c6-b's cable back for its console capture (the door's state, not
      // this page's), then let go of the page's USB links.
      await driver.attach(B);
      await driver.navigate("about:blank");
      return `${A} ${joined[A].ip} (${joined[A].host}) remembered as ${JSON.stringify(book[bookKey(mac[A])])} · ${B} ${joined[B].ip} · ${configuration}`;
    }, { fatal: true });

    // From here each board's USB link is the walk's: its console.
    for (const id of BOARDS) {
      holds[id] = await holdConsole({ doorAddr: door.addr, board: id, file: path.join(consoleDir, `${id}.link.log`) });
    }
    for (const id of BOARDS) await holds[id].console.waitFor("[link] up (session", { what: "its USB link up for the capture" });
    const page = new Page(driver);

    await step("S2", `no cable: ${A}'s tile offers "${WORDS.connectOverWifi}"; pressed, it comes back over Wi‑Fi and an edit lands`, async (seen) => {
      // The stand-in (DD193, see the header): the remembered entry's ip
      // becomes the board's forward, so the host can dial it.
      await page.load(plainUrl);
      const rewritten = await driver.evaluate(`(() => {
        const b = ${READ_BOOK}; const k = ${JSON.stringify(bookKey(mac[A]))};
        if (!b[k]) return null;
        b[k].ip = ${JSON.stringify(fwd[A])};
        localStorage.setItem(${JSON.stringify(BOOK_KEY)}, JSON.stringify(b));
        return b[k];
      })()`);
      if (!rewritten) throw new Error(`the book lost ${A} across the reload`);
      seen.substituted = rewritten;
      await page.load(plainUrl);
      const from = holds[A].console.mark();
      // The board is a card under Offline boards, always open: wait for its
      // verb THERE (not for a control elsewhere on the page that says the
      // same), then press it.
      await driver.clickWhenReady(WORDS.connectOverWifi, {
        timeoutMs: STEP_MS,
        scope: `document.querySelector('#home-offline-boards')`,
      });
      seen.opened = (await holds[A].console.waitFor(/\[lan\] link \S+ .*secure session opening/, { from, what: "a secure LAN session opening" })).trim();
      await page.cardSays(fwd[A], "Ready");
      const shown = await page.cardMac(fwd[A]);
      if (shown !== mac[A]) throw new Error(`the Wi‑Fi card shows ${shown}, not ${A}'s ${mac[A]}`);
      // The same device: no card left under Offline boards for it. c6-b was
      // never seen by this browser, so nothing else is remembered and the
      // whole section is gone.
      const offline = await driver.evaluate(`Boolean(document.querySelector('#home-offline-boards'))`);
      seen.offlineSection = offline;
      if (offline) throw new Error(`Offline boards is still drawn after the merge: ${await driver.evaluate(MAIN_TEXT)}`);
      // An edit lands: a project pushed from the Wi‑Fi card loads on the board.
      const card = `(${cardOf(fwd[A])}?.innerText || '')`;
      const face = await driver.waitFor(
        `(() => { const t = ${card}; return t.includes('Remove project') ? 'running' : t.includes('to choose from') ? 'empty' : false; })()`,
        { timeoutMs: STEP_MS, what: `${A}'s card to say what it runs` },
      );
      if (face === "running") {
        await page.clickInCard(fwd[A], "Remove project");
        await driver.click("Remove project", { scope: cardOf(fwd[A]) });
        await driver.waitFor(`${card}.includes('to choose from')`, { timeoutMs: STEP_MS, what: `${A}'s empty face` });
      }
      const pushed = holds[A].console.mark();
      await page.clickInCard(fwd[A], "to choose from");
      await driver.waitFor(`Boolean(${PANEL})`, { timeoutMs: STEP_MS, what: "the project popover" });
      await driver.click(WALK_PROJECT, { scope: PANEL });
      await page.clickInCard(fwd[A], "Put it on the board");
      seen.loadLine = (await holds[A].console.waitFor("Project loaded", { from: pushed, what: "`Project loaded`" })).trim();
      seen.frameCounts = await holds[A].console.framesAdvance({ from: pushed });
      return { opened: seen.opened, loaded: seen.loadLine, frames: seen.frameCounts };
    }, { fatal: true });
    const aOpen = holds[A].console.mark();

    await step("S3", `${B}, never seen, by its address; a second browser to ${A} is told it is busy`, async (seen) => {
      const from = holds[B].console.mark();
      await openNetworkRow(driver, { timeoutMs: STEP_MS });
      const typed = await driver.evaluate(typeAddressAndConnect(fwd[B]));
      if (typed !== "typed") throw new Error(`the Network row's address field: ${typed}`);
      await driver.waitFor(PRESS_ADDRESS_CONNECT, { timeoutMs: 30_000, what: "the Network row's Connect" });
      seen.bOpened = (await holds[B].console.waitFor(/\[lan\] link \S+ .*secure session opening/, { from, what: "a secure LAN session opening" })).trim();
      await page.cardSays(fwd[B], "Ready");
      const shown = await page.cardMac(fwd[B]);
      if (shown !== mac[B]) throw new Error(`the card at ${fwd[B]} shows ${shown}, not ${B}'s ${mac[B]}`);

      second = await StudioDriver.launch({ width: 1100, height: 900 });
      await second.navigate(plainUrl);
      await second.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: 420_000, what: "the second Studio to load" });
      // S4 (below) types into the same page later: the row stays open.
      await openNetworkRow(second, { timeoutMs: STEP_MS });
      const busyFrom = holds[A].console.mark();
      const typedSecond = await second.evaluate(typeAddressAndConnect(fwd[A]));
      if (typedSecond !== "typed") throw new Error(`the second page's address field: ${typedSecond}`);
      await second.waitFor(PRESS_ADDRESS_CONNECT, { timeoutMs: 30_000, what: "the second page's Connect" });
      seen.turnedAway = (await holds[A].console.waitFor("[lan] every LAN link is in use", { from: busyFrom, what: "the second link turned away" })).trim();
      seen.said = await second.waitFor(`(() => { const t = ${ADDRESS_STATUS}; return t.includes(${JSON.stringify(WORDS.busy)}) ? t : false; })()`, {
        timeoutMs: STEP_MS,
        what: "the second page to say the board is busy",
      });
      const closed = holds[A].console.since(aOpen).split("\n").filter((l) => /\[lan\] link \S+: closed/.test(l));
      if (closed.length) throw new Error(`${A} closed the first page's LAN link: ${closed[0]}`);
      return { b: seen.bOpened, turnedAway: seen.turnedAway, said: seen.said };
    });

    await step("S4", `an address nothing answers at says so`, async (seen) => {
      const typed = await second.evaluate(typeAddressAndConnect(NOWHERE));
      if (typed !== "typed") throw new Error(`the second page's address field: ${typed}`);
      await second.waitFor(PRESS_ADDRESS_CONNECT, { timeoutMs: 30_000, what: "the second page's Connect" });
      seen.said = await second.waitFor(
        `(() => { const t = ${ADDRESS_STATUS}; return t.includes(${JSON.stringify(WORDS.unreachable(NOWHERE))}) ? t : false; })()`,
        { timeoutMs: STEP_MS, what: "the page to say nothing answered" },
      );
      return seen.said;
    });
  } catch (error) {
    fatal = error;
  }

  report.configuration = configuration;
  writeFileSync(path.join(out, "walk-wifi-emu-studio-lan.json"), JSON.stringify(report, null, 2));
  writeFileSync(path.join(out, "walk.jsonl"), sink.records.map((r) => JSON.stringify(r)).join("\n"));
  console.log("\n=== the studio-lan walk, step by step");
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${s.id}  ${s.describe}`);
  console.log(`  configuration ${configuration} · lp-emu ${commit}`);
  console.log(`  summary → ${path.join(out, "walk-wifi-emu-studio-lan.json")}`);
  console.log(`  consoles → ${consoleDir}`);

  if (!options.keepOpen) {
    if (second) await second.close();
    if (driver) await driver.close();
    for (const hold of Object.values(holds)) await releaseConsole(hold);
    await stopDoor(door);
    sink.server.close();
    server.close();
  }
  if (fatal) {
    console.error(`\nThe studio-lan walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  const failed = report.steps.filter((s) => !s.ok);
  if (failed.length) {
    console.error(`\nThe studio-lan walk ran to the end, but ${failed.map((s) => s.id).join(", ")} failed.`);
    process.exit(1);
  }
  await delay(0);
  console.log(`\n✓ the studio-lan walk finished S1–S4 (${configuration}, lp-emu ${commit}).`);
}

await main();
