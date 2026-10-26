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
//   T3  signed in again, still no cable: the account's list (`ListAccountBoards`)
//       says the board is online at lightplayer.app, so its card stands
//       under Online boards with its picture from lightplayer.app; it
//       offers `connect-relay`
//       (its primary, or in its link row's details when Connect over Wi‑Fi
//       leads, `primary_action.rs`); pressed, the hub opens a member session
//       to the board, the BOARD says a relay route's secure session opened,
//       and the SAME device (its card, keyed by its MAC) comes back ready on
//       the relay: its link row reads "Wi‑Fi via lightplayer.app" and
//       names neither Bluetooth nor USB, and its link row's details hold no
//       USB line
//   T4  the board cannot reach the relay (the walk drops its device leg and
//       refuses new ones): the page's session ends, the card is a card under
//       Offline boards again, and a press says "The board isn't online." on
//       its link row (the connect's failed work; the hub's 4404); once
//       the board is back on the relay (its own `[relay] leg open`), a press
//       brings the card back ready
//   T5  a SECOND browser that never met the board (a fresh profile, as on
//       another computer), signed in: the board is a card under Online
//       boards (`mac-<hex>`, from the account's list alone), its
//       lightplayer.app row says the account and "online", its picture is
//       lightplayer.app's, and Connect through lightplayer.app (its
//       primary) brings it up ready on the relay, the board saying a relay
//       route's secure session opened (your boards follow you, M1)
//
// T1–T5 run with the page's LAN shim (`walk-lan-shim.mjs`) REFUSING the
// board's LAN address, as Chrome does to a page without the Local Network
// grant: the session through lightplayer.app tries to move onto Wi‑Fi once,
// fails, and stays (T3 and T5 read that: one `Staying on lightplayer.app`
// line, no LAN link on the board). Then (best link in one click, M3):
//
//   T6  the move: a fresh page, signed in, the list saying the board is on
//       ANOTHER network (the walk's forwarder gives the page another public
//       address); Connect through lightplayer.app, Edit, the project in the
//       editor; then the page is "home" (its address is the board's again)
//       and the shim FORWARDS: at the list's next answer the session moves —
//       the board says a LAN link took the network link over (the same key)
//       and its relay route closed; the editor never shows "Reconnecting…";
//       the Studio log says `Moved to Wi‑Fi on your network` once; the hub
//       opens no new browser leg for 15 s (wall clock: an absence check)
//   T7  the way back: the shim drops the forwarded socket (1006) and refuses
//       redials: the relay route reopens on the board after core's 5 s
//       (quoted from the Studio log's own clock); a fresh page moved again,
//       dropped with 1000 (another connection took the slot): no relay route
//       for 8 s, and the card says someone else is connected
//   T8  best link: a fresh profile that never met the board, the shim
//       forwarding: the card's primary is Connect over Wi‑Fi; two polls of
//       the list later neither the board nor the hub has a session (watching
//       steps aside); pressed, the board opens a LAN link and the hub no
//       browser leg. Another fresh profile, the shim refusing: the same press
//       goes on through lightplayer.app, with one `Couldn't reach … over
//       Wi‑Fi; trying lightplayer.app.` line
//
// The account's list (your boards follow you, M4; `studio-relay-account`):
// T1, then a second board, `c6-b`, on the same door with NO Wi‑Fi (no LAN,
// no network ever given):
//
//   T9  c6-b answers `relay noAccount` over its USB door; Studio, signed in
//       (the first profile), meets it over USB, which puts the account's key
//       on it and reports it (`ReportBoard`): lightplayer.app's list holds it,
//       seen over USB and never at the relay; the board then answers
//       `relay waitingForInternet` (it holds an account, has no network) and
//       the hub never had it online (AC3)
//   T10 the power goes off (the door that runs both boards stops): the hub
//       says c6-a went offline; a fresh profile that never met either board,
//       signed in, shows BOTH under Offline boards, from the account's list
//       alone, with what the account last knew: the name, "last seen" (c6-a's
//       within one 30 s poll of the power going), the firmware, the project,
//       and c6-a's last picture from lightplayer.app; no Connect on either
//       (AC2, AC3)
//   T11 Remove from your account on c6-b, in that profile: the first click
//       arms it (still on the list, still drawn), the second removes it: the
//       card leaves at once, lightplayer.app's list no longer holds it, and
//       another fresh profile shows c6-a under Offline boards and no c6-b
//       (Forget)
//
// T10 and T11 have no board to hear from: the hub's log and lightplayer.app's
// own list (`ListAccountBoards`, asked by the walk with the account's
// session) are the words there. ⚠️ The power cut is the door's process
// exiting, which closes the board's TCP leg at once: the hub hears the leg
// end there and then, where a real power cut leaves it to its own timeout.
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
// a desk walk's). A loopback http page never sees Chrome's Local Network
// prompt: the shim stands in for the LAN's reachability. Nothing here says
// anything about Mac or Android Chrome's prompt.
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
  forwardOf,
  holdConsole,
  lpCli,
  lpEmuCommit,
  releaseConsole,
  stationOf,
} from "./walk-wifi-emu-lan.mjs";
import { CLOUD_API_VERSION, WALK_EMAIL, forwardHttp, forwardUpgrade, relayId, startRelayCloud } from "./walk-ota-relay.mjs";
import { dropLanShim, installLanShim, lanShimDials, setLanShim } from "./walk-lan-shim.mjs";

/// The public address the walk's forwarder gives the page while it is "away"
/// (`Fly-Client-IP`, which the local lp-cloud-server reads as fly's proxy
/// would set it): the board registered from loopback, so the account's list
/// says another network (`sameNetwork: false`).
const AWAY_IP = "203.0.113.9";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const LP_CLI = path.join(ROOT, "target/debug/lp-cli");
const LP_CLOUD_SERVER = path.join(ROOT, "target/debug/lp-cloud-server");
const A = "c6-a";
/// The account lane's second board: on the door, never on a network.
const B = "c6-b";
const JOIN_MS = 420_000;

/// Core's words (`relay_connect_op.rs`, `relay_connect_failure.rs`,
/// `ui_link_kind.rs`, `board_card/link_bar.rs`). The connect and the
/// link line are what this PR adds; the board's words say what happened.
const WORDS = {
  // `connect-relay`'s own words, where the link row's details draw it; as
  // the offline card's primary it reads "Connect". Pressed by its offer.
  connectRelay: "Connect through lightplayer.app",
  // The lightplayer.app row on a relay link: the row's kind
  // (`UiLinkKind::Relay.label()`), the account, then "connected", this
  // tab's link (`cloud_bar.rs`).
  relayLine: /^lightplayer\.app .*(?<!not )connected$/,
  // `RELAY_OFFLINE_WORDS`: a relay connect the hub refused (4404), the
  // lightplayer.app row's failed work's reason, first in its details.
  offline: "The board isn't online.",
  // The USB row of a board reached through the relay: not its link.
  usbRow: "USB not connected",
  // The lightplayer.app row of a board online at lightplayer.app that no
  // link of the tab reaches (`cloud_bar.rs`): the account, then "online".
  onlineLine: /^lightplayer\.app .*online$/,
};

function usage() {
  return "usage: node scripts/emu/walk-wifi-emu.mjs <studio-relay|studio-relay-move|studio-relay-account> [--out <dir>] [--keep-open] [--dry-run]";
}

function parseArgs(argv) {
  // Three lanes over one set of helpers, each under ten minutes:
  // `studio-relay` (T1–T5), `studio-relay-move` (T1, then the move's T6–T8)
  // and `studio-relay-account` (T1, then the account's list: T9–T11, with a
  // second board).
  const lanes = ["studio-relay", "studio-relay-move", "studio-relay-account"];
  const lane = lanes.includes(argv[0]) ? argv[0] : "studio-relay";
  const rest = lanes.includes(argv[0]) ? argv.slice(1) : argv;
  const move = lane === "studio-relay-move";
  const account = lane === "studio-relay-account";
  const options = { lane, move, account, out: path.join(ROOT, `target/walk-wifi-emu/${lane}`), keepOpen: false, dryRun: false };
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

/// A firmware version as Studio words it (`firmware_version.rs`): a
/// release `v2026.10.16-1`, a local build `dev c688c99`, from the version a
/// record holds, with or without the hello's target (`fw-esp32c6 …`);
/// `null` when nothing in it reads as a version.
function firmwareWords(version) {
  for (const word of String(version ?? "").split(/\s+/)) {
    const bare = word.replace(/^v/, "");
    if (/^\d{4}\.\d{2}\.\d{2}-\d+$/.test(bare)) return `v${bare}`;
    if (/^[0-9a-f]{7,40}$/.test(bare)) return `dev ${bare.slice(0, 7)}`;
  }
  return null;
}

/// The bars of a board card, by their `data-bar` layer.
const BARS = ["project", "usb", "wifi", "bluetooth", "cloud", "access", "firmware", "hardware"];

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
  // No `lan=`: c6-b has no network to join, ever (T9).
  const boardSpecB = `${B}={merged},kind=rom-up`;
  const boardSpecs = options.account ? [boardSpec, boardSpecB] : [boardSpec];
  const needs = prerequisites();
  const chrome = findChrome();

  if (options.dryRun) {
    const steps = [
      `T1 lp-cli wifi add <usb door> ${NET.ssid} → status connected, relay noAccount; Studio signed in, ?emu= → the USB square → ${A}'s card ready (push or edit offered) → status relay connected`,
      `T2 no cookie, no ?emu= → ${A}'s card under Offline boards, and no connect-relay ("${WORDS.connectRelay}") on it, face or link row's details`,
      `T3 signed in, no ?emu= → ${A}'s card under Online boards (lightplayer.app … online) → connect-relay → hub: member session open; board: [relay] route … secure session opening → its card (${A}'s MAC) ready, lightplayer.app row "lightplayer.app … connected", one device`,
      `T4 the device leg cut and refused → card offline → connect-relay → link row "${WORDS.offline}" (failed); leg allowed → board: [relay] leg open → press → ready`,
      `T5 a fresh profile, signed in, never met ${A} → its card under Online boards (data-picture=cloud, lightplayer.app … online) → connect-relay → board: [relay] route … secure session opening → ready; the LAN shim refuses: one "Staying on lightplayer.app" line`,
      `T6 a fresh page, "away" (another public address), shim forward → connect-relay → edit → the editor → "home" → board: [lan] link … took the network link over, [relay] route … closed; never [data-reconnecting]; one "Moved to Wi‑Fi on your network"; the hub opens no browser leg for 15 s`,
      `T7 shim drop(1006), refuse → board: [relay] route … secure session opening, "Wi‑Fi dropped; back through lightplayer.app" within 5 s of core's own loss line; a fresh page moved again → drop(1000) → no relay route for 8 s, the card says someone else`,
      `T8 a fresh never-met profile, shim forward → primary connect-wifi; two list polls, no session on board or hub → press → board: [lan] link … secure session opening, no browser leg; a fresh profile, shim refuse → press → board: [relay] route …, one "Couldn't reach … trying lightplayer.app." line`,
      `T9 ${B} (no network) status relay noAccount → Studio signed in, ?emu= → USB → ${B}'s card ready → ListAccountBoards holds ${B} (seen usb, never relay) → status relay waitingForInternet, station not connected; the hub never had ${B} online`,
      `T10 the door stops (both boards' power) → hub: relay: board <${A}> offline → a fresh never-met profile, signed in → ${A} and ${B} under Offline boards: name, "last seen" (${A}'s within 30 s of the power going), firmware, project, ${A}'s picture data-picture=cloud; no connect offer on either`,
      `T11 forget-from-account on ${B} (hardware details): click 1 → still listed and drawn; click 2 → the card goes, ListAccountBoards drops ${B}; a fresh profile → ${A} under Offline boards, no ${B}`,
    ];
    const laneSteps = steps.filter(
      (line) =>
        line.startsWith("T1 ") || (options.move ? /^T[678] /.test(line) : options.account ? /^T(9|10|11) /.test(line) : /^T[2-5] /.test(line)),
    );
    steps.length = 0;
    steps.push(...laneSteps);
    writeFileSync(path.join(out, "walk-plan.json"), JSON.stringify({ out, lane: options.lane, boards: boardSpecs, steps, prerequisites: needs, chrome }, null, 2));
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
  /// The page is "away" (T6): its requests reach the cloud from another
  /// public address, so the list says the board is on another network.
  let away = false;
  const route = (request, response, url) => {
    if (cloud && /^\/(api|auth|relay)(\/|$)/.test(url.pathname)) {
      if (away) request.headers["fly-client-ip"] = AWAY_IP;
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
    boards: boardSpecs,
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

  const report = { lane: options.lane, lpEmu: commit, door: door.addr, board: { mac, relayId: board }, steps: [] };
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
  /// The page's console, with each line's time on the page's own clock
  /// (`Runtime.consoleAPICalled`'s timestamp): the Studio log is mirrored
  /// there, stamped by core's injected clock (`Date.now()`).
  let pageLog = [];
  /// Every page's console, kept for the record (`page-console.log`).
  const pageLogAll = [];
  /// A Chrome with a fresh profile, the LAN shim in front of its pages
  /// (refusing until a step says otherwise), its console kept with times.
  const launch = async () => {
    const launched = await StudioDriver.launch({ width: 1100, height: 900 });
    await launched.cdp.send("Emulation.setDeviceMetricsOverride", { width: 1100, height: 900, deviceScaleFactor: 2, mobile: false }, launched.sessionId);
    await installLanShim(launched);
    pageLog = [];
    launched.cdp.on("Runtime.consoleAPICalled", (params, sid) => {
      if (sid !== launched.sessionId) return;
      const text = (params.args ?? []).map((arg) => arg.value ?? arg.description ?? arg.type).join(" ");
      pageLog.push({ t: params.timestamp, text });
      pageLogAll.push(`${new Date(params.timestamp).toISOString()} [${params.type}] ${text}`);
    });
    return launched;
  };
  /// The page console's lines (from `from` on) that contain `words`.
  const said = (words, from = 0) => pageLog.slice(from).filter((line) => line.text.includes(words));
  /// Wait for the page console to say `words` (from `from` on); its line.
  const waitSaid = async (words, { from = 0, timeoutMs = STEP_MS } = {}) => {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const hit = said(words, from)[0];
      if (hit) return hit;
      if (Date.now() > deadline) throw new Error(`the Studio log never said ${JSON.stringify(words)}`);
      await delay(250);
    }
  };
  /// The board's LAN address as the page dials it is the shim's; its
  /// forward, the door's.
  const forward = async () => forwardOf((await boardRegistry(door.addr)).find((b) => b.id === A));
  /// Watch the page for the editor's "Reconnecting…" curtain, however
  /// briefly (a MutationObserver), and sample it every 100 ms as well.
  const watchCurtain = async () => {
    await driver.evaluate(`(() => {
      window.__lpWalkCurtain = Boolean(document.querySelector('[data-reconnecting="true"]'));
      window.__lpWalkCurtainObserver?.disconnect();
      window.__lpWalkCurtainObserver = new MutationObserver(() => {
        if (document.querySelector('[data-reconnecting="true"]')) window.__lpWalkCurtain = true;
      });
      window.__lpWalkCurtainObserver.observe(document.body, { subtree: true, childList: true, attributes: true, attributeFilter: ['data-reconnecting'] });
      clearInterval(window.__lpWalkCurtainSampler);
      window.__lpWalkCurtainSamples = 0;
      window.__lpWalkCurtainSampler = setInterval(() => {
        window.__lpWalkCurtainSamples += 1;
        if (document.querySelector('[data-reconnecting="true"]') || /Reconnecting/.test(document.body.innerText)) window.__lpWalkCurtain = true;
      }, 100);
    })()`);
  };
  const curtainSeen = async () =>
    driver.evaluate(`({ seen: window.__lpWalkCurtain === true, samples: window.__lpWalkCurtainSamples ?? 0 })`);
  /// Members sessions the hub opened for the board from `from` on.
  const browserLegs = (from) => (hubSince(from).match(new RegExp(`relay: session ${board}/\\d+ open \\(member\\)`, "g")) ?? []).length;
  /// A fresh page of the current profile, signed in, the list "away" or not,
  /// connected through lightplayer.app; returns once it is ready there.
  const relaySession = async ({ awayFirst }) => {
    away = awayFirst;
    await signIn(true);
    await load(plainUrl);
    await openOnline();
    await pressConnectRelay();
    await readyOnRelay();
  };
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
  /// The board's card under Online boards: a board the account's list says
  /// is online at lightplayer.app, that no link here reaches.
  const openOnline = async () => {
    await driver.waitFor(
      `Boolean(document.querySelector(${JSON.stringify(`#home-online-boards [data-board-card="${boardPath(mac)}"]`)}))`,
      { timeoutMs: STEP_MS, what: "the board's card under Online boards" },
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
  /// Press `connect-relay` where core put it on the offline card: on its
  /// face when it is the card's way back (a failed connect's Retry, on the
  /// link row's face, is the same offer), else in the connection
  /// details — the primary is Connect (`connect`, one meaning since the
  /// connected card: it reaches the board Wi‑Fi first, then the cloud, then
  /// the cable, and opens its panel), or, for a board with no registry row,
  /// the first of the roads' own offers (`primary_action.rs`). This walk
  /// takes the cloud's road by its own offer, which reaches the board and
  /// opens nothing. Waits for the card to show its way back; returns the
  /// pressed button's words.
  const pressConnectRelay = async () => {
    // Where it is drawn can move while the page settles (the face's
    // primary is `connect-relay` until the library names the board and
    // `connect` takes its place), so look again on every try: the face
    // first, then the lightplayer.app row's details.
    const deadline = Date.now() + STEP_MS;
    for (;;) {
      if (await driver.offered("connect-relay", { board: mac, enabled: true })) {
        try {
          return `${await driver.pressOffer("connect-relay", { board: mac, timeoutMs: 5_000 })} (the card's face)`;
        } catch {
          // it moved: look again
        }
      } else {
        await driver.openBar("cloud", { board: mac, timeoutMs: STEP_MS }).catch(() => {});
        if (await driver.offered("connect-relay", { board: mac, enabled: true, inDetails: true })) {
          try {
            const pressed = await driver.pressOffer("connect-relay", { board: mac, bar: "cloud", timeoutMs: 5_000 });
            await driver.closeDetails({ board: mac }).catch(() => {});
            return `${pressed} (the link row's details)`;
          } catch {
            // it moved: look again
          }
        }
        await driver.closeDetails({ board: mac }).catch(() => {});
      }
      if (Date.now() > deadline) {
        const verbs = await cardVerbs().catch(() => null);
        throw new Error(`the card never offered \`connect-relay\` (enabled): its verbs ${JSON.stringify(verbs)}`);
      }
      await delay(1_000);
    }
  };
  /// One press connects (defect
  /// `2026-10-25-a-relay-connect-looked-like-it-needed-two-presses`): every
  /// 25 ms, page-side, from just before the press until
  /// `stopConnectTimeline`, what the board's card says (`data-surface`),
  /// whether its face draws a Connect that can be pressed (`connect`,
  /// `connect-relay`, `connect-wifi`), and how many board cards the page
  /// draws; and every click on one of them, wherever it is drawn. A gap
  /// where the card offered its Connect again, or a second card stood
  /// beside it, is caught even if it lasted a moment.
  const startConnectTimeline = async () => {
    const scope = await driver.card({ board: mac });
    await driver.evaluate(`(() => {
      clearInterval(window.__lpConnectTimer);
      const t0 = performance.now();
      const timeline = { t0, presses: [], samples: [] };
      window.__lpConnectTimeline = timeline;
      const CONNECTS = '[data-offer-path$="/connect-relay"], [data-offer-path$="/connect"], [data-offer-path$="/connect-wifi"]';
      const isConnect = (el) => el?.closest?.(CONNECTS);
      window.__lpConnectClick = (event) => { if (isConnect(event.target)) timeline.presses.push(Math.round(performance.now() - t0)); };
      document.addEventListener('click', window.__lpConnectClick, true);
      const sample = () => {
        const card = ${scope};
        const enabled = card ? [...card.querySelectorAll(CONNECTS)]
          .filter((mark) => !mark.closest('.ux-popover-layer'))
          .some((mark) => { const all = mark.querySelectorAll('button'); const button = all[all.length - 1]; return button && !button.disabled; }) : false;
        const now = { surface: card?.querySelector('[data-surface]')?.getAttribute('data-surface') ?? (card ? 'none' : 'no-card'),
                      connect: enabled, cards: document.querySelectorAll('[data-board-card]').length };
        const last = timeline.samples[timeline.samples.length - 1];
        if (!last || last.surface !== now.surface || last.connect !== now.connect || last.cards !== now.cards) {
          timeline.samples.push({ t: Math.round(performance.now() - t0), ...now });
        }
      };
      sample();
      window.__lpConnectTimer = setInterval(sample, 25);
    })()`);
  };
  /// Stop the timeline and return it: `{ presses, samples }`, times in ms
  /// from its start.
  const stopConnectTimeline = async () =>
    driver.evaluate(`(() => {
      clearInterval(window.__lpConnectTimer);
      document.removeEventListener('click', window.__lpConnectClick, true);
      const { presses, samples } = window.__lpConnectTimeline;
      return { presses, samples };
    })()`);
  /// Judge a timeline: one press; the card says "connecting" within a
  /// second of it; and from then until the card says something else (the
  /// board's hello), it draws no Connect to press and no second card stands
  /// beside it — and what it says next is not the offline or online-at-
  /// lightplayer.app face whose Connect asks for the press again. Returns
  /// the numbers for the report; throws with the whole timeline otherwise.
  const judgeOnePress = (timeline) => {
    const { presses, samples } = timeline;
    const fail = (why) => { throw new Error(`${why}: ${JSON.stringify(timeline)}`); };
    if (presses.length !== 1) fail(`${presses.length} presses of Connect, not one`);
    const pressAt = presses[0];
    const before = [...samples].reverse().find((sample) => sample.t <= pressAt) ?? samples[0];
    const first = samples.findIndex((sample) => sample.t >= pressAt && sample.surface === "connecting");
    if (first < 0) fail("the card never said it was connecting");
    const connectingAfterMs = Math.max(0, samples[first].t - pressAt);
    if (connectingAfterMs > 1_000) fail(`the card said "connecting" ${connectingAfterMs} ms after the press`);
    let end = first;
    while (end + 1 < samples.length && samples[end + 1].surface === "connecting") end += 1;
    const run = samples.slice(first, end + 1);
    if (run.some((sample) => sample.connect)) fail("the card drew a Connect to press while it was connecting");
    if (run.some((sample) => sample.cards !== before.cards)) fail(`a second card stood beside the board's while it connected (${before.cards} before the press)`);
    const next = samples[end + 1];
    if (!next) fail("the card was still connecting when the board was ready");
    if (["online-at-cloud", "offline-reachable", "offline", "no-card"].includes(next.surface)) {
      fail(`after "connecting" the card said "${next.surface}", which asks for the press again`);
    }
    return { presses: presses.length, connectingAfterMs, connectingForMs: next.t - samples[first].t, then: next.surface };
  };
  /// The board's card back on the relay and ready: its link row reads
  /// the relay link, and core offers it a project or the editor. Returns the
  /// link row's line.
  const readyOnRelay = async () => {
    const line = await driver.waitBar("cloud", WORDS.relayLine, { board: mac, timeoutMs: STEP_MS });
    await driver.boardRuns({ board: mac, timeoutMs: STEP_MS });
    return line;
  };
  /// lightplayer.app's own list for the walk's account (`ListAccountBoards`,
  /// asked with the account's session): the cloud's words, not Studio's.
  const accountBoards = async () => {
    const response = await fetch(`${cloud.origin}/api`, {
      method: "POST",
      headers: { "content-type": "application/json", cookie: `lp_session=${cloud.cookie}` },
      body: JSON.stringify({ version: CLOUD_API_VERSION, request: "listAccountBoards" }),
    });
    const reply = await response.json();
    const boards = reply?.result?.Ok?.accountBoardList?.boards;
    if (!Array.isArray(boards)) throw new Error(`ListAccountBoards answered ${JSON.stringify(reply).slice(0, 300)}`);
    return boards;
  };
  /// Wait until lightplayer.app's list holds (`present`) or no longer holds
  /// the board `id`; its row, or `null`.
  const waitListed = async (id, present, what) => {
    const deadline = Date.now() + STEP_MS;
    for (;;) {
      const row = (await accountBoards()).find((b) => b.id === id) ?? null;
      if (Boolean(row) === present) return row;
      if (Date.now() > deadline) throw new Error(`lightplayer.app's list never ${what}`);
      await delay(500);
    }
  };
  /// What a board's card says it last knew: its name, the surface's two
  /// sentences, its section, each row's line, its picture, and the verbs it
  /// draws.
  const offlineFacts = async (boardMac) => {
    const scope = await driver.card({ board: boardMac });
    const facts = await driver.evaluate(`(() => { const card = ${scope}; if (!card) return null;
      const bar = card.querySelector('[data-name-bar]');
      const titled = bar?.querySelector('h3[title]') ?? bar?.querySelector('[title]');
      return {
        name: titled?.getAttribute('title') ?? (bar?.innerText || '').split('\\n')[0].trim(),
        surface: card.querySelector('[data-surface]')?.getAttribute('data-surface') ?? null,
        first: card.querySelector('[data-surface-first]')?.textContent?.trim() ?? null,
        second: card.querySelector('[data-surface-second]')?.textContent?.trim() ?? null,
        section: card.closest('#home-online-boards') ? 'online' : card.closest('#home-offline-boards') ? 'offline' : null,
      }; })()`);
    if (!facts) return null;
    facts.picture = await driver.pictureOf({ board: boardMac });
    facts.lines = {};
    for (const layer of ["project", "firmware", "cloud", "hardware"]) facts.lines[layer] = await driver.barText(layer, { board: boardMac });
    facts.verbs = await driver.evaluate(`[...new Set([...(${scope}?.querySelectorAll('[data-offer-path]') ?? [])]
      .map((mark) => mark.getAttribute('data-offer-path').split('/').pop()))]`);
    return facts;
  };
  /// Wait for a board's card under Offline boards, by its MAC.
  const waitOfflineCard = (boardMac, what) =>
    driver.waitFor(`Boolean(document.querySelector(${JSON.stringify(`#home-offline-boards [data-board-card="${boardPath(boardMac)}"]`)}))`, {
      timeoutMs: STEP_MS,
      what,
    });

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
      // A project on the board, so its pictures through lightplayer.app
      // carry colours (nothing loaded is 0 lamps: no picture to draw).
      const upload = await lpCli(["upload", path.join(ROOT, "projects/test/basic"), usb], { timeoutMs: STEP_MS });
      if (upload.code !== 0) throw new Error(`upload → exit ${upload.code}: ${upload.stderr.trim().split("\n").slice(-3).join(" | ")}`);

      driver = await launch();
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

    if (!options.move && !options.account) {
    await step("T2", `no cable, signed out: ${A}'s tile does not offer "${WORDS.connectRelay}"`, async (seen) => {
      await signIn(false);
      await load(plainUrl);
      await openRemembered();
      // Cached account keys may draw it for a moment at boot; it goes once
      // the page knows nobody is signed in, and stays gone — from the card's
      // face here, and from its link row's details below, where it stands
      // when Connect over Wi‑Fi leads.
      const drawn = await relayDrawn();
      await driver.waitFor(`!${drawn}`, {
        timeoutMs: STEP_MS,
        what: "no relay connect while signed out",
      });
      await delay(8_000);
      await driver.openBar("cloud", { board: mac, timeoutMs: STEP_MS });
      const still = await driver.evaluate(drawn);
      seen.tile = await cardVerbs();
      await driver.closeDetails({ board: mac });
      if (still) throw new Error(`signed out, the card still offers \`connect-relay\`: its verbs ${JSON.stringify(seen.tile)}`);
      return seen.tile;
    });

    await step("T3", `signed in, no cable: ${A} is online at lightplayer.app; "${WORDS.connectRelay}" brings it back as the same device`, async (seen) => {
      const logFrom = pageLog.length;
      await signIn(true);
      await load(plainUrl);
      await openOnline();
      seen.online = await driver.waitBar("cloud", WORDS.onlineLine, { board: mac, timeoutMs: STEP_MS });
      const hubFrom = cloud.log().length;
      const from = hold.console.mark();
      await startConnectTimeline();
      seen.pressed = await pressConnectRelay();
      seen.hub = await waitHub(new RegExp(`relay: session ${board}/\\d+ open \\(member\\)`), "a member session to the board", hubFrom);
      seen.board = (await hold.console.waitFor(/\[relay\] route \d+: link \S+, secure session opening/, { from, what: "a relay route's secure session" })).trim();
      // The SAME device: the card keyed by ITS MAC (the hook) is the one
      // back on the relay link, and ready.
      const line = await readyOnRelay();
      // One press did it, and the card said so the whole way.
      seen.onePress = judgeOnePress(await stopConnectTimeline());
      console.log(`  one press: "connecting" ${seen.onePress.connectingAfterMs} ms after it, for ${seen.onePress.connectingForMs} ms, then "${seen.onePress.then}"`);
      const ref = await driver.waitCard({ board: mac, timeoutMs: STEP_MS });
      const card = await cardLines();
      seen.card = card;
      const shown = macOfPath(ref);
      if (shown !== mac) throw new Error(`the relay card is ${ref} (${shown}), not ${A}'s ${mac}`);
      // No other link named: the lightplayer.app row is the link in use,
      // and the USB row says only its own fact.
      if (card.usb !== WORDS.usbRow) throw new Error(`the relay card's USB row says ${JSON.stringify(card.usb)}, not ${JSON.stringify(WORDS.usbRow)}`);
      // The same device: no card left under Offline boards for it (it was
      // the only remembered board, and it was online all along).
      const offline = await driver.evaluate(`Boolean(document.querySelector('#home-offline-boards'))`);
      if (offline) throw new Error(`Offline boards is still drawn after the merge: ${await driver.evaluate(MAIN_TEXT)}`);
      // The session through lightplayer.app tries Wi‑Fi once (the list says
      // the board shares this network) and the shim refuses it, as Chrome
      // does without the Local Network grant: one line, the card still on
      // lightplayer.app, no LAN link on the board.
      const stayed = await waitSaid("Staying on lightplayer.app", { from: logFrom });
      await delay(3_000);
      const stays = said("Staying on lightplayer.app", logFrom).length;
      if (stays !== 1) throw new Error(`${stays} "Staying on lightplayer.app" lines, not one`);
      const lanLinks = hold.console.since(from).split("\n").filter((l) => l.includes("[lan] link"));
      if (lanLinks.length) throw new Error(`the board reported a LAN link while the shim refused: ${lanLinks[0]}`);
      await driver.waitBar("cloud", WORDS.relayLine, { board: mac, timeoutMs: STEP_MS });
      seen.stayed = stayed.text;
      seen.shim = await lanShimDials(driver);
      return { hub: seen.hub, board: seen.board, line, stayed: seen.stayed, onePress: seen.onePress };
    }, { fatal: true });

    await step("T4", `${A} off the relay: the tile says "${WORDS.offline}"; back on, a press brings it back`, async (seen) => {
      const from = hold.console.mark();
      cloud.refuseDeviceLeg(true);
      seen.cut = cloud.cutDeviceLeg();
      // The card leaves Online boards for Offline boards: ITS card, there,
      // once the account's list (asked every 30 s) no longer names it.
      await openRemembered();
      seen.hubEnded = hubSince(0).split("\n").filter((line) => line.includes(board) && /ended|refused/.test(line)).slice(-2);
      const hubFrom = cloud.log().length;
      seen.pressed = await pressConnectRelay();
      // The refusal is the connect's failed work on the link row,
      // striped, in core's words (`link_bar.rs`, `failed_connect`).
      seen.work = await driver.waitWork("cloud", "failed", { board: mac, timeoutMs: STEP_MS });
      await driver.openBar("cloud", { board: mac, timeoutMs: STEP_MS });
      seen.said = await driver.waitFor(
        `(() => { const t = (${await driver.card({ board: mac })}?.querySelector('[data-bar="cloud"] [id^="ux-popover-panel"]')?.innerText || '');
                  return t.includes(${JSON.stringify(WORDS.offline)}) ? ${JSON.stringify(WORDS.offline)} : false; })()`,
        { timeoutMs: STEP_MS, what: "the lightplayer.app row's details to say why" },
      );
      await driver.closeDetails({ board: mac });
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

    await step("T5", `a browser that never met ${A}, signed in: its card is in Online boards, and connects through lightplayer.app`, async (seen) => {
      // The first browser lets go of the board (its one network slot), and
      // a fresh profile — another computer, as far as Studio knows — opens.
      await driver.close();
      driver = await launch();
      const logFrom = 0;
      await signIn(true);
      // AC1's number: from Studio loaded to the board's card under Online
      // boards, which must be within one poll of the list (30 s).
      const navigatedAt = Date.now();
      await load(plainUrl);
      const loadedAt = Date.now();
      await openOnline();
      seen.seenAfterMs = Date.now() - loadedAt;
      seen.loadMs = loadedAt - navigatedAt;
      if (seen.seenAfterMs > 30_000) throw new Error(`the card took ${seen.seenAfterMs} ms, more than one 30 s poll`);
      console.log(`  AC1: a profile that never met ${A} saw it under Online boards ${seen.seenAfterMs} ms after Studio loaded (${seen.loadMs} ms to load)`);
      seen.online = await driver.waitBar("cloud", WORDS.onlineLine, { board: mac, timeoutMs: STEP_MS });
      seen.picture = await driver.waitFor(
        `${await driver.card({ board: mac })}?.querySelector('[data-picture="cloud"][data-picture-frame="true"]') ? 'cloud' : false`,
        { timeoutMs: STEP_MS, what: "the card to draw lightplayer.app's picture" },
      );
      // Lit, not just present: the picture's canvas has a pixel that is not
      // black (the board plays Basic, whose lamps are never all off).
      seen.lit = await driver.waitFor(
        `(() => { const canvas = ${await driver.card({ board: mac })}?.querySelector('[data-picture="cloud"] canvas');
                  if (!canvas || !canvas.width || !canvas.height) return false;
                  const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
                  let lit = 0;
                  for (let i = 0; i < data.length; i += 4) if (data[i + 3] > 0 && data[i] + data[i + 1] + data[i + 2] > 60) lit += 1;
                  return lit > 0 ? lit : false; })()`,
        { timeoutMs: STEP_MS, what: "lightplayer.app's picture to light some lamps" },
      );
      await driver.screenshot(path.join(shots, "studio-relay-T5-never-met.png"));
      const hubFrom = cloud.log().length;
      const from = hold.console.mark();
      await startConnectTimeline();
      seen.pressed = await pressConnectRelay();
      seen.hub = await waitHub(new RegExp(`relay: session ${board}/\\d+ open \\(member\\)`), "a member session to the board", hubFrom);
      seen.board = (await hold.console.waitFor(/\[relay\] route \d+: link \S+, secure session opening/, { from, what: "a relay route's secure session" })).trim();
      const line = await readyOnRelay();
      // One press did it, on the card the person pressed, the whole way.
      seen.onePress = judgeOnePress(await stopConnectTimeline());
      console.log(`  one press: "connecting" ${seen.onePress.connectingAfterMs} ms after it, for ${seen.onePress.connectingForMs} ms, then "${seen.onePress.then}"`);
      // The session through lightplayer.app tries Wi‑Fi once (the list says
      // the board shares this network) and the shim refuses it, as Chrome
      // does without the Local Network grant: one line, the card still on
      // lightplayer.app, no LAN link on the board.
      const stayed = await waitSaid("Staying on lightplayer.app", { from: logFrom });
      await delay(3_000);
      const stays = said("Staying on lightplayer.app", logFrom).length;
      if (stays !== 1) throw new Error(`${stays} "Staying on lightplayer.app" lines, not one`);
      const lanLinks = hold.console.since(from).split("\n").filter((l) => l.includes("[lan] link"));
      if (lanLinks.length) throw new Error(`the board reported a LAN link while the shim refused: ${lanLinks[0]}`);
      await driver.waitBar("cloud", WORDS.relayLine, { board: mac, timeoutMs: STEP_MS });
      seen.stayed = stayed.text;
      seen.shim = await lanShimDials(driver);
      return { seenAfterMs: seen.seenAfterMs, loadMs: seen.loadMs, online: seen.online, picture: seen.picture, litPixels: seen.lit, hub: seen.hub, board: seen.board, line, stayed: seen.stayed, onePress: seen.onePress };
    });

    }
    if (options.move) {
    /// The board's project open in the editor (its address and its panel).
    const waitEditor = () =>
      driver.waitFor(
        `(location.pathname.startsWith('/p/') || location.pathname.startsWith('/device/'))
          && Boolean(document.querySelector('#main [data-panel-channel]'))`,
        { timeoutMs: STEP_MS, what: "the board's project open in the editor" },
      );
    /// A fresh profile's page, signed in, through lightplayer.app, the list
    /// saying the board is on another network until `home()` is called.
    const awayOnRelay = async () => {
      await driver.close();
      driver = await launch();
      away = true;
      await signIn(true);
      await load(plainUrl);
      await setLanShim(driver, "forward", await forward());
      await openOnline();
      const from = hold.console.mark();
      await pressConnectRelay();
      await readyOnRelay();
      return (await hold.console.waitFor(/\[relay\] route \d+: link \S+, secure session opening/, { from, what: "a relay route" })).trim();
    };
    /// The page comes home: the list's next answer says the board shares
    /// this network, and the session moves. The board's words and the
    /// Studio log's line.
    const home = async () => {
      const logFrom = pageLog.length;
      const from = hold.console.mark();
      away = false;
      const moved = await waitSaid("Moved to Wi‑Fi on your network", { from: logFrom, timeoutMs: 120_000 });
      const lan = (await hold.console.waitFor(/\[lan\] link \S+.*took the network link over/, { from, what: "a LAN link taking the network link over" })).trim();
      const relayClosed = (await hold.console.waitFor(/\[relay\] route \d+: closed/, { from, what: "its relay route closing" })).trim();
      return { logFrom, moved, lan, relayClosed };
    };

    await step("T6", `the move: ${A}'s session through lightplayer.app moves onto Wi‑Fi with the editor open, never "Reconnecting…"`, async (seen) => {
      seen.relay = await awayOnRelay();
      await driver.pressOffer("edit", { board: mac, timeoutMs: STEP_MS });
      await waitEditor();
      if (said("Moved to Wi‑Fi").length) throw new Error("the session moved while the list said another network");
      await watchCurtain();
      const hubFrom = cloud.log().length;
      const homeAt = Date.now();
      const moved = await home();
      seen.lan = moved.lan;
      seen.relayClosed = moved.relayClosed;
      seen.movedAfterMs = moved.moved.t - homeAt;
      await delay(2_000);
      seen.curtain = await curtainSeen();
      if (seen.curtain.seen) throw new Error(`the editor showed "Reconnecting…" during the move (${seen.curtain.samples} samples)`);
      if (!(await driver.evaluate(`Boolean(document.querySelector('#main [data-panel-channel]'))`))) throw new Error("the editor went away");
      // Wall clock, an absence check: the relay is not redialled.
      await delay(15_000);
      seen.browserLegs = browserLegs(hubFrom);
      if (seen.browserLegs) throw new Error(`the hub opened ${seen.browserLegs} browser leg(s) after the move`);
      seen.moves = said("Moved to Wi‑Fi on your network", moved.logFrom).length;
      if (seen.moves !== 1) throw new Error(`${seen.moves} "Moved to Wi‑Fi" lines, not one`);
      await driver.screenshot(path.join(shots, "studio-relay-T6-editor-after-move.png"));
      // The card, home: its session ends (Done), its Wi‑Fi row is this tab's link.
      const how = await driver.evaluate(`(() => {
        const a = [...document.querySelectorAll('a[href]')].find((l) => new URL(l.href, location.href).pathname === '/');
        if (!a) return false; a.click(); return true; })()`);
      if (!how) throw new Error("no in-app way home");
      await driver.waitFor(`location.pathname === '/'`, { timeoutMs: STEP_MS, what: "the home page" });
      await driver.pressOffer("done", { board: mac, timeoutMs: STEP_MS });
      seen.wifiLine = await driver.waitBar("wifi", /(?<!not )connected$/, { board: mac, timeoutMs: STEP_MS });
      seen.cloudLine = await driver.barText("cloud", { board: mac });
      await driver.screenshot(path.join(shots, "studio-relay-T6-card-after-move.png"));
      return { lan: seen.lan, relayClosed: seen.relayClosed, movedAfterMs: seen.movedAfterMs, curtain: seen.curtain, browserLegs: seen.browserLegs, wifiLine: seen.wifiLine };
    });

    await step("T7", `the way back: Wi‑Fi drops, lightplayer.app carries ${A} again after 5 s; taken (1000), it does not`, async (seen) => {
      // The editor on the board again, now over Wi‑Fi.
      await driver.pressOffer("edit", { board: mac, timeoutMs: STEP_MS });
      await waitEditor();
      const logFrom = pageLog.length;
      const from = hold.console.mark();
      await setLanShim(driver, "refuse");
      const droppedAt = Date.now();
      seen.dropped = await dropLanShim(driver, 1006);
      if (!seen.dropped) throw new Error("no forwarded socket to drop");
      const lost = await waitSaid("the moved link went", { from: logFrom, timeoutMs: 60_000 });
      const back = await waitSaid("Wi‑Fi dropped; back through lightplayer.app", { from: logFrom, timeoutMs: 60_000 });
      seen.lossHeardMs = lost.t - droppedAt;
      seen.returnAfterMs = back.t - lost.t;
      console.log(`  T7: the drop heard ${seen.lossHeardMs} ms after it; back through lightplayer.app ${seen.returnAfterMs} ms after core saw the loss (core's clock)`);
      if (seen.returnAfterMs < 5_000 || seen.returnAfterMs > 6_000) {
        throw new Error(`the return came ${seen.returnAfterMs} ms after the loss, not at core's 5 s`);
      }
      seen.relay = (await hold.console.waitFor(/\[relay\] route \d+: link \S+, secure session opening/, { from, what: "the relay route reopened" })).trim();
      seen.resumed = (await waitSaid("the editor picks up where it left off", { from: logFrom })).text;
      await driver.screenshot(path.join(shots, "studio-relay-T7-back-on-relay.png"));

      // Moved again on a fresh page, then the board's slot is taken (1000).
      await awayOnRelay();
      await home();
      const takenFrom = hold.console.mark();
      const hubFrom = cloud.log().length;
      const logFrom2 = pageLog.length;
      await setLanShim(driver, "refuse");
      if (!(await dropLanShim(driver, 1000))) throw new Error("no forwarded socket to drop (1000)");
      await delay(8_000);
      const reopened = hold.console.since(takenFrom).split("\n").filter((l) => /\[relay\] route \d+: link \S+, secure session opening/.test(l));
      seen.takenLegs = browserLegs(hubFrom);
      if (reopened.length || seen.takenLegs) throw new Error(`after a 1000 the relay came back: ${reopened[0] ?? `${seen.takenLegs} browser leg(s)`}`);
      if (said("Wi‑Fi dropped; back through lightplayer.app", logFrom2).length) throw new Error("the page went back through lightplayer.app after a 1000");
      seen.someoneElse = await driver.waitFor(
        `(() => { const t = ${await driver.card({ board: mac })}?.innerText || ''; return t.includes('Someone else') ? t.split('\\n').find((l) => l.includes('Someone else')) : false; })()`,
        { timeoutMs: STEP_MS, what: "the card to say someone else is connected" },
      );
      await driver.screenshot(path.join(shots, "studio-relay-T7-taken.png"));
      return { lossHeardMs: seen.lossHeardMs, returnAfterMs: seen.returnAfterMs, relay: seen.relay, someoneElse: seen.someoneElse };
    });

    await step("T8", `best link: a profile that never met ${A} connects over Wi‑Fi first; watching opens nothing; Wi‑Fi refused, the same press goes through lightplayer.app`, async (seen) => {
      await driver.close();
      driver = await launch();
      away = false;
      await signIn(true);
      const from = hold.console.mark();
      const hubFrom = cloud.log().length;
      await load(plainUrl);
      await setLanShim(driver, "forward", await forward());
      await openOnline();
      const primary = async (verb) =>
        driver.evaluate(`Boolean(${await driver.card({ board: mac })}?.querySelector('[data-name-bar] [data-offer-path$="/${verb}"]'))`);
      await driver.waitFor(`Boolean(${await driver.card({ board: mac })}?.querySelector('[data-name-bar] [data-offer-path$="/connect-wifi"]'))`, {
        timeoutMs: STEP_MS,
        what: "Connect over Wi‑Fi as the card's primary",
      });
      await driver.screenshot(path.join(shots, "studio-relay-T8-wifi-first.png"));
      // Watching steps aside: two of the list's polls (30 s each) with the
      // card drawn, and no session of any kind (wall clock).
      await delay(65_000);
      const sessions = hold.console.since(from).split("\n").filter((l) => /\[lan\] link \S+.*secure session opening|\[relay\] route \d+: link/.test(l));
      seen.watchingLegs = browserLegs(hubFrom);
      if (sessions.length || seen.watchingLegs) throw new Error(`watching opened a session: ${sessions[0] ?? `${seen.watchingLegs} browser leg(s)`}`);
      if (!(await primary("connect-wifi"))) throw new Error("Connect over Wi‑Fi is no longer the primary");
      const pressFrom = hold.console.mark();
      const pressHub = cloud.log().length;
      await driver.pressOffer("connect-wifi", { board: mac, timeoutMs: STEP_MS });
      seen.lan = (await hold.console.waitFor(/\[lan\] link \S+.*secure session opening/, { from: pressFrom, what: "a LAN link" })).trim();
      seen.wifiLine = await driver.waitBar("wifi", /(?<!not )connected$/, { board: mac, timeoutMs: STEP_MS });
      seen.pressLegs = browserLegs(pressHub);
      if (seen.pressLegs) throw new Error(`the hub opened ${seen.pressLegs} browser leg(s) for a Wi‑Fi connect`);

      // The same press with Wi‑Fi refused, on another fresh profile.
      await driver.close();
      driver = await launch();
      await signIn(true);
      await load(plainUrl);
      await openOnline();
      await driver.waitFor(`Boolean(${await driver.card({ board: mac })}?.querySelector('[data-name-bar] [data-offer-path$="/connect-wifi"]'))`, {
        timeoutMs: STEP_MS,
        what: "Connect over Wi‑Fi as the card's primary (refused profile)",
      });
      const refusedFrom = hold.console.mark();
      const logFrom = pageLog.length;
      await driver.pressOffer("connect-wifi", { board: mac, timeoutMs: STEP_MS });
      seen.fellThrough = (await waitSaid("trying lightplayer.app.", { from: logFrom })).text;
      seen.relay = (await hold.console.waitFor(/\[relay\] route \d+: link \S+, secure session opening/, { from: refusedFrom, what: "a relay route after the refused Wi‑Fi" })).trim();
      await readyOnRelay();
      await delay(2_000);
      seen.lines = said("trying lightplayer.app.", logFrom).length;
      if (seen.lines !== 1) throw new Error(`${seen.lines} fall-through lines, not one`);
      await driver.screenshot(path.join(shots, "studio-relay-T8-fell-through.png"));
      return { lan: seen.lan, wifiLine: seen.wifiLine, fellThrough: seen.fellThrough, relay: seen.relay };
    });
    }
    if (options.account) {
    const entryB = (await boardRegistry(door.addr)).find((b) => b.id === B);
    const macB = String(entryB.mac).toLowerCase();
    const idB = relayId(macB);
    const usbB = `serial:ws://${door.addr}/board/${B}/bytes`;
    report.boardB = { mac: macB, relayId: idB };
    /// Whether the hub ever had `id` online.
    const hubHadOnline = (id) => cloud.log().includes(`relay: board ${id} online`);

    await step("T9", `${B}, with no Wi‑Fi, gets the account's key over USB: it joins the account's list, and never the relay`, async (seen) => {
      const before = await awaitStatus(usbB, (s) => s.relay !== undefined, { what: "its relay state", timeoutMs: JOIN_MS });
      seen.before = { station: stationOf(before), relay: before.relay };
      if (JSON.stringify(before.relay) !== JSON.stringify("noAccount")) {
        throw new Error(`before Studio, ${B}'s relay is ${JSON.stringify(before.relay)}, not noAccount`);
      }
      if (stationOf(before).kind === "connected") throw new Error(`${B} is on a network: ${JSON.stringify(seen.before.station)}`);
      if ((await accountBoards()).some((b) => b.id === idB)) throw new Error(`${B} is on the account's list before Studio met it`);
      // A project on it, so the list has one to say.
      const upload = await lpCli(["upload", path.join(ROOT, "projects/test/basic"), usbB], { timeoutMs: STEP_MS });
      if (upload.code !== 0) throw new Error(`upload → exit ${upload.code}: ${upload.stderr.trim().split("\n").slice(-3).join(" | ")}`);

      // The first profile, signed in, meets it over USB.
      await signIn(true);
      await driver.navigate(usbUrl);
      await driver.awaitShim();
      await driver.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: 420_000, what: "Studio to finish loading" });
      await driver.pressConnect("USB", { timeoutMs: STEP_MS });
      await driver.pickBoard(B, { timeoutMs: STEP_MS });
      await driver.waitCard({ board: macB, timeoutMs: STEP_MS });
      await driver.boardRuns({ board: macB, timeoutMs: STEP_MS });
      // lightplayer.app's list: Studio's report put it there.
      const row = await waitListed(idB, true, `held ${B}`);
      seen.record = row;
      if (!row.seen?.usb) throw new Error(`${B}'s record was not seen over USB: ${JSON.stringify(row.seen)}`);
      if (row.seen?.relay) throw new Error(`${B}'s record says the relay saw it: ${JSON.stringify(row.seen)}`);
      if (row.online) throw new Error(`${B}'s record says it is online at the relay: ${JSON.stringify(row.online)}`);
      // Let go of its USB link, then ask the board itself.
      await driver.navigate("about:blank");
      const after = await awaitStatus(usbB, (s) => s.relay !== undefined && JSON.stringify(s.relay) !== JSON.stringify("noAccount"), {
        what: "a relay state other than noAccount (it holds the account's key)",
        timeoutMs: STEP_MS,
      });
      seen.after = { station: stationOf(after), relay: after.relay };
      if (JSON.stringify(after.relay) !== JSON.stringify("waitingForInternet")) {
        throw new Error(`after Studio, ${B}'s relay is ${JSON.stringify(after.relay)}, not waitingForInternet`);
      }
      if (stationOf(after).kind === "connected") throw new Error(`${B} joined a network: ${JSON.stringify(seen.after.station)}`);
      if (hubHadOnline(idB)) throw new Error(`the hub had ${B} online`);
      return {
        relay: `${JSON.stringify(before.relay)} → ${JSON.stringify(after.relay)}`,
        station: seen.after.station.kind,
        record: { label: row.label, firmware: row.firmware ?? null, project: row.project ?? null, seen: row.seen },
      };
    }, { fatal: true });

    await step("T10", `the power goes off: a profile that never met ${A} or ${B} shows both under Offline boards with what the account last knew`, async (seen) => {
      // c6-a online at lightplayer.app, as T1 left it: its record before.
      seen.recordA = await waitListed(board, true, `held ${A}`);
      if (!seen.recordA.online) throw new Error(`${A} is not online at lightplayer.app before the power goes: ${JSON.stringify(seen.recordA)}`);
      if (hold) {
        await releaseConsole(hold);
        hold = null;
      }
      const hubFrom = cloud.log().length;
      const offAt = Date.now() / 1000;
      await stopDoor(door);
      seen.hub = await waitHub(new RegExp(`relay: board ${board} offline[^\\n]*`), `${A} offline`, hubFrom);
      // A fresh profile, as on a computer that never met either board.
      await driver.close();
      driver = await launch();
      await signIn(true);
      await load(plainUrl);
      await waitOfflineCard(mac, `${A}'s card under Offline boards`);
      await waitOfflineCard(macB, `${B}'s card under Offline boards`);
      // lightplayer.app's last picture of c6-a, lit (kept while it is offline).
      seen.litA = await driver.waitFor(
        `(() => { const canvas = ${await driver.card({ board: mac })}?.querySelector('[data-picture="cloud"] canvas');
                  if (!canvas || !canvas.width || !canvas.height) return false;
                  const data = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
                  let lit = 0;
                  for (let i = 0; i < data.length; i += 4) if (data[i + 3] > 0 && data[i] + data[i + 1] + data[i + 2] > 60) lit += 1;
                  return lit > 0 ? lit : false; })()`,
        { timeoutMs: STEP_MS, what: `${A}'s last picture from lightplayer.app to light some lamps` },
      );
      const rows = await accountBoards();
      const rowA = rows.find((b) => b.id === board);
      const rowB = rows.find((b) => b.id === idB);
      if (!rowA || !rowB) throw new Error(`lightplayer.app's list lost a board: ${JSON.stringify(rows.map((b) => b.id))}`);
      seen.rowA = rowA;
      seen.rowB = rowB;
      seen.offAt = offAt;
      if (rowA.online || rowB.online) throw new Error(`a board is still online at lightplayer.app: ${JSON.stringify(rows)}`);
      const lastSeenA = Math.max(...Object.values(rowA.seen ?? {}));
      seen.lastSeenAfterOffS = Math.round((lastSeenA - offAt) * 10) / 10;
      // AC2: last seen accurate to the poll (30 s).
      if (!(Math.abs(lastSeenA - offAt) <= 30)) throw new Error(`${A}'s last seen is ${seen.lastSeenAfterOffS} s from the power going, more than a 30 s poll`);
      seen.a = await offlineFacts(mac);
      seen.b = await offlineFacts(macB);
      await driver.screenshot(path.join(shots, "studio-relay-T10-offline-never-met.png"));
      const check = (who, facts, row) => {
        if (facts.section !== "offline") throw new Error(`${who}'s card is under ${facts.section}, not Offline boards`);
        if (facts.name !== row.label) throw new Error(`${who}'s card is named ${JSON.stringify(facts.name)}, not the board's ${JSON.stringify(row.label)}`);
        if (!/last seen/.test(facts.second ?? "")) throw new Error(`${who}'s card says no "last seen": ${JSON.stringify([facts.first, facts.second])}`);
        const firmware = firmwareWords(row.firmware);
        if (!firmware || !(facts.lines.firmware ?? "").includes(firmware)) {
          throw new Error(`${who}'s firmware row says ${JSON.stringify(facts.lines.firmware)}, the record ${JSON.stringify(row.firmware)} (${JSON.stringify(firmware)})`);
        }
        if (!row.project || !(facts.lines.project ?? "").toLowerCase().includes(row.project.toLowerCase())) {
          throw new Error(`${who}'s project row says ${JSON.stringify(facts.lines.project)}, the record ${JSON.stringify(row.project)}`);
        }
        if (!(facts.lines.cloud ?? "").includes(WALK_EMAIL)) throw new Error(`${who}'s lightplayer.app row says ${JSON.stringify(facts.lines.cloud)}`);
        const connects = facts.verbs.filter((verb) => /^connect/.test(verb));
        if (connects.length) throw new Error(`${who}'s offline card offers ${connects.join(", ")}`);
      };
      check(A, seen.a, rowA);
      check(B, seen.b, rowB);
      if (seen.a.picture?.source !== "cloud" || !seen.a.picture?.frame) throw new Error(`${A}'s picture is ${JSON.stringify(seen.a.picture)}, not lightplayer.app's`);
      return { hub: seen.hub, lastSeenAfterOffS: seen.lastSeenAfterOffS, a: seen.a, b: seen.b, litA: seen.litA };
    }, { fatal: true });

    await step("T11", `Remove from your account (two clicks) takes ${B} off the account's list on every computer; ${A} stays`, async (seen) => {
      const scopeB = await driver.card({ board: macB });
      seen.firstClick = await driver.pressOffer("forget-from-account", { board: macB, bar: "hardware", timeoutMs: STEP_MS });
      // Armed, not pressed: still on lightplayer.app's list, and still drawn.
      seen.armedListed = (await accountBoards()).some((b) => b.id === idB);
      seen.armedDrawn = await driver.evaluate(`Boolean(${scopeB})`);
      if (!seen.armedListed || !seen.armedDrawn) throw new Error(`one click removed ${B} (listed ${seen.armedListed}, drawn ${seen.armedDrawn})`);
      seen.secondClick = await driver.pressOffer("forget-from-account", { board: macB, bar: "hardware", timeoutMs: 3_000 });
      await driver.waitFor(`!${scopeB}`, { timeoutMs: STEP_MS, what: `${B}'s card to leave` });
      await waitListed(idB, false, `let go of ${B}`);
      if (!(await accountBoards()).some((b) => b.id === board)) throw new Error(`${A} left the list too`);
      // Another computer: a fresh profile shows c6-a and not c6-b.
      await driver.close();
      driver = await launch();
      await signIn(true);
      await load(plainUrl);
      await waitOfflineCard(mac, `${A}'s card under Offline boards`);
      await delay(3_000);
      seen.cards = await driver.cardPaths();
      if (seen.cards.includes(boardPath(macB))) throw new Error(`${B} still shows on a fresh profile: ${JSON.stringify(seen.cards)}`);
      await driver.screenshot(path.join(shots, "studio-relay-T11-after-remove.png"));
      return { firstClick: seen.firstClick, secondClick: seen.secondClick, cards: seen.cards, list: (await accountBoards()).map((b) => b.id) };
    });
    }
  } catch (error) {
    fatal = error;
  }

  report.configuration = configuration;
  writeFileSync(path.join(out, `walk-wifi-emu-${options.lane}.json`), JSON.stringify(report, null, 2));
  writeFileSync(path.join(out, "relay-cloud.log"), cloud.log());
  writeFileSync(path.join(out, "page-console.log"), pageLogAll.join("\n"));
  console.log("\n=== the studio-relay walk, step by step");
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${s.id}  ${s.describe}`);
  console.log(`  configuration ${configuration} through a local relay · lp-emu ${commit}`);
  console.log(`  summary → ${path.join(out, `walk-wifi-emu-${options.lane}.json`)}`);
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
  console.log(`\n✓ the ${options.lane} walk finished ${options.move ? "T1, T6–T8" : options.account ? "T1, T9–T11" : "T1–T5"} (${configuration}, lp-emu ${commit}).`);
}

await main();
