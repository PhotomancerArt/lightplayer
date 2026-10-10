#!/usr/bin/env node
// THE EMULATED WALK OF STUDIO REACHING A BOARD THROUGH LIGHTPLAYER.APP WITH
// NO FLAG (network-transport plan, PR C; `just walk-wifi-emu studio-relay`).
//
// One emulated ESP32-C6, `c6-a`, running the packaged firmware ROM-up on a
// virtual LAN whose fixture names an uplink to `lightplayer.app` — played by
// a REAL lp-cloud-server on this machine (mem store, a made-up account
// signed in by dev sign-in; `walk-ota-relay.mjs`, the OTA walk's relay
// cloud). Real Studio, headless, on its release bundle, served on the
// walk's own origin with `/api`, `/auth` and `/relay` forwarded to that
// server (one origin, as on lightplayer.app, so the session cookie rides the
// relay's browser leg). NO `?relay=` and NO `?lan=` anywhere: the relay is on
// for everyone, and a board is reached the way a person reaches it.
//
//   T1  the board joins the fixture's network over its USB door and answers
//       `relay noAccount`; Studio, SIGNED IN, over the USB shim (`?emu=`),
//       meets it — and its USB connect puts the account's key on the board
//       (Studio's own sync, not a seeded chip): the board then answers
//       `relay connected` (it registered with the relay by itself)
//   T2  the cable is gone and nobody is signed in (no `?emu=`, no cookie):
//       the board is a card under Offline boards, and it does NOT offer
//       "Connect through lightplayer.app" (`connect-relay`) anywhere on it
//   T3  signed in again, still no cable: the card offers `connect-relay`
//       (its primary, or in its connection details when Connect over Wi‑Fi
//       leads, `primary_action.rs`); pressed, the hub opens a member session
//       to the board, the BOARD says a relay route's secure session opened,
//       and the SAME device (its card, keyed by its MAC) comes back ready on
//       the relay: its connection bar reads "Wi‑Fi via lightplayer.app" and
//       names neither Bluetooth nor USB, and its connection details hold no
//       USB line
//   T4  the board cannot reach the relay (the walk drops its device leg and
//       refuses new ones): the page's session ends, the card is a card under
//       Offline boards again, and a press says "The board isn't online." on
//       its connection bar (the connect's failed work; the hub's 4404); once
//       the board is back on the relay (its own `[relay] leg open`), a press
//       brings the card back ready
//
// THE BOARD'S WORDS DECIDE EVERY STEP: its status answers over its USB door
// (T1), then its console (its USB link held by `lp-cli link capture` once
// the page lets go of it), and the hub's log for the browser leg. Studio's
// words say when to look — except the words this PR is about (the card's
// link line, the connect's refusal), which are the claim. The card is read by
// its hooks (`studio-driver.mjs`'s card helpers), by the board's MAC, and its
// verbs pressed by their offer paths: never by its face text.
//
// ⚠️ A local lp-cloud-server stands in for lightplayer.app: no internet, no
// fly proxy, no NAT, no real round trips; the board's leg crosses the
// emulated LAN's uplink. It proves the offer, the dial, the card's words and
// the hub's refusal end to end on the shipped firmware and Studio — NOT the
// production edge, iPhone Safari or Chrome's Local Network prompt (those are
// a desk walk's).
//
// NOT CI. Made-up test values only. Headless Chrome. Needs: `just
// studio-web-story-build`, `just studio-firmware-package-esp32c6` (the merged
// C6 image), `cargo build -p lp-cli -p lp-cloud-server`, Chrome. `--dry-run`
// writes the plan and starts nothing. Emulated numbers name the
// configuration and the lp-emu commit; nothing here is a timing claim.

import { existsSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { StudioDriver, boardPath, findChrome, macOfPath } from "./studio-driver.mjs";
import {
  PACKAGED_C6_MERGED,
  RELEASE_BUNDLE,
  boardRegistry,
  serveStudioBundle,
  startDoor,
  stopDoor,
  walkPort,
} from "./emulated-lane.mjs";
import {
  FIXTURE,
  LAN,
  MAIN_TEXT,
  NET,
  STEP_MS,
  awaitStatus,
  delay,
  holdConsole,
  lpCli,
  lpEmuCommit,
  releaseConsole,
  stationOf,
} from "./walk-wifi-emu-lan.mjs";
import { WALK_EMAIL, forwardHttp, forwardUpgrade, relayId, startRelayCloud } from "./walk-ota-relay.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const LP_CLI = path.join(ROOT, "target/debug/lp-cli");
const LP_CLOUD_SERVER = path.join(ROOT, "target/debug/lp-cloud-server");
const A = "c6-a";
const JOIN_MS = 420_000;

/// Core's words (`relay_connect_op.rs`, `relay_connect_failure.rs`,
/// `ui_link_kind.rs`, `board_card/connection_bar.rs`). The connect and the
/// link line are what this PR adds; the board's words say what happened.
const WORDS = {
  // `connect-relay`'s own words, where the connection details draw it; as
  // the offline card's primary it reads "Connect". Pressed by its offer.
  connectRelay: "Connect through lightplayer.app",
  // `UiLinkKind::Relay.label()`: the connection bar's line on a relay link
  // (`<link> · connected` or `· live`).
  relayLine: "Wi‑Fi via lightplayer.app",
  // `RELAY_OFFLINE_WORDS`: a relay connect the hub refused (4404), the
  // connection bar's failed work.
  offline: "The board isn't online.",
  // The other links' words (`UiLinkKind::label`), which a relay card's
  // connection bar must not say; and the connection details' USB line
  // (`Links` → "USB", drawn only on a USB link).
  bluetooth: "Bluetooth",
  usb: "USB",
};

function usage() {
  return "usage: node scripts/emu/walk-wifi-emu.mjs studio-relay [--out <dir>] [--keep-open] [--dry-run]";
}

function parseArgs(argv) {
  const rest = argv[0] === "studio-relay" ? argv.slice(1) : argv;
  const options = { out: path.join(ROOT, "target/walk-wifi-emu/studio-relay"), keepOpen: false, dryRun: false };
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
    ["a debug lp-cloud-server (cargo build -p lp-cloud-server)", LP_CLOUD_SERVER],
    ["the release Studio bundle (just studio-web-story-build)", path.join(ROOT, RELEASE_BUNDLE)],
    ["the packaged merged C6 image (just studio-firmware-package-esp32c6)", path.join(ROOT, PACKAGED_C6_MERGED)],
  ].map(([what, at]) => ({ what, at, present: existsSync(at) }));
}

/// The bars of a board card, by their `data-bar` layer.
const BARS = ["project", "connection", "access", "firmware", "hardware"];

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
  const boardSpec = `${A}={merged},kind=rom-up,lan=${LAN}`;
  const needs = prerequisites();
  const chrome = findChrome();

  if (options.dryRun) {
    const steps = [
      `T1 lp-cli wifi add <usb door> ${NET.ssid} → status connected, relay noAccount; Studio signed in, ?emu= → the USB square → ${A}'s card ready (push or edit offered) → status relay connected`,
      `T2 no cookie, no ?emu= → ${A}'s card under Offline boards, and no connect-relay ("${WORDS.connectRelay}") on it, face or connection details`,
      `T3 signed in, no ?emu= → ${A}'s offline card → connect-relay → hub: member session open; board: [relay] route … secure session opening → its card (${A}'s MAC) ready, connection bar "${WORDS.relayLine} · …", one device`,
      `T4 the device leg cut and refused → card offline → connect-relay → connection bar "${WORDS.offline}" (failed); leg allowed → board: [relay] leg open → press → ready`,
    ];
    writeFileSync(path.join(out, "walk-plan.json"), JSON.stringify({ out, board: boardSpec, steps, prerequisites: needs, chrome }, null, 2));
    console.log("THE EMULATED STUDIO-RELAY WALK — dry run: nothing started");
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
  /// The relay's cloud, once started (below): the Studio server forwards to it.
  let cloud = null;
  const route = (request, response, url) => {
    if (cloud && /^\/(api|auth|relay)(\/|$)/.test(url.pathname)) {
      forwardHttp(request, response, cloud.port);
      return true;
    }
    return false;
  };
  const port = walkPort(ROOT, "walk-wifi-emu-studio-relay");
  const server = await serveStudioBundle({ root: ROOT, port, route });
  const studioOrigin = `http://localhost:${port}`;
  cloud = await startRelayCloud({
    root: ROOT,
    binary: LP_CLOUD_SERVER,
    studioOrigin,
    log: path.join(out, "lp-cloud-server.log"),
  });
  server.on("upgrade", (request, socket, head) => {
    if (new URL(request.url, "http://x").pathname.startsWith("/relay/")) forwardUpgrade(request, socket, head, cloud.port);
    else socket.destroy();
  });
  writeFileSync(fixturePath, `${FIXTURE}\n[[uplink]]\nname = "lightplayer.app"\nto = "127.0.0.1:${cloud.devicePort}"\n`);
  const door = await startDoor({
    root: ROOT,
    id: "walk-wifi-studio-relay",
    boards: [boardSpec],
    extraArgs: ["--lan", `${LAN}=${fixturePath}`],
    stateDir,
    consoleDir,
    logFile: path.join(out, "serve.log"),
    fresh: true,
  });
  const usb = `serial:ws://${door.addr}/board/${A}/bytes`;
  const entry = (await boardRegistry(door.addr)).find((b) => b.id === A);
  const mac = String(entry.mac).toLowerCase();
  const board = relayId(mac);
  let configuration = entry.configuration ?? "unknown";
  const usbUrl = `${studioOrigin}/?emu=${encodeURIComponent(`ws://${door.addr}`)}`;
  const plainUrl = `${studioOrigin}/`;

  console.log("\nTHE EMULATED STUDIO-RELAY WALK (no ?relay=, no ?lan=)");
  console.log("  ⚠️  a local lp-cloud-server stands in for lightplayer.app: no internet, no fly proxy, no");
  console.log("     NAT, no real round trips; the board's leg crosses the emulated LAN's uplink.");
  console.log(`  board    ${A} ${mac} (relay id ${board})`);
  console.log(`  relay    ${cloud.origin} (device leg via 127.0.0.1:${cloud.devicePort}); account ${WALK_EMAIL}`);
  console.log(`  door     http://${door.addr}/boards   (pid ${door.pid})`);
  console.log(`  page     ${plainUrl}\n`);

  const report = { lane: "studio-relay", lpEmu: commit, door: door.addr, board: { mac, relayId: board }, steps: [] };
  let hold = null;
  let driver = null;
  let fatal = null;
  const hubSince = (mark) => cloud.log().slice(mark);
  const waitHub = async (pattern, what, from, timeoutMs = STEP_MS) => {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const hit = hubSince(from).match(pattern);
      if (hit) return hit[0];
      if (Date.now() > deadline) throw new Error(`the hub never said ${what} (${pattern})`);
      await delay(250);
    }
  };
  const signIn = (on) =>
    on
      ? driver.cdp.send(
          "Network.setCookie",
          { name: "lp_session", value: cloud.cookie, url: studioOrigin, path: "/", httpOnly: true },
          driver.sessionId,
        )
      : driver.cdp.send("Network.deleteCookies", { name: "lp_session", url: studioOrigin }, driver.sessionId);
  const load = async (url) => {
    await driver.navigate(url);
    await driver.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: 420_000, what: "Studio to finish loading" });
  };
  /// The board's card under Offline boards, which is always open: wait for
  /// ITS card there (`#home-offline-boards`, by the hook keyed by its MAC),
  /// not for text elsewhere on the page.
  const openRemembered = async () => {
    await driver.waitFor(
      `Boolean(document.querySelector(${JSON.stringify(`#home-offline-boards [data-board-card="${boardPath(mac)}"]`)}))`,
      { timeoutMs: STEP_MS, what: "the board's card under Offline boards" },
    );
  };
  /// Page-side: whether the board's card draws `connect-relay` anywhere —
  /// its face, or details open on it.
  const relayDrawn = async () =>
    `Boolean(${await driver.card({ board: mac })}?.querySelector('[data-offer-path$="/connect-relay"]'))`;
  /// The verbs the board's card draws right now (its face, and any details
  /// open on it), by their offer paths' last segment.
  const cardVerbs = async () =>
    driver.evaluate(`[...new Set([...(${await driver.card({ board: mac })}?.querySelectorAll('[data-offer-path]') ?? [])]
      .map((mark) => mark.getAttribute('data-offer-path').split('/').pop()))]`);
  /// Each of the card's bars as it reads (`StudioDriver.barText`).
  const cardLines = async () => {
    const lines = {};
    for (const layer of BARS) lines[layer] = await driver.barText(layer, { board: mac });
    return lines;
  };
  /// Press `connect-relay` where core put it on the offline card: its
  /// primary when the cloud is its first way back, else in the connection
  /// details — Wi‑Fi first, then the cloud, then the cable
  /// (`primary_action.rs`), and a board met over USB is remembered with its
  /// Wi‑Fi address, so `connect-wifi` may lead. A failed connect's Retry, on
  /// the connection bar's face, is the same offer. Waits for the card to
  /// show one of the two; returns the pressed button's words.
  const pressConnectRelay = async () => {
    const scope = await driver.card({ board: mac });
    const where = await driver.waitFor(
      `(() => { const card = ${scope}; if (!card) return false;
                const face = (verb) => [...card.querySelectorAll('[data-offer-path$="/' + verb + '"]')]
                  .some((mark) => !mark.closest('.ux-popover-layer'));
                return face('connect-relay') ? 'face' : face('connect-wifi') ? 'connection' : false; })()`,
      { timeoutMs: STEP_MS, what: "the card to offer `connect-relay` (or `connect-wifi` ahead of it)" },
    );
    const bar = where === "face" ? null : "connection";
    const pressed = await driver.pressOffer("connect-relay", { board: mac, bar, timeoutMs: STEP_MS });
    if (bar) await driver.closeDetails({ board: mac }).catch(() => {});
    return `${pressed} (${bar ? "the connection details" : "the card's face"})`;
  };
  /// The board's card back on the relay and ready: its connection bar reads
  /// the relay link, and core offers it a project or the editor. Returns the
  /// connection bar's line.
  const readyOnRelay = async () => {
    const line = await driver.waitBar("connection", WORDS.relayLine, { board: mac, timeoutMs: STEP_MS });
    await driver.boardRuns({ board: mac, timeoutMs: STEP_MS });
    return line;
  };

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
    const shot = path.join(shots, `studio-relay-${report.steps.length + 1}-${id}.png`);
    let shotTaken = null;
    if (driver) {
      try {
        await driver.screenshot(shot);
        shotTaken = shot;
      } catch {
        // the page may be gone
      }
    }
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"}${note ? `  ${typeof note === "string" ? note : JSON.stringify(note)}` : ""}`);
    report.steps.push({ id, describe, ok: !error, error: error?.message ?? null, note, seen, shot: shotTaken });
    if (error && stops) throw error;
    return note;
  };

  try {
    await step("T1", `${A} joins; Studio, signed in, meets it over USB and puts the account's key on it`, async (seen) => {
      let added = null;
      for (let attempt = 1; attempt <= 4; attempt += 1) {
        added = await lpCli(["wifi", "add", usb, NET.ssid, "--password-stdin", "--json"], { stdin: NET.password });
        if (added.code === 0 || !added.stderr.includes("did not become ready")) break;
      }
      if (added.code !== 0) throw new Error(`wifi add → exit ${added.code}: ${added.stderr.trim().split("\n").slice(-3).join(" | ")}`);
      const before = await awaitStatus(usb, (s) => stationOf(s).kind === "connected" && s.relay !== undefined, {
        what: "joined, with its relay state",
        timeoutMs: JOIN_MS,
      });
      seen.before = { station: stationOf(before), relay: before.relay };
      if (JSON.stringify(before.relay) !== JSON.stringify("noAccount")) {
        throw new Error(`before Studio, the board's relay is ${JSON.stringify(before.relay)}, not noAccount`);
      }
      configuration = (await boardRegistry(door.addr)).find((b) => b.id === A)?.configuration ?? configuration;

      driver = await StudioDriver.launch({ width: 1100, height: 900 });
      await driver.cdp.send("Emulation.setDeviceMetricsOverride", { width: 1100, height: 900, deviceScaleFactor: 2, mobile: false }, driver.sessionId);
      await signIn(true);
      await driver.navigate(usbUrl);
      await driver.awaitShim();
      await driver.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: 420_000, what: "Studio to finish loading" });
      await driver.pressConnect("USB", { timeoutMs: STEP_MS });
      await driver.pickBoard(A, { timeoutMs: STEP_MS });
      // Ready over USB, as core reads it: the board's card (by its MAC)
      // offers it a project or the editor.
      await driver.waitCard({ board: mac, timeoutMs: STEP_MS });
      await driver.boardRuns({ board: mac, timeoutMs: STEP_MS });
      // The toast Studio raises when its USB connect added keys on its own.
      seen.toast = await driver
        .waitFor(`(() => { const t = document.querySelector('.ux-access-toast')?.innerText || ''; return t.includes('account') ? t : false; })()`, {
          timeoutMs: STEP_MS,
          what: "the toast naming the account's key",
        })
        .catch(() => null);
      // Let go of the USB link, then ask the board itself.
      await driver.navigate("about:blank");
      const after = await awaitStatus(usb, (s) => JSON.stringify(s.relay) === JSON.stringify("connected"), {
        what: "relay connected (it registered with the relay by itself)",
        timeoutMs: JOIN_MS,
      });
      seen.after = { relay: after.relay };
      return `relay noAccount → ${JSON.stringify(after.relay)} after Studio's USB connect · toast: ${JSON.stringify(seen.toast)} · ${configuration}`;
    }, { fatal: true });

    // From here the board's USB link is the walk's: its console.
    hold = await holdConsole({ doorAddr: door.addr, board: A, file: path.join(consoleDir, `${A}.link.log`) });
    await hold.console.waitFor("[link] up (session", { what: "its USB link up for the capture" });

    await step("T2", `no cable, signed out: ${A}'s tile does not offer "${WORDS.connectRelay}"`, async (seen) => {
      await signIn(false);
      await load(plainUrl);
      await openRemembered();
      // Cached account keys may draw it for a moment at boot; it goes once
      // the page knows nobody is signed in, and stays gone — from the card's
      // face here, and from its connection details below, where it stands
      // when Connect over Wi‑Fi leads.
      const drawn = await relayDrawn();
      await driver.waitFor(`!${drawn}`, {
        timeoutMs: STEP_MS,
        what: "no relay connect while signed out",
      });
      await delay(8_000);
      await driver.openBar("connection", { board: mac, timeoutMs: STEP_MS });
      const still = await driver.evaluate(drawn);
      seen.tile = await cardVerbs();
      await driver.closeDetails({ board: mac });
      if (still) throw new Error(`signed out, the card still offers \`connect-relay\`: its verbs ${JSON.stringify(seen.tile)}`);
      return seen.tile;
    });

    await step("T3", `signed in, no cable: "${WORDS.connectRelay}" brings ${A} back as the same device`, async (seen) => {
      await signIn(true);
      await load(plainUrl);
      await openRemembered();
      const hubFrom = cloud.log().length;
      const from = hold.console.mark();
      seen.pressed = await pressConnectRelay();
      seen.hub = await waitHub(new RegExp(`relay: session ${board}/\\d+ open \\(member\\)`), "a member session to the board", hubFrom);
      seen.board = (await hold.console.waitFor(/\[relay\] route \d+: link \S+, secure session opening/, { from, what: "a relay route's secure session" })).trim();
      // The SAME device: the card keyed by ITS MAC (the hook) is the one
      // back on the relay link, and ready.
      const line = await readyOnRelay();
      const ref = await driver.waitCard({ board: mac, timeoutMs: STEP_MS });
      const card = await cardLines();
      seen.card = card;
      const shown = macOfPath(ref);
      if (shown !== mac) throw new Error(`the relay card is ${ref} (${shown}), not ${A}'s ${mac}`);
      // No other link named: the connection bar says neither Bluetooth nor
      // USB, and its details hold no USB line (drawn only on a USB link).
      for (const wrong of [WORDS.bluetooth, WORDS.usb]) {
        if (line.includes(wrong)) throw new Error(`the relay card's connection bar says ${JSON.stringify(wrong)}: ${line}`);
      }
      const usbLine = await driver.detailsFact("connection", WORDS.usb, { board: mac, timeoutMs: STEP_MS });
      if (usbLine !== null) throw new Error(`the relay card's connection details have a USB line: ${JSON.stringify(usbLine)}`);
      // The same device: no card left under Offline boards for it (it was the
      // only remembered board, so the whole section is gone).
      const offline = await driver.evaluate(`Boolean(document.querySelector('#home-offline-boards'))`);
      if (offline) throw new Error(`Offline boards is still drawn after the merge: ${await driver.evaluate(MAIN_TEXT)}`);
      return { hub: seen.hub, board: seen.board, line: card.connection };
    }, { fatal: true });

    await step("T4", `${A} off the relay: the tile says "${WORDS.offline}"; back on, a press brings it back`, async (seen) => {
      const from = hold.console.mark();
      cloud.refuseDeviceLeg(true);
      seen.cut = cloud.cutDeviceLeg();
      // The card leaves Online boards for Offline boards: ITS card, there.
      await openRemembered();
      seen.hubEnded = hubSince(0).split("\n").filter((line) => line.includes(board) && /ended|refused/.test(line)).slice(-2);
      const hubFrom = cloud.log().length;
      seen.pressed = await pressConnectRelay();
      // The refusal is the connect's failed work on the connection bar,
      // striped, in core's words (`connection_bar.rs`, `failed_connect`).
      seen.said = await driver.waitBar("connection", WORDS.offline, { board: mac, timeoutMs: STEP_MS });
      seen.work = await driver.waitWork("connection", "failed", { board: mac, timeoutMs: STEP_MS });
      seen.hub = hubSince(hubFrom).split("\n").filter((line) => line.includes(board)).slice(-3);
      await driver.screenshot(path.join(shots, "studio-relay-T4-offline.png"));

      cloud.refuseDeviceLeg(false);
      seen.back = (await hold.console.waitFor(/\[relay\] leg open to \S+/, { from, timeoutMs: JOIN_MS, what: "it reached the relay again" })).trim();
      // Pressed again (the failed work's Retry is the same offer): the card
      // back on the relay, ready.
      seen.pressedAgain = await pressConnectRelay();
      await readyOnRelay();
      return { cut: seen.cut, said: seen.said, back: seen.back };
    });
  } catch (error) {
    fatal = error;
  }

  report.configuration = configuration;
  writeFileSync(path.join(out, "walk-wifi-emu-studio-relay.json"), JSON.stringify(report, null, 2));
  writeFileSync(path.join(out, "relay-cloud.log"), cloud.log());
  console.log("\n=== the studio-relay walk, step by step");
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${s.id}  ${s.describe}`);
  console.log(`  configuration ${configuration} through a local relay · lp-emu ${commit}`);
  console.log(`  summary → ${path.join(out, "walk-wifi-emu-studio-relay.json")}`);
  console.log(`  console → ${consoleDir}`);

  if (!options.keepOpen) {
    if (driver) await driver.close();
    if (hold) await releaseConsole(hold);
    await stopDoor(door);
    cloud.stop();
    server.close();
  }
  if (fatal) {
    console.error(`\nThe studio-relay walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  const failed = report.steps.filter((s) => !s.ok);
  if (failed.length) {
    console.error(`\nThe studio-relay walk ran to the end, but ${failed.map((s) => s.id).join(", ")} failed.`);
    process.exit(1);
  }
  console.log(`\n✓ the studio-relay walk finished T1–T4 (${configuration}, lp-emu ${commit}).`);
}

await main();
