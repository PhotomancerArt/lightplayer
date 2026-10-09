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
//                has X: the new board's card is set up (`adopt`), its firmware
//                bar says "Needs …, which Studio can't get" → Install Y
//   crashing     E10 · NOT walked: no image makes an engine keep crashing yet
//                (Part B's U7 skip, for the same reason) — said so in the report
//   needs-usb    E9 · a pre-update board (today's single image): no update over
//                the air, today's USB flash (Lasting, `update-firmware`) on the
//                card
//   store-backup R5 · a board on a PUBLISHED release (XR, `build-image.sh …
//                2026.10.07-77` into images/x-release, not built by the
//                recipe): with an empty cache the update takes the board's
//                engine from the release store — the walk's store serves XR —
//                and never reads it back. Not in the default steps
//   install-older "Other version…" over the release index (OTA M10), inline in
//                the firmware bar's details: a board
//                on release r2 (`2026.10.02-1`) and a REAL lp-cloud-server in
//                front of a GitHub-shaped upstream holding r1 and r2 (their
//                assets, a REST releases list, `latest`). The list shows r2
//                as the board's own, r1 and this Studio's build; r1, typed
//                whole into the list's box (which narrows the list to it),
//                arms (older: Lasting) and installs on the second press of
//                `install-firmware`; then r2
//                installs at one click (newer: Routine). Both images are
//                built by the recipe (`build-image.sh` into images/r1,
//                images/r2), and differ only in their version. Not in the
//                default steps: `--steps install-older`
//   install-lookup The same store with r1 left out of its releases list (its
//                assets still served): r1 typed whole into the box finds
//                nothing in the list, so the press reads "Look up r1"; the
//                store finds it by version, it joins the list, arms (older)
//                and installs on the second press. `--steps install-lookup`
//   install-file  "From a file…" (`install-firmware-file`, in the same
//                details): only r2 in the store; r1's `ota/` folder
//                (its manifest, core, engine and .z files) is put into the
//                card's file input as the file dialog would hand it over;
//                core checks it, r1 joins the list "from your files", arms
//                with the custom-build copy, and installs — never asking the
//                store for r1. `--steps install-file`
//
// Every assertion waits for the BOARD's words (its console, `[OTA]`,
// `[LOADER]`, `[CORE]` lines) as well as its card's. The card is the board
// card (`lp-app/lpa-studio-web/src/app/board_card/`, built in core,
// `lpa-studio-core/src/app/devices/board_card/`), read by its hooks only: its
// bars (`data-bar`, their work `data-bar-work`), its status corner
// (`data-board-corner`, whose details hold the board's terminal) and the
// offers core publishes on it (`data-offer-path`), pressed by path — never
// `#main`'s text, which Studio can satisfy by itself. A card line alone
// proves nothing about the board. The browser profile remembers earlier
// boards as offline cards, so every step names its board's card by the MAC
// the door (or the tab's bus) gave it. Each step gets its own door (a fresh
// board) and a page load; the page's origin is the walk's own server, so the
// browser's engine cache (OPFS) carries from one step to the next.
//
// `--tab` runs update / cut-core / engine-less against `?emu=tab` (the board a
// Worker in the page, no door): the tab board's chip is written with the same
// seeded X chip (or its engine-less copy) the door lane boots, and power
// cycled, before Studio is asked to find it; its console is read off the
// page's emulator as it goes.
//
// `--ble` (`just walk-ota-ble-emu`, M7 P12) walks update / cut-core /
// phantom-core / engine-less with Studio reaching the same door boards over
// `?ble=emu` — Studio's real Bluetooth stack (`browser_ble.js`, its lp-link
// end and channel 3) against the `navigator.bluetooth` polyfill, which
// translates its datagrams to the board's USB stream and models every reset
// of the board as a GATT drop (the board's radio goes with its CPU). The
// card's connection bar must say "Bluetooth" while it updates, and Studio's
// lines in the board's terminal must time every reconnect:
//
//   cut-backup    the board goes out of range mid-BACKUP (an empty engine
//                 cache, so the update reads X's engine back first) and comes
//                 back: the backup resumes where it was, and the update
//                 finishes with no click (2026-10-07: on silicon a drop
//                 there ended the update; also walkable over `?emu=`)
//   cut-core      the board goes out of range mid-core (the banner's
//                 `detach`: the GATT connection drops and connects fail until
//                 `attach`), comes back: the update finishes with no click
//   phantom-core  Bluefy's phantom drop mid-core (`gatt.connected` false, no
//                 event, found when the page is shown again): torn down,
//                 reconnected, finished with no click
//
// `--lan` (`just walk-ota-emu --lan`, OTA M8 P8) walks the update over the
// boards' Wi‑Fi: each board is on the door's virtual LAN (`lan=home`, the
// network saved on its chip when it was seeded), and Studio reaches it with
// NO `?emu=` at all — `?lan=ws://<its forward>/link`, Studio's real LAN
// stack (`browser_websocket.js`, its secure lp-link and channel 3) against
// the board's own LAN endpoint, through core-only and its three resets. The
// card's connection bar must say "Wi‑Fi" while it updates, and Studio's lines
// in the board's terminal must time the reconnects:
//
//   update        X → Y with one press, as on USB, every reset a socket close
//                 the page redials by itself
//   cut-core      the board's power cut (`power-cycle` on its control
//                 channel) mid-core: it reboots, rejoins, and the update
//                 finishes with no click
//   cut-engine    the same, mid-engine
//   engine-less   an engine-less board on Wi‑Fi (core-only serves the LAN
//                 link, no hello) is restored on connect with no click
//   renumber      the board's next lease is a different address
//                 (`renumber`): the update's first reset puts it at a new
//                 IP, and the update finishes. ⚠️ Studio dials the board's
//                 port FORWARD (127.0.0.1), which follows the board, so this
//                 proves the update across a renumbered board — not
//                 Studio's `.local` fallback, which needs a resolver the
//                 emulated LAN does not give the host
//   second-client mid-update, lp-cli (`wifi status lan:<forward>`) is turned
//                 away busy, and a second Studio typing the address is told
//                 "Busy with another connection"; the update finishes
//
// `--relay` (`just walk-ota-emu --relay`, OTA M8 PR C) walks the update
// through lightplayer.app's relay, played by a REAL lp-cloud-server on this
// machine (mem store, a made-up account signed in by dev sign-in): each
// board is on the door's virtual LAN with an uplink that carries
// `lightplayer.app:80` to that server, the account's key installed on its
// chip as Studio installs it over USB, so it registers by itself; Studio
// is served on the walk's own origin with `/api`, `/auth` and `/relay`
// forwarded to the server (one origin, as on lightplayer.app, so the
// session cookie rides the relay's browser leg) and opened with
// `?relay=<board>` — no `?emu=`, no `?lan=`. The card's connection bar must
// say "Wi‑Fi via lightplayer.app" while it updates:
//
//   update        X → Y with one press through the relay, every reset the
//                 board's relay leg dropping and the page riding through
//                 "board offline" until it is back
//   relay-drop    the relay drops the board mid-core (the walk cuts its
//                 device leg): the board dials again, the page redials, and
//                 the update finishes with no click
//   cut-engine    the board's power cut mid-engine, as on --lan
//
// ⚠️ TRUST: over `?ble=emu` the emulated board sees its trusted USB link, so
// every request — the update's login included — is answered at the edit
// tier: this proves the transport, the card and the reconnects, NOT access
// (P10's host tests and the silicon walk do). And NO number it prints is a
// Bluetooth number: the bytes go through the emulated board's USB link, with
// no radio, no connection interval and no MTU — every rate and reconnect time
// is labelled with the lp-emu commit and `?ble=emu`.
//
// NOT a CI job (minutes of emulated boards). It serves the release bundle
// itself (no dev server): `just studio-web-story-build`, the images and a
// debug lp-cli first — `just walk-ota-emu` builds what is missing.
//
//   node scripts/emu/walk-ota-emu.mjs [--fresh] [--tab | --ble | --lan | --relay] [--steps update,cut-core,...]
//
// The browser profile (target/walk-ota-emu/chrome-profile) outlives a run, so
// `engine-less` finds the engine `update` backed up even when the steps run
// as separate invocations; `--fresh` starts from a browser that has never
// seen a board.

import { appendFileSync, existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import path from "node:path";
import process from "node:process";
import { execFileSync, spawn } from "node:child_process";

import { PANEL, StudioDriver, boardPath } from "./studio-driver.mjs";
import {
  boardRegistry,
  openNetworkRow,
  serveStudioBundle,
  startDoor,
  startRecordSink,
  stopDoor,
  walkPort,
} from "./emulated-lane.mjs";
import { WALK_EMAIL, forwardHttp, forwardUpgrade, relayId, startRelayCloud } from "./walk-ota-relay.mjs";
import {
  FIXTURE,
  LAN,
  NET,
  control as doorControl,
  forwardOf,
  holdConsole,
  lpCli,
  refusedAsInUse,
  releaseConsole,
} from "./walk-wifi-emu-lan.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const ARGS = process.argv.slice(2);
const TAB = ARGS.includes("--tab");
/// Studio reaches the door's boards over `?ble=emu` (M7 P12).
const BLE = ARGS.includes("--ble");
/// Studio reaches the door's boards over their Wi‑Fi (`?lan=`, OTA M8).
const LAN_LANE = ARGS.includes("--lan");
/// Studio reaches them through a local relay (`?relay=`, OTA M8 PR C).
const RELAY_LANE = ARGS.includes("--relay");
/// The boards are on the door's virtual LAN (both Wi‑Fi lanes).
const ON_LAN = LAN_LANE || RELAY_LANE;
const STEPS_ARG = ARGS.includes("--steps") ? ARGS[ARGS.indexOf("--steps") + 1].split(",") : null;
/// Every step, in the order their boards' MACs are numbered.
const ALL_STEPS = ["update", "cut-core", "cut-engine", "engine-less", "cant-get", "crashing", "needs-usb", "phantom-core", "cut-backup", "store-backup", "install-older", "install-lookup", "install-file", "renumber", "second-client", "relay-drop"];
const DOOR_STEPS = ["update", "cut-core", "cut-engine", "engine-less", "cant-get", "crashing", "needs-usb"];
const TAB_STEPS = ["update", "cut-core", "engine-less"];
const BLE_STEPS = ["update", "cut-backup", "cut-core", "phantom-core", "engine-less"];
const LAN_STEPS = ["update", "cut-core", "cut-engine", "engine-less", "renumber", "second-client"];
const RELAY_STEPS = ["update", "relay-drop", "cut-engine"];
const STEPS = STEPS_ARG ?? (TAB ? TAB_STEPS : BLE ? BLE_STEPS : LAN_LANE ? LAN_STEPS : RELAY_LANE ? RELAY_STEPS : DOOR_STEPS);
/// The steps that stand r1 and r2 behind a real lp-cloud-server.
const RELEASE_STEPS = ["install-older", "install-lookup", "install-file"].some((step) => STEPS.includes(step));
/// The link this lane updates over, in the steps' words.
const LINK_WORD = BLE ? "Bluetooth" : ON_LAN ? "Wi\u2011Fi" : "USB";
/// The link the card must name: its connection bar's summary leads with the
/// link's label (`UiLinkKind::label`, `ui_link_kind.rs`), then " · " and how
/// it is going ("USB · live", "Bluetooth · connected", a new board's "USB ·
/// new" — `connection_bar.rs`). Through the relay it is "Wi‑Fi via
/// lightplayer.app".
const LINK_LABEL = BLE ? "Bluetooth" : RELAY_LANE ? "Wi\u2011Fi via lightplayer.app" : LAN_LANE ? "Wi\u2011Fi" : "USB";
if (RELAY_LANE && (TAB || BLE || LAN_LANE)) {
  console.error("walk-ota-emu: --relay walks the door's boards through a local relay; it does not combine with --tab, --ble or --lan");
  process.exit(2);
}
if (RELAY_LANE && STEPS.some((step) => !RELAY_STEPS.includes(step))) {
  console.error(`walk-ota-emu: --relay walks ${RELAY_STEPS.join(", ")}`);
  process.exit(2);
}
if (!RELAY_LANE && STEPS.includes("relay-drop")) {
  console.error("walk-ota-emu: relay-drop is a relay step (--relay)");
  process.exit(2);
}
if (LAN_LANE && (TAB || BLE)) {
  console.error("walk-ota-emu: --lan walks the door's boards over Wi-Fi; it does not combine with --tab or --ble");
  process.exit(2);
}
if (!LAN_LANE && STEPS.some((step) => step === "renumber" || step === "second-client")) {
  console.error("walk-ota-emu: renumber and second-client are Wi-Fi steps (--lan)");
  process.exit(2);
}
if (LAN_LANE && STEPS.some((step) => !LAN_STEPS.includes(step))) {
  console.error(`walk-ota-emu: --lan walks ${LAN_STEPS.join(", ")}`);
  process.exit(2);
}
if (TAB && BLE) {
  console.error("walk-ota-emu: --ble walks the door's boards; it does not combine with --tab");
  process.exit(2);
}
for (const name of STEPS) {
  if (!BLE && name === "phantom-core") {
    console.error("walk-ota-emu: phantom-core is a Bluetooth step (--ble)");
    process.exit(2);
  }
}

const IMAGES = path.join(ROOT, "target/walk-ota-emu/images");
const X = path.join(IMAGES, "x");
/// X at a release version, for `store-backup` (built by hand, see the header).
const XR = path.join(IMAGES, "x-release");
const xr = existsSync(path.join(XR, "ota/ota-manifest.json"))
  ? JSON.parse(readFileSync(path.join(XR, "ota/ota-manifest.json"), "utf8"))
  : null;
const MONO = path.join(IMAGES, "mono");
/// `install-older`'s two releases (the recipe builds them).
const R1 = path.join(IMAGES, "r1");
const R2 = path.join(IMAGES, "r2");
const LP_CLOUD_SERVER = path.join(ROOT, "target/debug/lp-cloud-server");
/// Y's update files and package. `WALK_Y_PARTS` / `WALK_Y_PACKAGES` point at
/// a copy instead — for a worktree where something else rebuilds the shared
/// `target/firmware-parts` while a walk runs.
const PARTS = process.env.WALK_Y_PARTS ?? path.join(ROOT, "target/firmware-parts");
const PACKAGES = process.env.WALK_Y_PACKAGES ?? path.join(ROOT, "target/studio-web-assets/firmware");
const LP_CLI = path.join(ROOT, "target/debug/lp-cli");
const PROJECT = process.env.WALK_PROJECT ?? "Peach (1D)";

/// Waits. Every one is the page's or the board's, never a sleep: these are
/// only how long the walk lets one stage take before it calls it failed.
const STEP_MS = 180_000;
/// A board booting ROM-up and joining the virtual LAN (a deadline, not a
/// measurement).
const JOIN_MS = 300_000;
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

/// The page's own text, for one thing only: that Studio has loaded at all.
/// Never a card's verb or state: those are read by the card's hooks.
const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;

// --- what the update shows -------------------------------------------------

/// The card's update lines, for the record: the first time each kind showed.
/// Each is matched against the step's board's firmware bar as it reads (its
/// work while the update runs, else its summary) and, on a new board's card,
/// against its connection details' `State`.
///
/// - The firmware bar's work while an update runs is the update's short line
///   (the board card's copy pass, `device_update_words.rs`; `firmware_bar.rs`
///   `firmware_work`, tested by `the_header_chip_takes_the_updates_word_
///   while_it_owns_the_board`): "Backing up · 18%", "Updating · 1 of 2 ·
///   40%" (the new core), "Updating · 2 of 2 · 70%" (its engine: the old
///   "Finishing the update"), "Resuming · 2 of 2 · 70%" (the same, found
///   half-way), "Restoring · 35%", "Another device is updating it · 40%".
///   The link is no longer in the line: the connection bar says it.
/// - A new board's card (`pending_board_card`: a core-only board says no
///   hello, so it is not kept) tells no update story: its connection
///   details' `State` carries the model's own stage words
///   (`pending_link_view`, `lpa-devices/src/view.rs`; the labels are
///   `UpdateStageFacts::label`): "Backing up current firmware… 40%",
///   "Updating firmware… 40%", "Restoring firmware… 35%", "Finishing the
///   update… 70%", "Another device is updating it… 40%".
/// - What needs a person is the bar's summary (the update's line, NeedsYou):
///   "Needs <v>, which Studio can't get", "Needs one update over USB".
///
/// Two kinds are not lines: "available" is the firmware bar's action, the
/// over-the-air `update-firmware`, offered (`cardLines`); "up to date" is
/// the firmware details' sentence (`upToDate`), since the bar shows the
/// version alone.
const CARD_LINES = [
  ["backing up", /^Backing up\b[^\n]*/],
  ["updating", /^(?:Updating · 1 of 2|Updating firmware…)[^\n]*/],
  ["finishing", /^(?:(?:Updating|Resuming) · 2 of 2|Finishing the update…)[^\n]*/],
  ["resuming", /^Resuming · [^\n]*/],
  ["restoring", /^Restoring\b[^\n]*/],
  ["another device", /^Another device is updating it[^\n]*/],
  ["cant get", /^Needs [^\n]*, which Studio can't get/],
  ["needs usb", /^Needs one update over USB/],
];

/// A MAC's 12 lowercase hex, the way a card's path carries it
/// (`devices/mac-<12 hex>`, `BoardRef`).
const macHex = (mac) => (mac ? String(mac).replace(/:/g, "").toLowerCase() : "");

async function main() {
  for (const [what, at] of [
    ["the release Studio bundle (just studio-web-story-build)", path.join(ROOT, "target/dx/lpa-studio-web/release/web/public")],
    ["X's split image (scripts/ota/build-image.sh target/walk-ota-emu/images/x a0a0a0a0)", path.join(X, "merged.bin")],
    ["the pre-update single image (lp-cli firmware package esp32c6-4mb --single-image --out …/mono/package)", path.join(MONO, "package/manifest.json")],
    ["Y, this Studio's split package (lp-cli firmware package esp32c6-4mb)", path.join(PACKAGES, "esp32c6-4mb/manifest.json")],
    ["a debug lp-cli (cargo build -p lp-cli)", LP_CLI],
    ...(RELEASE_STEPS
      ? [
          ["release r1 (scripts/ota/build-image.sh target/walk-ota-emu/images/r1 2026.10.01-1)", path.join(R1, "ota/ota-manifest.json")],
          ["release r2 (scripts/ota/build-image.sh target/walk-ota-emu/images/r2 2026.10.02-1)", path.join(R2, "ota/ota-manifest.json")],
          ["a debug lp-cloud-server (cargo build -p lp-cloud-server)", LP_CLOUD_SERVER],
        ]
      : []),
    ...(RELAY_LANE ? [["a debug lp-cloud-server (cargo build -p lp-cloud-server)", LP_CLOUD_SERVER]] : []),
  ]) {
    if (!existsSync(at)) {
      console.error(`walk-ota-emu: missing ${what}: ${at}\n  run: just walk-ota-emu (it builds what is missing)`);
      process.exit(1);
    }
  }

  const stamp = new Date().toISOString().replace(/[:.]/g, "-");
  const out = path.join(ROOT, "target/walk-ota-emu", `${TAB ? "tab-" : BLE ? "ble-" : LAN_LANE ? "lan-" : RELAY_LANE ? "relay-" : ""}${stamp}`);
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
  // The Wi‑Fi lane's boards: X seeded the same way, with the virtual LAN's
  // network saved (a `networkAdd`, as Studio's Wi‑Fi panel sends it), and
  // its engine-less copy — an engine-less board can take no `wifi add`.
  const xLanChip = path.join(chips, "x-lan.bin");
  const xLanEngineLess = path.join(chips, "x-lan-engine-less.bin");
  if (LAN_LANE) {
    seedChip(path.join(X, "merged.bin"), xLanChip, "60s", [
      JSON.stringify({ networkAdd: { ssid: NET.ssid, password: NET.password } }),
    ]);
    const bytes = readFileSync(xLanChip);
    bytes.fill(0xff, xSplit.engine.offset, xSplit.engine.offset + 0x1000);
    writeFileSync(xLanEngineLess, bytes);
  }
  const fixturePath = path.join(out, "virtual_lan.toml");
  if (LAN_LANE) writeFileSync(fixturePath, FIXTURE);
  // The relay lane's boards: seeded once the relay's cloud is up (their
  // chips carry its account's key), below.
  const xRelayChip = path.join(chips, "x-relay.bin");
  const chipFiles = { x: xChip, "x-engine-less": xEngineLess };
  const xrChip = path.join(chips, "x-release.bin");
  if (xr && STEPS.includes("store-backup")) seedChip(path.join(XR, "merged.bin"), xrChip);
  const r2Chip = path.join(chips, "r2.bin");
  if (RELEASE_STEPS) seedChip(path.join(R2, "merged.bin"), r2Chip);
  const lpEmu = git(["log", "-1", "--format=%h", "--", "lp-emu"]);
  const head = git(["rev-parse", "--short=12", "HEAD"]);

  const served = stagedY;
  /// The relay lane's cloud, once started (below).
  let cloud = null;
  const route = (request, response, url) => {
    // One origin, as on lightplayer.app: the relay's cloud answers these.
    if (cloud && /^\/(api|auth|relay)(\/|$)/.test(url.pathname)) {
      forwardHttp(request, response, cloud.port);
      return true;
    }
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
  if (RELAY_LANE) {
    cloud = await startRelayCloud({
      root: ROOT,
      binary: LP_CLOUD_SERVER,
      studioOrigin: `http://localhost:${studioPort}`,
      log: path.join(out, "lp-cloud-server.log"),
    });
    bundle.on("upgrade", (request, socket, head) => {
      if (new URL(request.url, "http://x").pathname.startsWith("/relay/")) forwardUpgrade(request, socket, head, cloud.port);
      else socket.destroy();
    });
    // The fixture's uplink: `lightplayer.app` is the walk's cut-able
    // forward in front of the cloud.
    writeFileSync(fixturePath, `${FIXTURE}\n[[uplink]]\nname = "lightplayer.app"\nto = "127.0.0.1:${cloud.devicePort}"\n`);
    // A new account each run (mem store), so a new key: always seeded anew.
    rmSync(xRelayChip, { force: true });
    seedChip(path.join(X, "merged.bin"), xRelayChip, "60s", [
      JSON.stringify({ networkAdd: { ssid: NET.ssid, password: NET.password } }),
      cloud.accessAdd,
    ]);
  }
  // A firmware store that holds nothing (every lookup 404s): X is a dev
  // build no store would have, and the walk never touches the internet. The
  // one exception is XR, the release `store-backup` stands its board on:
  // `/firmware/<target>/<release>[+<id>]/<file>` from images/x-release/ota.
  const storeHits = [];
  const store = await new Promise((resolve) => {
    const server = createServer((request, response) => {
      const parts = decodeURIComponent(new URL(request.url, "http://store").pathname).split("/");
      // ["", "firmware", target, release-or-build-id, file]
      const release = parts[3]?.split("+")[0];
      const file = parts[4];
      if (xr && parts[1] === "firmware" && parts[2] === xr.target && release === xr.version && file && !file.includes("..")) {
        const at = path.join(XR, "ota", file);
        if (existsSync(at)) {
          storeHits.push(file);
          response.writeHead(200, {
            "access-control-allow-origin": "*",
            "content-type": file.endsWith(".json") ? "application/json" : "application/octet-stream",
          });
          response.end(readFileSync(at));
          return;
        }
      }
      response.writeHead(404, { "access-control-allow-origin": "*" });
      response.end();
    });
    server.listen(0, "127.0.0.1", () => resolve(server));
  });
  const storeOrigin = `http://127.0.0.1:${store.address().port}`;
  // `WALK_RECORD=1`: the page records its whole session (`?record=`) into
  // `records.jsonl` beside the report — the device journal, every request,
  // every transport's bytes — for when a step fails and the card alone
  // cannot say why.
  const recorder = process.env.WALK_RECORD ? startRecordSink() : null;
  const recordUrl = recorder ? await recorder.listen() : null;

  console.log(`\nTHE OVER-THE-AIR UPDATE WALK WITH NO BOARD${BLE ? " — OVER ?ble=emu" : LAN_LANE ? " — OVER WI-FI (?lan=, the emulated LAN)" : RELAY_LANE ? " — THROUGH THE RELAY (?relay=, a local lp-cloud-server)" : ""}`);
  if (RELAY_LANE) {
    console.log("  ⚠️  a local lp-cloud-server stands in for lightplayer.app: no internet, no fly proxy, no");
    console.log("     NAT, no real round trips; the board's leg crosses the emulated LAN's uplink.");
    console.log(`  relay           ${cloud.origin} (device leg via 127.0.0.1:${cloud.devicePort}); account ${WALK_EMAIL}`);
  }
  if (LAN_LANE) {
    console.log("  ⚠️  the emulated LAN proves the IP stack, the link and the board's rules — not the radio,");
    console.log("     not Chrome's Local Network prompt; Studio dials each board's 127.0.0.1 forward.");
  }
  if (BLE) {
    console.log("  ⚠️  ?ble=emu proves the transport, the card and the reconnects — not access (the");
    console.log("     emulated board answers at the edit tier), and no number here is a Bluetooth number.");
  }
  console.log(`  this tree       ${head} (lp-emu ${lpEmu}, lp-emu:esp32c6:t1${ON_LAN ? "+net=lan" : ""})`);
  console.log(`  X (the boards)  ${x.version}+${x.commit.slice(0, 12)}`);
  console.log(`  Y (this Studio) ${y.version}+${y.commit.slice(0, 12)}`);
  console.log(`  Studio          http://127.0.0.1:${studioPort}/ (the release bundle, served by this walk)`);
  console.log(`  host tty        ${EMU_TTY ?? "the page's default (a Mac's model on a Mac)"}`);
  console.log(`  steps           ${STEPS.join(", ")}${TAB ? " (?emu=tab)" : BLE ? " (?ble=emu)" : LAN_LANE ? " (?lan=)" : RELAY_LANE ? " (?relay=)" : ""}\n`);

  const report = {
    tree: head,
    lpEmu,
    configuration: ON_LAN ? "lp-emu:esp32c6:t1+net=lan" : "lp-emu:esp32c6:t1",
    backing: TAB ? "tab" : "door",
    link: BLE
      ? "?ble=emu (the polyfill over the door's USB link; not a radio)"
      : LAN_LANE
        ? "?lan= (Studio's LAN link through the board's port forward, on the emulated LAN; not a radio)"
        : RELAY_LANE
          ? "?relay= (Studio through a local lp-cloud-server's relay; the board's leg through the emulated LAN's uplink; not the internet)"
          : "?emu= (Web Serial over the door)",
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
  // Signed in to the relay's made-up account, on Studio's origin.
  if (RELAY_LANE) {
    await driver.cdp.send(
      "Network.setCookie",
      { name: "lp_session", value: cloud.cookie, url: `http://localhost:${studioPort}`, path: "/", httpOnly: true },
      driver.sessionId,
    );
  }
  let door = null;
  /// The Wi‑Fi lane's console capture on the board's USB link.
  let hold = null;

  const pageUrl = (doorAddr, firmwareStore = storeOrigin) =>
    (LAN_LANE
      ? `http://localhost:${studioPort}/?lan=${encodeURIComponent(`ws://${door.forward}/link`)}` +
        `&firmware-store=${encodeURIComponent(firmwareStore)}`
      : RELAY_LANE
      ? `http://localhost:${studioPort}/?relay=${door.relayId}&firmware-store=${encodeURIComponent(firmwareStore)}`
      : `http://localhost:${studioPort}/?emu=${doorAddr ? encodeURIComponent(`ws://${doorAddr}`) : "tab"}` +
        `&firmware-store=${encodeURIComponent(firmwareStore)}` +
        (EMU_TTY ? `&emu-tty=${EMU_TTY}` : "") +
        (BLE ? "&ble=emu" : "")) + (recordUrl ? `&record=${encodeURIComponent(recordUrl)}` : "");

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
  const boardWords = (board) => (TAB ? tabConsole : hold ? hold.console.text() : boardConsole(board));
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
  // --- the step's board, as its card reads ---------------------------------
  //
  // Every board is drawn by the board card, and the walk reads it by its
  // hooks (`lp-app/lpa-studio-web/src/app/board_card/mod.rs`, "Walk hooks")
  // through the driver's card helpers, or the snapshot below where it reads
  // several at once. The browser profile remembers earlier boards as offline
  // cards, so the walk NAMES its board, by the MAC its door gave it.

  /// The step's board: its door (or tab) id, its MAC, and its card's path
  /// (`devices/mac-<12 hex>`: core keys a board by its MAC once it has said
  /// who it is). Set by `nameBoard` before each step.
  let here = null;
  /// Name the step's board by its MAC: the door's registry (`GET /boards`
  /// lists each board's `mac`), or the tab's bus (`describeBoards()`), else
  /// `fallback` (the `mac=` the walk gave the door).
  const nameBoard = async (board, fallback = null) => {
    let mac = null;
    try {
      if (TAB) mac = (await driver.boards())?.find((row) => row.boardId === board)?.mac ?? null;
      else if (door) mac = (await boardRegistry(door.addr)).find((row) => row.id === board)?.mac ?? null;
    } catch {
      /* the fallback stands */
    }
    mac = mac ?? fallback;
    if (!mac) throw new Error(`no MAC for ${board}: the walk cannot name its card`);
    here = { board, mac, path: boardPath(mac) };
    return here;
  };
  /// The step's board's card once Studio watches it and has kept it — live
  /// (`data-board-corner` not `quiet`), not a new board's (`blank`) — as
  /// `snapshotExpr` picks it: its path, which the card helpers take, kept in
  /// `here.path`. A page-side wait.
  const liveCardPath = async (timeoutMs = STEP_MS) => {
    try {
      here.path = await driver.waitFor(
        `(() => { const card = JSON.parse(${snapshotExpr()}).board;
                  return card && card.corner !== 'quiet' && card.corner !== 'blank' ? card.path : false; })()`,
        { timeoutMs, what: `the card of ${here.mac} (live, and kept)` },
      );
    } catch (error) {
      // Say what the page held instead, so a miss reads as which card.
      const seen = await driver.evaluate(snapshotExpr()).catch((e) => `(unreadable: ${e.message})`);
      throw new Error(`${error.message.split("\n")[0]} — the page held: ${seen}`);
    }
    return here.path;
  };
  /// Page-side: the step's board as its card reads now, as JSON — local to
  /// this walk (the driver's helpers read one bar at a time; a watch reads
  /// them all at one instant). WHICH card: the one whose path ends with the
  /// board's MAC while Studio watches it; else a live `new-<n>` card — a
  /// board that has not said who it is (a core-only board says no hello),
  /// which in a step can only be this one (each step's door holds one board,
  /// and an earlier step's are offline: `data-board-corner="quiet"`); else
  /// its own card offline. For that card: its path; the corner's mark
  /// (`blank`: a new board's card, `pending_board_card`); whether it sits in
  /// the home page's Offline boards (`#home-offline-boards`); the firmware and
  /// connection bars' lines (a bar's pieces joined, as the driver's
  /// `barText`) and work (`data-bar-work`); the verbs on its face this walk
  /// reads (`data-offer-path`: enabled, and whether a press arms — a
  /// Lasting button carries `.ux-armed-label-armed`); and, while its
  /// connection details are open, their `State` fact.
  const snapshotExpr = () => `(() => {
    const hex = ${JSON.stringify(macHex(here?.mac))};
    const words = (el) => (el?.textContent || '').replace(/\\s+/g, ' ').trim();
    const lineOf = (el) => (el ? [...el.children].map(words).filter(Boolean).join(' ') : null);
    const pathOf = (card) => card.getAttribute('data-board-card') || '';
    const cornerOf = (card) => card.querySelector('[data-board-corner]')?.getAttribute('data-board-corner') ?? null;
    const live = (card) => cornerOf(card) !== 'quiet';
    const fresh = (card) => /^devices\\/new-\\d+$/.test(pathOf(card));
    const cards = [...document.querySelectorAll('[data-board-card]')];
    const named = cards.filter((card) => hex !== '' && pathOf(card).endsWith('-' + hex) && !fresh(card));
    // Last, the page's one live card: an in-tab board's bus lists the tab's
    // own MAC, while its card is keyed by the MAC the guest's hello says
    // (the seeded image's), so a tab step's board can be named by neither.
    const onlyLive = cards.filter(live);
    const card = named.find(live) ?? cards.find((card) => fresh(card) && live(card)) ?? named[0]
      ?? (onlyLive.length === 1 ? onlyLive[0] : null);
    if (!card) return JSON.stringify({ board: null, cards: cards.map(pathOf) });
    const bar = (layer, name) => {
      const el = card.querySelector('[data-bar="' + layer + '"]');
      if (!el) return null;
      return { line: lineOf(el.querySelector('button[aria-label="' + name + ' details"]')), work: el.getAttribute('data-bar-work') || 'none' };
    };
    const face = (verb) => {
      const mark = [...card.querySelectorAll('[data-offer-path$="/' + verb + '"]')].find((m) => !m.closest('.ux-popover-layer'));
      const buttons = mark ? mark.querySelectorAll('button') : [];
      const button = buttons.length ? buttons[buttons.length - 1] : null;
      return button ? { enabled: !button.disabled, arms: Boolean(button.querySelector('.ux-armed-label-armed')) } : null;
    };
    const fact = (layer, label) => {
      const dt = [...card.querySelectorAll('[data-bar="' + layer + '"] [id^="ux-popover-panel"] dt')]
        .find((el) => words(el).toLowerCase() === label.toLowerCase());
      return dt?.nextElementSibling ? words(dt.nextElementSibling) : null;
    };
    return JSON.stringify({
      board: {
        path: pathOf(card),
        corner: cornerOf(card),
        offlineBoards: Boolean(card.closest('#home-offline-boards')),
        firmware: bar('firmware', 'Firmware'),
        connection: bar('connection', 'Connection'),
        offers: { 'update-firmware': face('update-firmware'), 'install-firmware': face('install-firmware'), edit: face('edit') },
        state: fact('connection', 'State'),
      },
      cards: cards.map(pathOf),
    });
  })()`;
  const cardSnapshot = async () => {
    try {
      return JSON.parse(await driver.evaluate(snapshotExpr()));
    } catch {
      return { board: null, cards: [] }; // mid-navigation
    }
  };
  /// The path of the card the step's board is on right now (see
  /// `snapshotExpr`), else its own.
  const boardPathNow = async () => (await cardSnapshot()).board?.path ?? here.path;
  /// The update lines the snapshot's card shows (`CARD_LINES`): its firmware
  /// bar, and a new board's connection details' `State`; and "available"
  /// while the firmware bar's action is the over-the-air `update-firmware`
  /// (Routine: it does not arm — the USB flash at the same path does).
  const cardLines = (snap) => {
    const seen = {};
    const card = snap.board;
    if (!card) return seen;
    for (const text of [card.firmware?.line, card.state]) {
      if (!text) continue;
      for (const [kind, pattern] of CARD_LINES) {
        const match = text.match(pattern);
        if (match && !seen[kind]) seen[kind] = match[0].trim();
      }
    }
    const update = card.offers["update-firmware"];
    if (update?.enabled && !update.arms && card.firmware?.work !== "running") seen.available = card.firmware?.line ?? "update-firmware";
    return seen;
  };
  /// Studio's own lines in the board's terminal — the status corner's
  /// details (`data-board-terminal`), where `DeviceTerminal` starts each of
  /// Studio's rows with `▸ `: the update's narration, rates and reconnect
  /// times.
  const terminalLines = async () => {
    const rows = await driver.terminalLines({ board: await boardPathNow(), timeoutMs: STEP_MS });
    return rows.filter((line) => line.startsWith("▸ ")).map((line) => line.slice(2));
  };
  /// The firmware details' text (opened for the read, then closed).
  const firmwareDetails = async (cardPath) => {
    await driver.openBar("firmware", { board: cardPath, timeoutMs: 10_000 });
    try {
      return await driver.evaluate(
        `(document.querySelector(${JSON.stringify(`[data-board-card="${cardPath}"] [data-bar="firmware"] [id^="ux-popover-panel"]`)})?.innerText || '')`,
      );
    } finally {
      await driver.closeDetails({ board: cardPath }).catch(() => {});
    }
  };
  /// Watch the step's board's card until `done(snapshot, lines, order)`
  /// holds, noting every update line it shows on the way (the order they
  /// first appeared in). While the board is on a new board's card, its
  /// connection details stay open: their `State` is where that card says how
  /// its update goes.
  const watchCard = async (done, what, timeoutMs = UPDATE_MS, onTick = null) => {
    const order = [];
    const began = Date.now();
    let lastTrail = "";
    let pageSeen = 0;
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      await refreshTab();
      const snap = await cardSnapshot();
      if (snap.board?.corner === "blank") {
        await driver.openBar("connection", { board: snap.board.path, timeoutMs: 3_000 }).catch(() => {});
      }
      const lines = cardLines(snap);
      for (const [kind, line] of Object.entries(lines)) {
        if (!order.some((entry) => entry.kind === kind)) order.push({ kind, line, atMs: Date.now() });
      }
      if (onTick) await onTick(lines, snap);
      // A live trail for whoever watches the walk: the card's lines as they
      // change (and which card, and its link), with the time since the watch
      // began.
      const pageLines = driver.consoleLines();
      if (pageLines.length !== pageSeen) {
        pageSeen = pageLines.length;
        writeFileSync(path.join(out, "page-console.log"), pageLines.join("\n"));
      }
      const now = JSON.stringify({ card: snap.board?.path ?? null, link: snap.board?.connection?.line ?? null, lines });
      if (now !== lastTrail) {
        lastTrail = now;
        appendFileSync(trail, `${((Date.now() - began) / 1000).toFixed(1)} s ${now}\n`);
      }
      if (await done(snap, lines, order)) return order;
      if (Date.now() > deadline) {
        throw new Error(`the card never reached ${what}; it showed ${JSON.stringify(order)}; it reads ${JSON.stringify(snap.board)}`);
      }
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
  };
  /// Done once the board runs this Studio's build: its card kept and live
  /// (not a new board's, not offline), its firmware bar carrying no running
  /// work, and its firmware details saying so — "…, the same as this
  /// Studio." (the update's UpToDate sentence, `device_update_words.rs`,
  /// which the details' facts carry: `firmware_bar.rs` `details`; the bar
  /// itself shows the version alone). The details open at most every 2 s,
  /// and only while the bar is idle. A fresh one per watch.
  const upToDate = () => {
    let lookedAt = 0;
    return async (snap, lines, order) => {
      const card = snap.board;
      if (!card || card.corner === "blank" || card.corner === "quiet") return false;
      if (!card.firmware || card.firmware.work === "running") return false;
      if (Date.now() - lookedAt < 2_000) return false;
      lookedAt = Date.now();
      const said = (await firmwareDetails(card.path).catch(() => ""))
        .split("\n")
        .find((line) => line.includes("the same as this Studio"));
      if (!said) return false;
      order.push({ kind: "up to date", line: said.trim(), atMs: Date.now() });
      return true;
    };
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
    // The page as it stood: what a person reading the record would have
    // seen. The board's terminal is in its status corner's details, closed
    // on the page, so it is read there and kept beside the page's text;
    // Studio's own rows in it (the update's narration: rates, reconnect
    // times) start with `▸`.
    let terminal = [];
    try {
      const page = await driver.evaluate("document.body.innerText");
      writeFileSync(path.join(out, `${name}-page.txt`), page);
    } catch {
      /* the page may be gone */
    }
    try {
      if (here) {
        const rows = await driver.terminalLines({ board: await boardPathNow(), timeoutMs: 10_000 });
        writeFileSync(path.join(out, `${name}-terminal.txt`), rows.join("\n"));
        terminal = rows.filter((line) => line.startsWith("▸ ")).map((line) => line.slice(2));
      }
    } catch {
      /* no card, or no terminal on it (an offline card draws none) */
    }
    const record = {
      name,
      describe,
      ok: !error,
      error: error?.message ?? null,
      wallSeconds: Math.round((Date.now() - started) / 1000),
      ...(note ?? {}),
      cardPath: here?.path ?? null,
      terminal,
      shot: file,
    };
    report.steps.push(record);
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"}${note?.summary ? `  ${note.summary}` : ""}`);
    for (const line of record.terminal) console.log(`    · ${line}`);
    return !error;
  };

  /// A fresh door holding `boards`, and the page loaded against it (and
  /// against `firmwareStore`, when not the walk's empty store).
  const openDoor = async (id, boards, firmwareStore = storeOrigin) => {
    if (hold) await releaseConsole(hold);
    hold = null;
    if (door) await stopDoor(door);
    door = await startDoor({
      root: ROOT,
      id,
      boards: ON_LAN ? boards.map((board) => `${board},lan=${LAN}`) : boards,
      stateDir: path.join(out, "state", id),
      consoleDir: path.join(out, "console", id),
      logFile: path.join(out, `serve-${id}.log`),
      fresh: true,
      extraArgs: ON_LAN ? ["--lan", `${LAN}=${fixturePath}`] : [],
    });
    if (ON_LAN) {
      // The board's forward: what Studio dials for it on the host.
      const board = boards[0].split("=")[0];
      const entry = (await boardRegistry(door.addr)).find((b) => b.id === board);
      door.forward = forwardOf(entry);
      door.board = board;
      // The board's words: a console capture on its USB link (`lp-cli link
      // capture`), as the LAN walk reads them — after the network seam's
      // start, the firmware's log goes out on the USB lp-link's console
      // channel, and nothing else carries it. ⚠️ So a USB host IS on the
      // board's link for the walk: it drives nothing (the update is
      // Studio's, over the LAN), but its link counts as "a host on a link"
      // for the trial's confirmation. The LAN-only confirmation is PR A's
      // emulated scenario (`test-emu-c6-ota-lan`), not this walk's.
      hold = await holdConsole({ doorAddr: door.addr, board, file: path.join(out, "console", id, `${board}.link.log`) });
    }
    if (RELAY_LANE) {
      // The page dials the relay for the board at load, and a board the
      // relay does not hold yet is "offline" (which ends a session nothing
      // holds): load it once the board has registered by itself.
      door.relayId = relayId(boards[0].match(/mac=([0-9a-f:]+)/i)[1]);
      await waitBoard(door.board, /\[relay\] state=connected/, "it reached the relay", JOIN_MS);
    }
    await loadPage(door.addr, firmwareStore);
  };
  const loadPage = async (doorAddr, firmwareStore = storeOrigin) => {
    await driver.navigate(pageUrl(doorAddr, firmwareStore));
    if (!ON_LAN) await driver.awaitShim();
    if (BLE) await driver.waitFor("Boolean(window.__lpEmuBluetooth)", { timeoutMs: LOAD_MS, what: "the Bluetooth polyfill" });
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
    if (RELAY_LANE) {
      // No chooser: the board registered before the page loaded, and the
      // page dialled it through the relay at load.
      return;
    }
    if (LAN_LANE) {
      // No chooser: the page dialled the board's forward at load. The board
      // boots, joins the virtual LAN and serves its link; the page's
      // session redials until it answers.
      await waitBoard(board, /\[wifi\] address \d+\.\d+\.\d+\.\d+/, "it joined the virtual LAN", JOIN_MS);
      return;
    }
    await driver.pressConnect(BLE ? "Bluetooth" : "USB", { timeoutMs: STEP_MS });
    await driver.pickBoard(board, { timeoutMs: STEP_MS });
  };
  /// Over Wi‑Fi, every reset closes the board's socket and the page redials
  /// it: Studio's terminal must time at least `atLeast` reconnects.
  const checkWifiReconnects = (terminal, atLeast) => {
    if (!ON_LAN) return null;
    const lines = reconnectLines(terminal);
    if (lines.length < atLeast) {
      throw new Error(`over Wi-Fi the terminal timed ${lines.length} reconnects, fewer than ${atLeast}: ${JSON.stringify(lines)}`);
    }
    return { timed: lines };
  };
  /// The addresses the board said it was given on the LAN, in order
  /// (`[wifi] address a.b.c.d`, the station's own line).
  const boardAddresses = (board, from = 0) =>
    [...boardWords(board).slice(from).matchAll(/\[wifi\] address (\d+\.\d+\.\d+\.\d+)/g)].map((m) => m[1]);
  /// The polyfill's count of what happened on a board's air (`?ble=emu`):
  /// connects, the board's side of the link opening, resets turned into
  /// GATT drops.
  const bleStats = (board) =>
    BLE
      ? driver.evaluate(`JSON.stringify(window.__lpEmuBluetooth?.stats(${JSON.stringify(board)}) ?? null)`).then(JSON.parse)
      : Promise.resolve(null);
  /// Studio's terminal lines that time a reconnect (P5's narration).
  const reconnectLines = (terminal) => terminal.filter((line) => /reconnected in /.test(line));
  /// Over Bluetooth every drop the board's link had — a reset or one of the
  /// walk's own — must come back with its time on Studio's terminal.
  /// `timed: false` where no terminal is drawn to time them in: a restore
  /// runs on a core-only board's PENDING link (it says no hello), and a
  /// pending card draws no terminal (docs/defects/
  /// 2026-10-06-a-restore-on-a-pending-link-narrates-to-no-terminal.md).
  const checkReconnects = (terminal, before, after, drops, { timed = true } = {}) => {
    if (!BLE) return null;
    const lines = reconnectLines(terminal);
    const resets = (after?.resetDrops ?? 0) - (before?.resetDrops ?? 0);
    const reconnects = resets + drops;
    if (resets < 1) throw new Error(`the polyfill turned no board reset into a GATT drop (${JSON.stringify(after)})`);
    if (timed && lines.length < reconnects) {
      throw new Error(`${reconnects} reconnects (${resets} board resets, ${drops} drops) but the terminal timed ${lines.length}: ${JSON.stringify(lines)}`);
    }
    return { resets, drops, timed: lines };
  };
  /// The card must name this lane's link while the board updates. The
  /// update's line no longer carries it (`device_update_words.rs`: the link
  /// is in the sentence); the connection bar does — its summary, read on
  /// every tick the firmware bar said "Updating · 1 of 2", leads with the
  /// link's label (`LINK_LABEL`).
  const checkLinkWord = (ran) => {
    if (!ran.order.some((entry) => entry.kind === "updating")) throw new Error("the card never said Updating · 1 of 2");
    if (!ran.links.some((line) => line.startsWith(`${LINK_LABEL} · `))) {
      throw new Error(`while it updated, the card's connection bar said ${JSON.stringify(ran.links)}, never ${LINK_LABEL}`);
    }
  };
  /// Put the walk's project on the board, by its card's offers: a board
  /// running a project already (its primary Edit can be pressed) is left as
  /// it is; an empty one is given `push` — the project bar's "Add a
  /// project", drawn as the project pick — then the project in the picker,
  /// "Put it on the board", and the board's own "Project loaded" in its
  /// terminal.
  const pushProject = async () => {
    const board = here.path;
    const face = await driver.boardRuns({ board, timeoutMs: STEP_MS });
    if (face === "running") return "already running a project";
    await driver.pressOffer("push", { board, timeoutMs: STEP_MS });
    await driver.waitFor(`Boolean(${PANEL})`, { timeoutMs: STEP_MS, what: "the project picker" });
    await driver.click(PROJECT, { scope: PANEL, exact: true });
    await driver.clickWhenReady("Put it on the board", { scope: PANEL, timeoutMs: STEP_MS });
    await driver.boardSaid("Project loaded", { board, timeoutMs: STEP_MS });
    return `${PROJECT} loaded`;
  };
  /// Page-side read of the button an offer's mark carries on the card at
  /// `cardPath` — on its face, or in its open details (`inDetails`): its
  /// words (the resting label of a Lasting button, which carries its armed
  /// label too), whether a press arms it (Lasting: `.ux-armed-label-armed`,
  /// `ActionButton`), and whether it opens a pick.
  const offerButton = (cardPath, verb, inDetails = false) =>
    driver.evaluate(`(() => {
      const card = document.querySelector(${JSON.stringify(`[data-board-card="${cardPath}"]`)});
      const mark = card && [...card.querySelectorAll(${JSON.stringify(`[data-offer-path$="/${verb}"]`)})]
        .find((m) => Boolean(m.closest('.ux-popover-layer')) === ${Boolean(inDetails)});
      const buttons = mark ? mark.querySelectorAll('button') : [];
      const button = buttons.length ? buttons[buttons.length - 1] : null;
      if (!button) return null;
      const words = (el) => (el?.textContent || '').replace(/\\s+/g, ' ').trim();
      return {
        words: words(button.querySelector('.ux-armed-label-rest') ?? button),
        arms: Boolean(button.querySelector('.ux-armed-label-armed')),
        // A pick is a popover's trigger (aria-expanded), or a details row
        // that hands one to its bar (aria-haspopup).
        pick: button.hasAttribute('aria-expanded') || button.hasAttribute('aria-haspopup'),
      };
    })()`);
  /// Every verb the step's board's card draws on its face and in its
  /// firmware details (opened for the read, then closed): each mark's last
  /// path segment, where it is, its words, whether a press arms it
  /// (Lasting), whether it opens a pick.
  const cardOffers = async () => {
    const board = here.path;
    const read = (inDetails) =>
      driver.evaluate(`(() => {
        const card = document.querySelector(${JSON.stringify(`[data-board-card="${board}"]`)});
        if (!card) return [];
        const words = (el) => (el?.textContent || '').replace(/\\s+/g, ' ').trim();
        return [...card.querySelectorAll('[data-offer-path]')]
          .filter((mark) => Boolean(mark.closest('.ux-popover-layer')) === ${inDetails})
          .map((mark) => {
            const buttons = mark.querySelectorAll('button');
            const button = buttons.length ? buttons[buttons.length - 1] : null;
            return {
              verb: (mark.getAttribute('data-offer-path') || '').split('/').pop(),
              where: ${JSON.stringify(inDetails ? "firmware details" : "face")},
              words: words(button?.querySelector('.ux-armed-label-rest') ?? button),
              arms: Boolean(button?.querySelector('.ux-armed-label-armed')),
              pick: Boolean(button) && (button.hasAttribute('aria-expanded') || button.hasAttribute('aria-haspopup')),
            };
          });
      })()`);
    const face = await read(false);
    await driver.openBar("firmware", { board, timeoutMs: STEP_MS });
    try {
      return [...face, ...(await read(true))];
    } finally {
      await driver.closeDetails({ board }).catch(() => {});
    }
  };
  /// Press an update verb where the card draws it: on its face (the
  /// firmware bar's action, `firmware_bar.rs`), or — an install with a list
  /// to choose from — in the firmware details. A Lasting press (an install
  /// not known newer: two dev builds have no order, so "Install <Y>" on the
  /// E13 row arms) arms on its first click and presses on its second
  /// (`pressOffer`'s `confirm`). `overTheAir`: the verb must be the Routine
  /// over-the-air update ("Update", or "Install <Y>" between two builds,
  /// `device_update_offers.rs`) — never the USB flash, which is published
  /// at the same `update-firmware` and is Lasting (or a board pick).
  const pressUpdate = async (verb, { overTheAir = false } = {}) => {
    const board = await boardPathNow();
    let bar = null;
    if (verb === "update-firmware") {
      await driver.waitOffer(verb, { board, timeoutMs: STEP_MS });
    } else {
      try {
        await driver.waitOffer(verb, { board, timeoutMs: 30_000 });
      } catch {
        bar = "firmware";
        await driver.waitOffer(verb, { board, bar, timeoutMs: STEP_MS });
      }
    }
    const button = await offerButton(board, verb, Boolean(bar));
    if (!button) throw new Error(`the card's \`${verb}\` went away before it could be pressed`);
    if (overTheAir && (button.arms || button.pick)) {
      throw new Error(`the card's \`${verb}\` reads "${button.words}" and is the USB flash (Lasting), not the over-the-air update`);
    }
    await driver.pressOffer(verb, { board, bar, confirm: button.arms, timeoutMs: STEP_MS });
    if (bar) await driver.closeDetails({ board }).catch(() => {});
    return button.arms ? `${button.words} (armed, confirmed)` : button.words;
  };
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
    const label = await pressUpdate("update-firmware", { overTheAir: true });
    let cutAt = null;
    let cutShot = null;
    /// The cut stage's highest and lowest percent the card showed after the
    /// cut (a stage that starts over shows its low numbers again).
    let peakAfterCut = -1;
    let lowAfterCut = Infinity;
    let lastAir = "";
    let lastAirAt = 0;
    /// The connection bar's lines while the firmware bar said "Updating · 1
    /// of 2": what `checkLinkWord` reads the link from.
    const links = [];
    const order = await watchCard(upToDate(), "up to date on Y", UPDATE_MS, async (lines, snap) => {
      const link = snap.board?.connection?.line;
      if (lines.updating && link && !links.includes(link)) links.push(link);
      // Over `?ble=emu`, the air as the polyfill counts it, beside the
      // card's lines: connects, the board's side opening, reset drops.
      if (BLE) {
        const stats = await bleStats(board).catch(() => null);
        const connected = await driver
          .evaluate(`window.__lpEmuBluetooth?.describe(${JSON.stringify(board)})?.connected ?? null`)
          .catch(() => null);
        const air = JSON.stringify({
          connected,
          connects: stats?.connects,
          linkOpens: stats?.linkOpens,
          linkCloses: stats?.linkCloses,
          resetDrops: stats?.resetDrops,
        });
        // The edges as they happen, and the traffic every few seconds.
        if (air !== lastAir || Date.now() - lastAirAt > 5_000) {
          lastAir = air;
          lastAirAt = Date.now();
          appendFileSync(
            trail,
            `air ${air} writes=${stats?.writes} notifications=${stats?.notifications} textDropped=${stats?.textDropped}\n`,
          );
        }
      }
      if (cut && cutAt) {
        const after = Number(lines[cut.stage]?.match(/(\d+)%/)?.[1] ?? -1);
        peakAfterCut = Math.max(peakAfterCut, after);
        if (after >= 0) lowAfterCut = Math.min(lowAfterCut, after);
      }
      if (!cut || cutAt) return;
      const words = boardWords(board).slice(from);
      const line = lines[cut.stage];
      const percent = Number(line?.match(/(\d+)%/)?.[1] ?? -1);
      // A running engine says nothing per read-back: a backup's cut has no
      // board words to wait for.
      if ((!cut.boardSays || cut.boardSays.test(words)) && percent >= cut.atPercent) {
        cutAt = line;
        if (cut.act) {
          cutAt = `${line} (${await cut.act()})`;
          return;
        }
        if (cut.phantom) {
          // Bluefy's phantom drop: the page is told nothing, the board's side
          // stays up — until the page is shown again and re-checks.
          await driver.evaluate(`(() => { window.__lpEmuBluetooth.phantomDrop(${JSON.stringify(board)});
                                         document.dispatchEvent(new Event('visibilitychange')); })()`);
          cutShot = await shot("phantom-drop");
          writeFileSync(path.join(out, "phantom-drop-page.txt"), await driver.evaluate("document.body.innerText"));
          cutAt = `${line} (phantom drop; the page re-checked at once)`;
          return;
        }
        if (ON_LAN) {
          // The board's power, not a cable: there is none. It reboots,
          // rejoins the LAN, and the page redials it by itself.
          await doorControl(door.addr, board, "power-cycle");
          cutShot = await shot("power-cut");
          writeFileSync(path.join(out, "power-cut-page.txt"), await driver.evaluate("document.body.innerText"));
          cutAt = `${line} (power cut)`;
          return;
        }
        await driver.detach(board);
        const back = Date.now();
        cutShot = await shot(BLE ? "out-of-range" : "cable-out");
        writeFileSync(path.join(out, `${BLE ? "out-of-range" : "cable-out"}-page.txt`), await driver.evaluate("document.body.innerText"));
        if (BLE) {
          // Out of range: the GATT connection drops and every reconnect
          // fails until the board is back. Studio's side is its reconnect
          // loop (no Reconnect… button is owed on a link that comes back by
          // itself); the walk waits for the radio to be down, then the same
          // seconds a person walking back would take.
          await driver.waitFor(`!window.__lpEmuBluetooth.describe(${JSON.stringify(board)}).connected`, {
            timeoutMs: STEP_MS,
            what: "the GATT connection to drop",
          });
          await new Promise((resolve) => setTimeout(resolve, DETACHED_MS));
          await driver.attach(board);
          cutAt = `${line} (out of range for ${Date.now() - back} ms)`;
          return;
        }
        // The cable stays out until Studio has seen it go — the board's card
        // stops saying its link is up — then goes back in, as a person
        // re-seating it would. The card's connection bar leaves "<link> ·
        // live" / "<link> · connected" ("USB · not connected", "Offline · …",
        // or its work: `connection_bar.rs`), or the card goes quiet
        // (`data-board-corner="quiet"`: a board Studio is not watching), or
        // it moves to Offline boards (`#home-offline-boards`, the home
        // page's own section — where a core-only board's card may go).
        await driver.waitFor(
          `(() => { const card = JSON.parse(${snapshotExpr()}).board;
                    if (!card) return 'no card';
                    if (card.offlineBoards) return 'offline boards';
                    if (card.corner === 'quiet') return 'quiet';
                    const line = card.connection?.line ?? '';
                    return /· (live|connected)\\b/.test(line) ? false : line || 'no link'; })()`,
          { timeoutMs: STEP_MS, what: "Studio to see the cable go (the card's link no longer up)" },
        );
        // The one deliberate duration in this walk: a person re-seating a
        // cable takes seconds, not the instant the walk would otherwise take.
        await new Promise((resolve) => setTimeout(resolve, DETACHED_MS));
        await driver.attach(board);
        cutAt = `${line} (out for ${Date.now() - back} ms)`;
      }
    });
    return { label, order, links, cutAt, cutShot, from, peakAfterCut, lowAfterCut };
  };

  // The door writes a board's console file every 2 s (`emu serve`'s
  // FLUSH_EVERY), so the card can say the board is on its new build before
  // the file holds the engine's commit line: give the file one flush to catch
  // up before reading the board's words.
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

  /// `install-older`'s release store: `releases` laid out as GitHub serves
  /// them (`download/v<version>/<target>.<file>`, the newest under
  /// `latest/download/`), a GitHub-shaped REST releases list
  /// (`releases.json`: every asset its manifest names, `uploaded`), served
  /// by a small static server (a release marked `unlisted` keeps its assets
  /// but has no row), and a real lp-cloud-server in front of it
  /// (`LP_CLOUD_FIRMWARE_UPSTREAM`, `LP_CLOUD_FIRMWARE_RELEASES_LIST`) on a
  /// `scripts/dev-port.sh` port. Studio is pointed at the server, so the
  /// index it reads is the route's own answer.
  const startReleaseStore = async (step, releases) => {
    const root = path.join(out, "release-upstream");
    const list = [];
    for (const [at, { dir, publishedAt, unlisted = false }] of releases.entries()) {
      const manifest = JSON.parse(readFileSync(path.join(dir, "ota/ota-manifest.json"), "utf8"));
      const files = {
        "ota-manifest.json": path.join(dir, "ota/ota-manifest.json"),
        [manifest.core.file]: path.join(dir, "ota", manifest.core.file),
        [manifest.engine.file]: path.join(dir, "ota", manifest.engine.file),
        [manifest.package.file]: path.join(dir, "package/manifest.json"),
        [manifest.package.image.file]: path.join(dir, "package", manifest.package.image.file),
      };
      for (const encoding of manifest.encodings ?? []) {
        for (const piece of [encoding.core, encoding.engine]) {
          if (piece?.file) files[piece.file] = path.join(dir, "ota", piece.file);
        }
      }
      const homes = [path.join(root, `download/v${manifest.version}`)];
      if (at === 0) homes.push(path.join(root, "latest/download"));
      for (const home of homes) {
        mkdirSync(home, { recursive: true });
        for (const [file, from] of Object.entries(files)) {
          writeFileSync(path.join(home, `${manifest.target}.${file}`), readFileSync(from));
        }
      }
      // An unlisted release has its assets but no row in the list: older
      // than the index's window, reachable only by its version.
      if (!unlisted) list.push({
        tag_name: `v${manifest.version}`,
        draft: false,
        prerelease: false,
        published_at: publishedAt,
        assets: Object.keys(files).map((file) => ({ name: `${manifest.target}.${file}`, state: "uploaded" })),
      });
    }
    writeFileSync(path.join(root, "releases.json"), JSON.stringify(list));
    const upstreamHits = [];
    const upstream = await new Promise((resolve) => {
      const server = createServer((request, response) => {
        const at = path.join(root, decodeURIComponent(new URL(request.url, "http://upstream").pathname));
        upstreamHits.push(request.url);
        if (!at.startsWith(root) || !existsSync(at) || !statSync(at).isFile()) {
          response.writeHead(404);
          response.end();
          return;
        }
        response.writeHead(200, { "content-type": at.endsWith(".json") ? "application/json" : "application/octet-stream" });
        response.end(readFileSync(at));
      });
      server.listen(0, "127.0.0.1", () => resolve(server));
    });
    const upstreamOrigin = `http://127.0.0.1:${upstream.address().port}`;
    // One origin per step: each step serves its own list, and the browser
    // keeps a list for a minute (max-age=60) across runs at one origin.
    const port = execFileSync("scripts/dev-port.sh", [`walk-ota-cloud-${step}`], { cwd: ROOT, encoding: "utf8" }).trim();
    const origin = `http://127.0.0.1:${port}`;
    const log = path.join(out, "lp-cloud-server.log");
    const cloud = spawn(LP_CLOUD_SERVER, [], {
      cwd: ROOT,
      env: {
        ...process.env,
        LP_CLOUD_PORT: port,
        LP_CLOUD_BASE_URL: origin,
        LP_CLOUD_STORE: "mem",
        LP_CLOUD_BLOBS: "fs",
        LP_CLOUD_DATA_DIR: path.join(out, "cloud-data"),
        LP_CLOUD_FIRMWARE_UPSTREAM: upstreamOrigin,
        LP_CLOUD_FIRMWARE_RELEASES_LIST: `${upstreamOrigin}/releases.json`,
      },
      stdio: ["ignore", "pipe", "pipe"],
    });
    cloud.stdout.on("data", (chunk) => appendFileSync(log, chunk));
    cloud.stderr.on("data", (chunk) => appendFileSync(log, chunk));
    const stop = () => {
      cloud.kill();
      upstream.close();
    };
    const deadline = Date.now() + STEP_MS;
    for (;;) {
      try {
        if ((await fetch(`${origin}/healthz`)).ok) break;
      } catch {
        /* not up yet */
      }
      if (Date.now() > deadline || cloud.exitCode !== null) {
        stop();
        throw new Error(`lp-cloud-server never answered at ${origin} (its log: ${path.relative(ROOT, log)})`);
      }
      await new Promise((resolve) => setTimeout(resolve, 250));
    }
    return { origin, upstreamHits, stop };
  };
  /// A build id, as a regular expression's literal.
  const literal = (text) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  /// Page-side: "Other version…", inline in the firmware details
  /// (`UiDetailPanel::OtherVersion`, drawn by `other_version_form.rs` as one
  /// section of the details card titled in the offer's own words): the
  /// install offer's find box ("Type a version") over its version list (each
  /// version a button; the board's own drawn disabled, "On this board now";
  /// "this Studio's build", "from your files" beside a version), the copy of
  /// a Lasting pick, "From a file…" (`install-firmware-file`) and the press
  /// ("Install", or "Look up <v>"), marked `install-firmware`. The section is
  /// the one holding that mark inside the step's board's open details; null
  /// while they are closed.
  const otherVersion = () => `((() => {
    const card = document.querySelector(${JSON.stringify(`[data-board-card="${here.path}"]`)});
    const mark = card && [...card.querySelectorAll('[data-offer-path$="/install-firmware"]')].find((m) => Boolean(m.closest('.ux-popover-layer')));
    return mark ? mark.closest('section') : null; })())`;
  /// Open the firmware details, where "Other version…" is (the install offer
  /// with its list, `install-firmware`), and wait for the list to hold
  /// `version`. Returns the section's text.
  const openOtherVersion = async (version) => {
    await driver.waitOffer("install-firmware", { board: here.path, bar: "firmware", enabled: false, timeoutMs: STEP_MS });
    return driver.waitFor(
      `(() => { const t = ${otherVersion()}?.innerText ?? ''; return t.includes(${JSON.stringify(version)}) ? t : false; })()`,
      { timeoutMs: STEP_MS, what: `the version list to hold ${version}` },
    );
  };
  /// Whether the list's option for `version` is drawn disabled.
  const optionDisabled = (version) =>
    driver.evaluate(
      `[...(${otherVersion()}?.querySelectorAll('button') ?? [])].some((el) => el.disabled && (el.textContent || '').includes(${JSON.stringify(version)}))`,
    );
  /// Page-side: the Other version press armed (`.ux-armed`), its label swap
  /// (a 0.16 s fade) done, so a shot shows the armed reading alone.
  const installArmed = () => `(() => { const rest = ${otherVersion()}?.querySelector('.ux-armed .ux-armed-label-rest');
                                        return Boolean(rest) && getComputedStyle(rest).opacity === '0'; })()`;
  /// Install the version the Other version list holds picked, where the
  /// install is Lasting: the first press of `install-firmware` ARMS it (the
  /// walk waits for the arm — that it arms is the claim), `check` reads the
  /// section's text armed (the copy says what changes), `shotName` takes
  /// its picture, and the second press installs — `pressOffer`'s two clicks
  /// for `confirm`, with the armed reading caught between them. The details
  /// close after.
  const installAfterArming = async (version, { check = null, shotName = null } = {}) => {
    await driver.pressOffer("install-firmware", { board: here.path, bar: "firmware", timeoutMs: STEP_MS });
    await driver.waitFor(installArmed(), { timeoutMs: STEP_MS, what: `the press on ${version} to arm` });
    const copy = await driver.evaluate(`${otherVersion()}?.innerText ?? ''`);
    if (check) check(copy);
    const armedShot = shotName ? await shot(shotName) : null;
    await driver.pressOffer("install-firmware", { board: here.path, bar: "firmware", timeoutMs: STEP_MS });
    await driver.closeDetails({ board: here.path }).catch(() => {});
    return { copy, armedShot };
  };
  /// The board runs `release` and the card has finished: the board's core
  /// booted that build and committed its engine, and the board's card names
  /// the version — its firmware bar's summary, with no update work left on
  /// it — and runs its project: its primary Edit can be pressed, and the
  /// project details offer `remove-project`.
  const awaitRelease = async (board, release, from) => {
    const id = `${release.version}+${release.commit.slice(0, 12)}`;
    await waitBoard(board, new RegExp(`\\[OTA\\] offer ${literal(id)} → core`), `${id}'s core offer`, UPDATE_MS, from);
    const core = await waitBoard(board, new RegExp(`\\[CORE\\] [^\\n]*build ${literal(id)}`), `booting ${id}'s core`, UPDATE_MS, from);
    const order = await watchCard(
      async (snap) => {
        const card = snap.board;
        return Boolean(card) && card.corner !== "blank" && card.corner !== "quiet" && card.firmware?.work !== "running"
          && (card.firmware?.line ?? "").includes(release.version) && Boolean(card.offers.edit?.enabled);
      },
      `the card on ${release.version}, its project running`,
    );
    const cardPath = await liveCardPath();
    await driver.waitOffer("remove-project", { board: cardPath, bar: "project", timeoutMs: STEP_MS });
    await driver.closeDetails({ board: cardPath });
    await settle(board, from);
    const said = boardSaid(board, from, OTA_WORDS);
    for (const need of ["core offer", "core confirmed", "engine committed"]) {
      if (!said[need]) throw new Error(`the board never said ${need} for ${id}`);
    }
    return { id, core, order, said };
  };

  let fatal = null;
  try {
    for (const name of STEPS) {
      const board = TAB ? "tab-c6" : `c6-${name}`;
      // Every step's board is its own board: its own MAC, so no step's
      // card is a board Studio remembers from another.
      const lane = TAB ? 1 : LAN_LANE ? 2 : RELAY_LANE ? 3 : 0;
      const mac = `mac=02:4c:50:00:${(ALL_STEPS.indexOf(name) + 1).toString(16).padStart(2, "0")}:0${lane}`;
      const xBoard = `${board}=${LAN_LANE ? xLanChip : RELAY_LANE ? xRelayChip : xChip},kind=rom-up,${mac}`;
      // The step's board is named by its MAC (`nameBoard`, at the top of
      // each step): the door's registry says it, or the tab's bus; the `mac=`
      // the walk gave the door stands in when the registry does not.
      here = null;
      const nameThisBoard = () => nameBoard(board, TAB ? null : mac.slice("mac=".length));
      switch (name) {
        case "update":
          // An empty cache, so the update must back the board up first.
          await clearEngineCache();
          if (!TAB) await openDoor(name, [xBoard]);
          else {
            await loadPage(null);
            await seedTab("x");
          }
          await step("update", `X → Y with one press: back up, update over ${LINK_WORD}, finish; the project still runs`, async () => {
            await nameThisBoard();
            await connect(board);
            await liveCardPath();
            const pushed = await pushProject();
            const air0 = await bleStats(board);
            const ran = await runUpdate(board);
            const air1 = await bleStats(board);
            await settle(board, ran.from);
            const said = boardSaid(board, ran.from, OTA_WORDS);
            for (const need of ["core offer", "core on trial", "core confirmed", "engine offer", "engine committed"]) {
              if (!said[need]) throw new Error(`the board never said ${need}`);
            }
            if (!ran.order.some((entry) => entry.kind === "backing up")) throw new Error("the card never said Backing up");
            checkLinkWord(ran);
            const reconnects = ON_LAN
              ? checkWifiReconnects(await terminalLines(), 2)
              : checkReconnects(await terminalLines(), air0, air1, 0);
            // E14: the project survived — the card still runs it: its project
            // details offer `remove-project`.
            const kept = await liveCardPath();
            await driver.waitOffer("remove-project", { board: kept, bar: "project", timeoutMs: STEP_MS });
            await driver.closeDetails({ board: kept });
            const cache = await engineCache();
            const backup = cache.some((entry) => entry.includes(x.engine.sha256));
            if (!backup) throw new Error(`the engine cache does not hold X's engine (${x.engine.sha256.slice(0, 12)}…): ${JSON.stringify(cache)}`);
            return {
              summary: `${ran.label}; ${ran.order.map((e) => e.kind).join(" → ")}; project kept; X's engine cached${reconnects ? (reconnects.resets === undefined ? `; ${reconnects.timed.length} reconnects timed` : `; ${reconnects.resets} resets → ${reconnects.timed.length} reconnects timed`) : ""}`,
              pushed, card: ran.order, board: said, cache, air: air1, reconnects,
            };
          });
          break;
        case "store-backup": {
          if (!xr) throw new Error(`store-backup needs XR: scripts/ota/build-image.sh ${path.relative(ROOT, XR)} 2026.10.07-77`);
          await clearEngineCache();
          await openDoor(name, [`${board}=${xrChip},kind=rom-up,${mac}`]);
          await step(name, `a board on release ${xr.version}: its engine comes from the release store, never read back`, async () => {
            await nameThisBoard();
            await connect(board);
            // Ready, as core reads it: the card offers the board a project,
            // or the editor on the one it runs.
            await driver.boardRuns({ board: await liveCardPath(), timeoutMs: STEP_MS });
            const hitsBefore = storeHits.length;
            const ran = await runUpdate(board);
            await settle(board, ran.from);
            const said = boardSaid(board, ran.from, OTA_WORDS);
            for (const need of ["core offer", "core confirmed", "engine committed"]) {
              if (!said[need]) throw new Error(`the board never said ${need}`);
            }
            const hits = storeHits.slice(hitsBefore);
            if (!hits.includes("engine.bin")) throw new Error(`the store was never asked for XR's engine: ${JSON.stringify(hits)}`);
            if (ran.order.some((entry) => entry.kind === "backing up")) {
              throw new Error("the card said Backing up: the engine was read back, not fetched");
            }
            const cache = await engineCache();
            if (!cache.some((entry) => entry.includes(xr.engine.sha256))) {
              throw new Error(`the engine cache does not hold XR's engine: ${JSON.stringify(cache)}`);
            }
            const fetched = cache.some((entry) => entry.includes('"fetched"'));
            return {
              summary: `${ran.label}; ${ran.order.map((e) => e.kind).join(" → ")}; the store served ${hits.join(", ")}; no read-back; XR's engine cached${fetched ? " (source fetched)" : ""}`,
              card: ran.order, board: said, storeHits: hits, cache,
            };
          });
          break;
        }
        case "cut-backup": {
          // An empty cache: the update must read X's engine back first.
          await clearEngineCache();
          if (!TAB) await openDoor(name, [xBoard]);
          else {
            await loadPage(null);
            await seedTab("x");
          }
          const describe = BLE
            ? "the board goes out of range mid-backup and comes back: the backup resumes, the update finishes with no click"
            : "the cable comes out mid-backup and goes back in: the backup resumes, the update finishes with no click";
          await step(name, describe, async () => {
            await nameThisBoard();
            await connect(board);
            // Ready on X, as core reads it.
            await driver.boardRuns({ board: await liveCardPath(), timeoutMs: STEP_MS });
            const air0 = await bleStats(board);
            const ran = await runUpdate(board, { cut: { stage: "backing up", boardSays: null, atPercent: 40 } });
            const air1 = await bleStats(board);
            if (!ran.cutAt) throw new Error("the walk never found the moment to cut");
            await settle(board, ran.from);
            const said = boardSaid(board, ran.from, OTA_WORDS);
            for (const need of ["core offer", "core confirmed", "engine committed"]) {
              if (!said[need]) throw new Error(`the board never said ${need}`);
            }
            checkLinkWord(ran);
            // The backup went on after the cut: a "Backing up" line past the
            // percent it was cut at, and the read-back kept in the cache.
            const cutPercent = Number(ran.cutAt.match(/(\d+)%/)?.[1] ?? 0);
            if (ran.peakAfterCut <= cutPercent) {
              throw new Error(`the card never showed the backup past ${cutPercent}% after the cut (peak ${ran.peakAfterCut}%)`);
            }
            // Resumed, not started over: it never went back below the cut.
            if (ran.lowAfterCut < cutPercent) {
              throw new Error(`the backup started over after the cut: ${ran.lowAfterCut}% after ${cutPercent}%`);
            }
            const cache = await engineCache();
            if (!cache.some((entry) => entry.includes(x.engine.sha256))) {
              throw new Error(`the engine cache does not hold X's engine after the cut backup: ${JSON.stringify(cache)}`);
            }
            const reconnects = checkReconnects(await terminalLines(), air0, air1, 1);
            return {
              summary: `cut at ${ran.cutAt}; the backup resumed (never below ${ran.lowAfterCut}%) and went on to ${ran.peakAfterCut}%; X's engine cached; up to date${reconnects ? `; ${reconnects.resets} resets + 1 drop → ${reconnects.timed.length} reconnects timed` : ""}`,
              card: ran.order, board: said, air: air1, reconnects, cache,
            };
          });
          break;
        }
        case "cut-core":
        case "cut-engine":
        case "phantom-core": {
          const engine = name === "cut-engine";
          const phantom = name === "phantom-core";
          if (!TAB) await openDoor(name, [xBoard]);
          else {
            await loadPage(null);
            await seedTab("x");
          }
          const describe = phantom
            ? "Bluefy's phantom drop mid-core, found when the page is shown again: the update finishes with no click"
            : ON_LAN
              ? `the board's power is cut mid-${engine ? "engine" : "core"}: it reboots, rejoins, and the update finishes with no click`
              : BLE
              ? `the board goes out of range mid-${engine ? "engine" : "core"} and comes back: the update finishes with no click`
              : `the cable comes out mid-${engine ? "engine" : "core"} and goes back in: the update finishes with no click`;
          await step(name, describe, async () => {
            await nameThisBoard();
            await connect(board);
            // Ready on X, as core reads it.
            await driver.boardRuns({ board: await liveCardPath(), timeoutMs: STEP_MS });
            const air0 = await bleStats(board);
            const ran = await runUpdate(board, {
              cut: engine
                ? { stage: "finishing", boardSays: /\[OTA\] offer \S+ → engine/, atPercent: 40 }
                : { stage: "updating", boardSays: /\[OTA\] offer \S+ → core/, atPercent: 40, phantom },
            });
            const air1 = await bleStats(board);
            if (!ran.cutAt) throw new Error("the walk never found the moment to cut");
            await settle(board, ran.from);
            const said = boardSaid(board, ran.from, OTA_WORDS);
            // A phantom drop closes the board's side of the link (the page
            // tears it down), but the board keeps running: it resumes only if
            // its transfer was cut short, which the walk's own drop is.
            if (!said.resumed) throw new Error("the board never said it resumed the transfer");
            checkLinkWord(ran);
            // The engine, the update's second piece: "Updating · 2 of 2"
            // (or "Resuming · 2 of 2"; a new board's card: "Finishing the
            // update…").
            if (!BLE && !ran.order.some((entry) => entry.kind === "finishing")) throw new Error("the card never said Updating · 2 of 2");
            const reconnects = ON_LAN
              ? checkWifiReconnects(await terminalLines(), 2)
              : checkReconnects(await terminalLines(), air0, air1, 1);
            return {
              summary: `cut at ${ran.cutAt}; ${said.resumed}; up to date${reconnects ? (reconnects.resets === undefined ? `; ${reconnects.timed.length} reconnects timed` : `; ${reconnects.resets} resets + 1 drop → ${reconnects.timed.length} reconnects timed`) : ""}`,
              card: ran.order, board: said, air: air1, reconnects,
            };
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
          if (!TAB) await openDoor(name, [`${board}=${LAN_LANE ? xLanEngineLess : xEngineLess},kind=rom-up,${mac}`]);
          else {
            await loadPage(null);
            await seedTab("x-engine-less");
          }
          if (!clear) {
            await step(name, `the engine-less board${LAN_LANE ? ", reached over Wi‑Fi in core-only," : ""} restores itself on connect, with no click, from the cache`, async () => {
              const from = 0;
              await nameThisBoard();
              await connect(board);
              await waitBoard(board, /\[OTA\] offer \S+ → engine/, "the restore's engine offer");
              await shot("restoring");
              writeFileSync(path.join(out, "restoring-page.txt"), await driver.evaluate("document.body.innerText"));
              const air0 = await bleStats(board);
              // The restore runs on the new board's card (a core-only board
              // says no hello, so it is not kept): its connection details'
              // State says "Restoring firmware… N%" (`watchCard` keeps them
              // open). X running again: the board said who it is — its card
              // kept and live — and runs its project (its primary Edit can be
              // pressed) or is offered the update to Y over the air.
              const order = await watchCard(async (snap) => {
                const card = snap.board;
                if (!card || card.corner === "blank" || card.corner === "quiet") return false;
                const update = card.offers["update-firmware"];
                return Boolean(card.offers.edit?.enabled) || Boolean(update?.enabled && !update.arms);
              }, "X running again");
              const air1 = await bleStats(board);
              if (!order.some((entry) => entry.kind === "restoring")) throw new Error("the card never said Restoring");
              const reconnects = BLE ? checkReconnects(await terminalLines(), { resetDrops: 0 }, air1, 0, { timed: false }) : null;
              if (BLE && air0 === null) throw new Error("the Bluetooth polyfill holds no connection to the board");
              await settle(board, from);
              const said = boardSaid(board, from, OTA_WORDS);
              const lights = boardWords(board).slice(from).match(OTA_WORDS[7][1]) ?? [];
              if (!said["engine committed"]) throw new Error("the board never committed the engine");
              return {
                summary: `${order.map((e) => e.kind).join(" → ")}; lights: ${lights.map((l) => l.replace(/^.*\(/, "(")).join(" ")}${reconnects ? `; ${reconnects.resets} resets dropped and reconnected (a pending card draws no terminal to time them in)` : ""}`,
                card: order, board: said, lights, air: air1, reconnects,
              };
            });
          } else {
            await step(name, "the same board, the cache cleared and no store that has X: Studio says it can't get it; Install Y", async () => {
              const from = boardWords(board).length;
              await nameThisBoard();
              await connect(board);
              // A core-only board says no hello, so it stays a new board's
              // card until it is kept: its no-click restore runs (and misses)
              // there, and the row that needs a person is on the kept card.
              // The new board's card (`data-board-corner="blank"`,
              // `pending_board_card`) settled on core-only: its firmware bar
              // says "<version> · waiting for its firmware"
              // (`device_firmware_face.rs` `core_only_line`, the pending
              // firmware bar's summary).
              const pending = await driver.waitFor(
                `(() => { const card = JSON.parse(${snapshotExpr()}).board;
                          return card && card.corner === 'blank' && /waiting for its firmware/.test(card.firmware?.line ?? '') ? card.path : false; })()`,
                { timeoutMs: STEP_MS, what: "the new board's card to settle on core-only (… · waiting for its firmware)" },
              );
              await shot("cant-get-pending");
              // Set up this device: `adopt`, in the new board's hardware
              // details (`hardware_bar.rs` `pending_hardware_bar`).
              const kept = await driver.pressOffer("adopt", { board: pending, bar: "hardware", timeoutMs: STEP_MS });
              await driver.closeDetails({ board: pending }).catch(() => {});
              // The kept card's firmware bar: "Needs <X>, which Studio can't
              // get" (the update's NeedsYou line, `device_update_words.rs`).
              const order = await watchCard(async (snap, lines) => Boolean(lines["cant get"]), "Needs …, which Studio can't get", STEP_MS * 2);
              await shot("cant-get-row");
              // Install <Y>: the firmware bar's action (`install-firmware`,
              // one press with only this Studio's build to get; it arms,
              // since two dev builds have no order).
              const label = await pressUpdate("install-firmware");
              const rest = await watchCard(upToDate(), "up to date on Y");
              await settle(board, from);
              const said = boardSaid(board, from, OTA_WORDS);
              if (!said["engine committed"]) throw new Error("the board never committed Y's engine");
              return { summary: `pending core-only; pressed ${kept}; ${order.map((e) => e.kind).join(" → ")}; pressed ${label}; ${rest.map((e) => e.kind).join(" → ")}`, card: [...order, ...rest], board: said };
            });
          }
          break;
        }
        case "install-older": {
          if (TAB || BLE) throw new Error("install-older walks the door lane over ?emu= only");
          const r1 = JSON.parse(readFileSync(path.join(R1, "ota/ota-manifest.json"), "utf8"));
          const r2 = JSON.parse(readFileSync(path.join(R2, "ota/ota-manifest.json"), "utf8"));
          const store = await startReleaseStore(name, [
            { dir: R2, publishedAt: "2026-10-02T12:00:00Z" },
            { dir: R1, publishedAt: "2026-10-01T12:00:00Z" },
          ]);
          try {
            await openDoor(name, [`${board}=${r2Chip},kind=rom-up,${mac}`], store.origin);
            await step(name, `on ${r2.version}: Other version… lists the store's releases; ${r1.version}, typed in the box, arms and installs, then ${r2.version} installs at one click`, async () => {
              await nameThisBoard();
              await connect(board);
              // The board running its project: its project details offer
              // `remove-project`.
              await driver.waitOffer("remove-project", { board: await liveCardPath(), bar: "project", timeoutMs: STEP_MS });
              await driver.closeDetails({ board: here.path });

              // The list: the board's own drawn disabled, r1, this Studio's build.
              const list = await openOtherVersion(r1.version);
              if (!list.includes(r2.version) || !list.includes("On this board now")) throw new Error(`the list does not show ${r2.version} as the board's own: ${list}`);
              if (!(await optionDisabled(r2.version))) throw new Error(`${r2.version} is pickable, though the board runs it`);
              if (!list.includes("this Studio's build")) throw new Error(`the list does not offer this Studio's build: ${list}`);
              const listShot = await shot("install-older-list");

              // r1, typed whole into the box: the list narrows to it, picked;
              // older than the board's, so the press arms (Lasting).
              const from1 = boardWords(board).length;
              await driver.type("Type a version", r1.version, { scope: otherVersion() });
              const narrowed = await driver.waitFor(
                `(() => { const t = ${otherVersion()}?.innerText ?? '';
                          return t.includes(${JSON.stringify(r1.version)}) && !t.includes(${JSON.stringify(r2.version)}) ? t : false; })()`,
                { timeoutMs: STEP_MS, what: `the box to narrow the list to ${r1.version}` },
              );
              const typedShot = await shot("install-older-typed");
              // `install-firmware`, pressed: it arms; armed, the copy says what
              // changes; pressed again, it installs.
              const { armedShot } = await installAfterArming(r1.version, {
                shotName: "install-older-armed",
                check: (copy) => {
                  if (!copy.includes("Install an older version?")) throw new Error(`the armed press does not say what changes: ${copy}`);
                },
              });
              const toR1 = await awaitRelease(board, r1, from1);
              const r1Shot = await shot("install-older-on-r1");

              // r2: newer than the board's now, so one click.
              const from2 = boardWords(board).length;
              const again = await openOtherVersion(r2.version);
              if (!again.includes("On this board now")) throw new Error(`the list does not mark ${r1.version} as the board's own: ${again}`);
              if (!(await optionDisabled(r1.version))) throw new Error(`${r1.version} is pickable, though the board runs it`);
              await driver.click(r2.version, { scope: otherVersion() });
              await driver.pressOffer("install-firmware", { board: here.path, bar: "firmware", timeoutMs: STEP_MS });
              const armed = await driver.evaluate(`Boolean(${otherVersion()}?.querySelector('.ux-armed'))`);
              await driver.closeDetails({ board: here.path }).catch(() => {});
              if (armed) throw new Error(`the press on ${r2.version} armed: a newer version is one click`);
              const toR2 = await awaitRelease(board, r2, from2);
              const r2Shot = await shot("install-older-on-r2");

              const asked = store.upstreamHits.filter((url) => url.includes("releases.json")).length;
              if (asked < 1) throw new Error("lp-cloud-server never read the releases list");
              return {
                summary: `${toR1.id} armed, confirmed and installed (${toR1.order.map((e) => e.kind).join(" → ")}); ${toR2.id} at one click (${toR2.order.map((e) => e.kind).join(" → ")}); the project ran throughout`,
                shots: { list: listShot, typed: typedShot, armed: armedShot, onR1: r1Shot, onR2: r2Shot },
                narrowed: narrowed.replace(/\s+/g, " ").trim(),
                toR1: { core: toR1.core, card: toR1.order, board: toR1.said },
                toR2: { core: toR2.core, card: toR2.order, board: toR2.said },
                listsAsked: asked,
              };
            });
          } finally {
            store.stop();
          }
          break;
        }
        case "install-lookup": {
          if (TAB || BLE) throw new Error("install-lookup walks the door lane over ?emu= only");
          const r1 = JSON.parse(readFileSync(path.join(R1, "ota/ota-manifest.json"), "utf8"));
          const r2 = JSON.parse(readFileSync(path.join(R2, "ota/ota-manifest.json"), "utf8"));
          const store = await startReleaseStore(name, [
            { dir: R2, publishedAt: "2026-10-02T12:00:00Z" },
            { dir: R1, publishedAt: "2026-10-01T12:00:00Z", unlisted: true },
          ]);
          try {
            await openDoor(name, [`${board}=${r2Chip},kind=rom-up,${mac}`], store.origin);
            await step(name, `on ${r2.version}: ${r1.version} is not in the store's list; typed in the box, it is looked up by version, then arms and installs`, async () => {
              await nameThisBoard();
              await connect(board);
              // The board running its project: `remove-project` offered.
              await driver.waitOffer("remove-project", { board: await liveCardPath(), bar: "project", timeoutMs: STEP_MS });
              await driver.closeDetails({ board: here.path });

              // The list holds the board's own and this Studio's build, not r1.
              const list = await openOtherVersion(r2.version);
              if (list.includes(r1.version)) throw new Error(`${r1.version} is listed, though the store's list leaves it out: ${list}`);

              // r1, typed whole: nothing in the list matches, so the press
              // (`install-firmware`) looks it up.
              const from = boardWords(board).length;
              await driver.type("Type a version", r1.version, { scope: otherVersion() });
              await driver.waitFor(
                `(() => { const mark = ${otherVersion()}?.querySelector('[data-offer-path$="/install-firmware"]');
                          const buttons = mark ? mark.querySelectorAll('button') : [];
                          const press = buttons.length ? buttons[buttons.length - 1] : null;
                          return Boolean(press) && !press.disabled
                            && (press.textContent || '').includes(${JSON.stringify(`Look up ${r1.version}`)}); })()`,
                { timeoutMs: STEP_MS, what: `the press to read Look up ${r1.version}` },
              );
              const lookupShot = await shot("install-lookup-press");
              await driver.pressOffer("install-firmware", { board: here.path, bar: "firmware", timeoutMs: STEP_MS });

              // Found by the store's lookup: r1 joins the list, picked; older, so it arms.
              const found = await driver.waitFor(
                `(() => { const t = ${otherVersion()}?.innerText ?? ''; return t.includes('Install an older version?') ? t : false; })()`,
                { timeoutMs: STEP_MS, what: `the store to find ${r1.version} and the press to install it` },
              );
              const foundShot = await shot("install-lookup-found");
              // Found by the look-up, not the list: a looked-up release carries no
              // publish time, so its line is the version's day alone (the list's
              // would read "Oct 1, 12:00 UTC"). The upstream may not be asked at
              // all: the browser keeps an immutable manifest it read before.
              if (!found.includes(r1.version) || found.includes("12:00 UTC")) throw new Error(`${r1.version} is not the looked-up choice: ${found}`);
              await installAfterArming(r1.version);
              const toR1 = await awaitRelease(board, r1, from);
              const r1Shot = await shot("install-lookup-on-r1");
              return {
                summary: `${r1.version}, unlisted, looked up by version, armed, confirmed and installed (${toR1.order.map((e) => e.kind).join(" → ")}); the project ran throughout`,
                shots: { press: lookupShot, found: foundShot, onR1: r1Shot },
                found: found.replace(/\s+/g, " ").trim(),
                toR1: { core: toR1.core, card: toR1.order, board: toR1.said },
              };
            });
          } finally {
            store.stop();
          }
          break;
        }
        case "install-file": {
          if (TAB || BLE) throw new Error("install-file walks the door lane over ?emu= only");
          const r1 = JSON.parse(readFileSync(path.join(R1, "ota/ota-manifest.json"), "utf8"));
          const r2 = JSON.parse(readFileSync(path.join(R2, "ota/ota-manifest.json"), "utf8"));
          // Only r2 in the store: r1 comes from its files alone.
          const store = await startReleaseStore(name, [{ dir: R2, publishedAt: "2026-10-02T12:00:00Z" }]);
          try {
            await openDoor(name, [`${board}=${r2Chip},kind=rom-up,${mac}`], store.origin);
            await step(name, `on ${r2.version}: ${r1.version}'s ota files, picked with From a file…, are checked, listed and installed (armed)`, async () => {
              await nameThisBoard();
              await connect(board);
              // The board running its project: `remove-project` offered.
              await driver.waitOffer("remove-project", { board: await liveCardPath(), bar: "project", timeoutMs: STEP_MS });
              await driver.closeDetails({ board: here.path });
              const list = await openOtherVersion(r2.version);
              if (list.includes(r1.version)) throw new Error(`${r1.version} is listed before its files were picked: ${list}`);
              // From a file… (`install-firmware-file`), beside the press in
              // the same details.
              if (!(await driver.offered("install-firmware-file", { board: here.path, inDetails: true }))) {
                throw new Error(`the firmware details offer no From a file… (\`install-firmware-file\`): ${list}`);
              }

              // Pick r1's ota folder, as the file dialog would hand it over:
              // From a file…'s file input, in this board's card.
              const ota = path.join(R1, "ota");
              const picked = readdirSync(ota).map((file) => path.join(ota, file));
              const from = boardWords(board).length;
              await driver.setFiles(`[data-board-card="${here.path}"] input[id^="firmware-file-"]`, picked);
              const found = await driver.waitFor(
                `(() => { const t = ${otherVersion()}?.innerText ?? '';
                          return t.includes(${JSON.stringify(r1.version)}) && t.includes('from your files') ? t : false; })()`,
                { timeoutMs: STEP_MS, what: `${r1.version} from the files to join the list` },
              );
              if (!found.includes("Install a custom build?")) throw new Error(`the build from files does not say it is a custom build: ${found}`);
              const foundShot = await shot("install-file-listed");
              const { armedShot } = await installAfterArming(r1.version, { shotName: "install-file-armed" });
              const toR1 = await awaitRelease(board, r1, from);
              const r1Shot = await shot("install-file-on-r1");
              const fetched = store.upstreamHits.filter((url) => url.includes(`v${r1.version}/`)).length;
              if (fetched > 0) throw new Error(`the store was asked for ${r1.version}, which came from files`);
              return {
                summary: `${r1.version} from its ota files (${picked.length} picked), checked, armed, confirmed and installed (${toR1.order.map((e) => e.kind).join(" → ")}); the project ran throughout`,
                shots: { listed: foundShot, armed: armedShot, onR1: r1Shot },
                toR1: { core: toR1.core, card: toR1.order, board: toR1.said },
              };
            });
          } finally {
            store.stop();
          }
          break;
        }
        case "relay-drop": {
          await openDoor(name, [xBoard]);
          await step(name, "the relay drops the board mid-core: it dials again, the page redials, and the update finishes with no click", async () => {
            await nameThisBoard();
            await connect(board);
            // Ready on X, as core reads it.
            await driver.boardRuns({ board: await liveCardPath(), timeoutMs: STEP_MS });
            let legs = 0;
            let cutFrom = 0;
            const ran = await runUpdate(board, {
              cut: {
                stage: "updating",
                boardSays: /\[OTA\] offer \S+ → core/,
                atPercent: 40,
                act: async () => {
                  cutFrom = boardWords(board).length;
                  legs = cloud.cutDeviceLeg();
                  if (!legs) throw new Error("no device leg to cut: the board was not on the relay");
                  return `${legs} device leg${legs === 1 ? "" : "s"} cut`;
                },
              },
            });
            if (!ran.cutAt) throw new Error("the walk never found the moment to cut");
            await settle(board, ran.from);
            const said = boardSaid(board, ran.from, OTA_WORDS);
            for (const need of ["core offer", "core confirmed", "engine committed"]) {
              if (!said[need]) throw new Error(`the board never said ${need}`);
            }
            if (!said.resumed) throw new Error("the board never said it resumed the transfer");
            // Core-only says no heartbeat: its leg's own line is the proof.
            const back = boardWords(board).slice(cutFrom).match(/\[relay\] leg open to \S+/)?.[0] ?? null;
            if (!back) throw new Error("the board never said it reached the relay again after the cut");
            checkLinkWord(ran);
            const reconnects = checkWifiReconnects(await terminalLines(), 3);
            return {
              summary: `cut at ${ran.cutAt}; the board back on the relay; ${said.resumed}; up to date; ${reconnects.timed.length} reconnects timed`,
              card: ran.order, board: said, reconnects,
            };
          });
          break;
        }
        case "renumber": {
          await openDoor(name, [xBoard]);
          await step(name, "the board's next lease is a new address: the update's resets put it there, and the update finishes", async () => {
            await nameThisBoard();
            await connect(board);
            // Ready on X, as core reads it.
            await driver.boardRuns({ board: await liveCardPath(), timeoutMs: STEP_MS });
            const before = boardAddresses(board);
            const renumbered = await doorControl(door.addr, board, "renumber");
            const ran = await runUpdate(board);
            await settle(board, ran.from);
            const said = boardSaid(board, ran.from, OTA_WORDS);
            for (const need of ["core offer", "core confirmed", "engine committed"]) {
              if (!said[need]) throw new Error(`the board never said ${need}`);
            }
            checkLinkWord(ran);
            const after = boardAddresses(board, ran.from);
            const first = before[before.length - 1];
            const moved = after.filter((ip) => ip !== first);
            if (!first || !moved.length) {
              throw new Error(`the board never said a new address: before ${JSON.stringify(before)}, after ${JSON.stringify(after)}`);
            }
            const reconnects = checkWifiReconnects(await terminalLines(), 2);
            return {
              summary: `${first} → ${[...new Set(moved)].join(", ")} across the update's resets; up to date; ${reconnects.timed.length} reconnects timed (through the board's forward, which follows it)`,
              renumbered, before, after, card: ran.order, board: said, reconnects,
            };
          });
          break;
        }
        case "second-client": {
          await openDoor(name, [xBoard]);
          await step(name, "mid-update, lp-cli and a second Studio are turned away busy; the update finishes", async () => {
            await nameThisBoard();
            await connect(board);
            // Ready on X, as core reads it.
            await driver.boardRuns({ board: await liveCardPath(), timeoutMs: STEP_MS });
            let lpCliRefusal = null;
            let studioSaid = null;
            let boardSaidBusy = null;
            let second = null;
            const ran = await runUpdate(board, {
              cut: {
                stage: "updating",
                boardSays: /\[OTA\] offer \S+ → core/,
                atPercent: 20,
                act: async () => {
                  const busyFrom = boardWords(board).length;
                  const refused = await lpCli(["wifi", "status", `lan:${door.forward}`, "--json"], { timeoutMs: 120_000 });
                  if (refused.code === 0 || !refusedAsInUse(refused.stderr)) {
                    throw new Error(`lp-cli was not turned away busy: exit ${refused.code}: ${refused.stderr.trim().split("\n").slice(-2).join(" | ")}`);
                  }
                  lpCliRefusal = refused.stderr.trim().split("\n").slice(-1)[0];
                  second = await StudioDriver.launch({ width: 1100, height: 900 });
                  await second.navigate(`http://localhost:${studioPort}/`);
                  await second.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: LOAD_MS, what: "the second Studio to load" });
                  await openNetworkRow(second, { timeoutMs: STEP_MS });
                  const typed = await second.evaluate(typeAddress(door.forward));
                  if (typed !== "typed") throw new Error(`the second page's address field: ${typed}`);
                  await second.waitFor(PRESS_ADDRESS_CONNECT, { timeoutMs: 30_000, what: "the second page's Connect" });
                  studioSaid = await second.waitFor(
                    `(() => { const t = ${ADDRESS_STATUS}; return t.includes(${JSON.stringify(BUSY_WORDS)}) ? t : false; })()`,
                    { timeoutMs: STEP_MS, what: "the second Studio to say the board is busy" },
                  );
                  try {
                    await second.screenshot(path.join(shots, "second-studio-busy.png"));
                  } catch {
                    /* the page may be gone */
                  }
                  await second.close();
                  second = null;
                  boardSaidBusy = await waitBoard(board, /\[lan\] every LAN link is in use[^\n]*/, "it turned a second link away", STEP_MS, busyFrom);
                  return "lp-cli and a second Studio turned away";
                },
              },
            });
            if (second) await second.close();
            if (!ran.cutAt) throw new Error("the walk never reached the moment to try a second client");
            await settle(board, ran.from);
            const said = boardSaid(board, ran.from, OTA_WORDS);
            for (const need of ["core offer", "core confirmed", "engine committed"]) {
              if (!said[need]) throw new Error(`the board never said ${need}`);
            }
            checkLinkWord(ran);
            return {
              summary: `at ${ran.cutAt}: lp-cli "${lpCliRefusal}"; the second Studio "${studioSaid}"; the board "${boardSaidBusy}"; up to date`,
              lpCliRefusal, studioSaid, boardSaidBusy, card: ran.order, board: said,
            };
          });
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
            await nameThisBoard();
            await connect(board);
            // Ready, as core reads it.
            await driver.boardRuns({ board: await liveCardPath(), timeoutMs: STEP_MS });
            // Every verb the card draws for it, on its face and in its
            // firmware details. Over the air is `install-firmware` (Other
            // version…), `reinstall-firmware`, `install-firmware-file`, or
            // an `update-firmware` that is Routine; today's USB flash is
            // published at the same `update-firmware` and is Lasting (or a
            // board pick) — `device_update_offers.rs`, `firmware_bar.rs`
            // `usb_update`.
            const offers = await cardOffers();
            const OVER_THE_AIR = ["install-firmware", "reinstall-firmware", "install-firmware-file"];
            const overTheAir = offers.filter((offer) => OVER_THE_AIR.includes(offer.verb) || (offer.verb === "update-firmware" && !offer.arms && !offer.pick));
            const controls = offers.map((offer) => `${offer.words} (${offer.verb}${offer.arms ? ", Lasting" : ""}${offer.pick ? ", a pick" : ""}, ${offer.where})`);
            if (overTheAir.length) {
              throw new Error(`a pre-update board was offered ${overTheAir.map((offer) => `"${offer.words}" (\`${offer.verb}\`, ${offer.where})`).join(", ")}`);
            }
            const words = boardWords(board);
            if (/\[OTA\]/.test(words)) throw new Error("a single image spoke the update protocol");
            const flash = offers.find((offer) => offer.verb === "flash" || (offer.verb === "update-firmware" && (offer.arms || offer.pick))) ?? null;
            return { summary: `no over-the-air offer; ${flash ? `today's "${flash.words}" (\`${flash.verb}\`, the USB flash)` : "no flash offered (this Studio's version)"}`, controls };
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
  if (recorder) {
    writeFileSync(path.join(out, "records.jsonl"), recorder.raw.join("\n"));
    recorder.server.close();
  }

  console.log("\n=== the over-the-air update walk");
  for (const s of report.steps) {
    console.log(`  ${s.skipped ? "–" : s.ok ? "✓" : "✗"} ${s.name.padEnd(22)} ${s.skipped ?? s.summary ?? s.error ?? ""}`);
  }
  console.log(`\n  lp-emu ${lpEmu} (lp-emu:esp32c6:t1${BLE ? ", over ?ble=emu: emulated times, not Bluetooth ones" : LAN_LANE ? "+net=lan: emulated times, not a radio's" : RELAY_LANE ? "+net=lan through a local relay: emulated times, not the internet's" : ""}); report → ${path.relative(ROOT, path.join(out, "walk-ota-emu.json"))}`);

  await driver.close();
  if (hold) await releaseConsole(hold);
  if (door) await stopDoor(door);
  bundle.close();
  store.close();
  if (cloud) {
    writeFileSync(path.join(out, "relay-cloud.log"), cloud.log());
    cloud.stop();
  }

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
function seedChip(image, chip, emulated = "60s", requests = []) {
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
      "--request", JSON.stringify({ filesystem: { write: { path: "/.lp/device.json", data: JSON.stringify({ uid: "dev00000000000000a1", name: "Walk board" }) } } }),
      ...requests.flatMap((request) => ["--request", request])],
    { cwd: ROOT, stdio: ["ignore", "ignore", "inherit"] },
  );
}

/// The Network row's address field and its Connect (the `studio-lan` walk's
/// selectors), and what it says under them. The row opens when the Network
/// square is pressed (`openNetworkRow`).
const ADDRESS_FIELD = `document.querySelector('#main input[placeholder^="192.168.1.40"]')`;
const ADDRESS_ENTRY = `${ADDRESS_FIELD}?.closest('label')?.parentElement?.parentElement`;
const ADDRESS_STATUS = `(${ADDRESS_ENTRY}?.querySelector('[role="status"]')?.innerText || '')`;
const PRESS_ADDRESS_CONNECT = `(() => {
  const entry = ${ADDRESS_ENTRY};
  const button = [...(entry?.querySelectorAll('button') ?? [])].find((b) => !b.disabled && (b.innerText || '').trim() === 'Connect');
  if (!button) return false;
  button.click();
  return true;
})()`;
/// Studio's busy words (`lpa-studio-core`'s `WIFI_BUSY_WORDS`).
const BUSY_WORDS = "Busy with another connection \u2014 try again";

/// Type `text` into the Network row's address field, as a keyboard does for
/// Dioxus (`input` events carry the value).
function typeAddress(text) {
  return `(() => {
    const field = ${ADDRESS_FIELD};
    if (!field) return 'no field';
    field.focus();
    field.value = ${JSON.stringify(text)};
    field.dispatchEvent(new Event('input', { bubbles: true }));
    return 'typed';
  })()`;
}

function hex(n) {
  return `0x${n.toString(16)}`;
}

await main();
