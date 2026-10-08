#!/usr/bin/env node
// THE EMULATED WI‑FI WALK: Studio over the LAN, two boards on one LAN
// (Wi‑Fi plan P13; `just walk-wifi-emu lan`).
//
// Two emulated ESP32-C6 boards, `c6-a` and `c6-b`, running the packaged
// firmware ROM-up, on ONE virtual LAN (`lan=home`) whose access points come
// from a fixture this walk writes. Real Studio, headless, reaches them over
// that LAN through each board's port forward (`?lan=`), the way it would
// reach a board on a desk's network:
//
//   W1  over each board's USB door, add the fixture's network
//       → each board's status: connected, an address, its `.local` name
//   W2  Studio with ?lan=<fwd a>,<fwd b> → two Wi‑Fi boards, each saying
//       hello over its secure LAN link (the board's console: the handshake)
//   W3  push a project to c6-a over the LAN, open it, edit a value
//       → the board's console: the load; its frame counter advances
//   W4  c6-b's card while c6-a stays connected → both links stay up
//   W5  the LAN probe asks for `_lightplayer._tcp` → both boards answer,
//       each with its own name and MAC
//   W6  a network with a wrong password on c6-b, from Studio over c6-b's
//       USB door (a LAN card has no Wi‑Fi row yet, below) → "Wrong
//       password"; the board goes back to the good network
//   W7  a name no access point has → "Not in range"
//   W8  a project with a Radio node on c6-a → the node says why it is off
//       (fw-esp32-common's `RADIO_OFF_FOR_WIFI`); the rest runs
//   W9  reset c6-a with the gateway's next-lease option on → a different
//       address; Studio's link comes back through the same forward
//   W10 `lp-cli link rtt lan:<fwd a>` → request p50/p90 in FRAMES (run
//       before W9, so the board it measures has not just been reset)
//
// THE BOARD'S WORDS DECIDE EVERY STEP (AGENTS.md: "wait for the board's
// words, not Studio's"). Three sources, none of which the page can satisfy:
//
//   * the board's console: each board's USB link held for the whole walk by
//     `lp-cli link capture` (through a TCP bridge to the door's `/bytes`),
//     which writes the DECODED console — log records ride the link, so the
//     door's raw console file holds none of them;
//   * the board's status answers: `lp-cli wifi status` over its USB door
//     (W1, before the capture takes the port; W4 and W9, the capture
//     handing the port over and taking it back, `usbStatus`) or over its
//     LAN forward while no Studio page holds that board's LAN link (W6,
//     W7);
//   * the LAN's own view: the door's `/boards` and its LAN probe.
//
// ONE LAN LINK PER BOARD (PR B, `fw-esp32c6`'s `LAN_LINK_SLOTS`, 1 on the
// C6): a second LAN connection to a board whose link is open is closed
// with "try again later" (WebSocket 1013; the board logs `[lan] every LAN
// link is in use`). So nothing of the walk's own dials a board's forward
// while the Studio page holds that board: W4 reads c6-b's status over its
// USB door (and checks that a second LAN dial is turned away while
// Studio's link stays up), and the steps whose tools dial the LAN (W6, W7's
// status reads, W8's upload, W10's `link rtt`) run with the LAN page off
// (`about:blank`, each board's console saying its link closed), Studio
// coming back for what it shows (W8's Radio message, W9's relink). Two
// boards, one link each, is the walk's shape throughout.
//
// Studio's words only say when to look. One exception, said where it is
// used: W8's Radio message is a string only the firmware holds, so the page
// showing it IS the board's answer relayed.
//
// The interfaces this walk drives were written ahead of the code that
// provides them (P12's hosts, built in parallel). The door's shapes (the
// fixture, `--lan`/`lan=`, `/boards`' `forward`, `renumber`, the browse) are
// reconciled with `lp-cli emu serve` as built and say "As built:"; every
// guess still standing is a `// ASSUMES:` comment.
//
// NOT CI (P13 §1). Made-up test values only. Headless Chrome only. It serves
// the release Studio bundle itself on this worktree's stable slot (no dev
// server, nothing adopted) and runs as one foreground command. Needs:
// `just studio-web-story-build`, `just studio-firmware-package-served`,
// `cargo build -p lp-cli`, Chrome. `--dry-run` checks the arguments and the
// prerequisites, writes the fixture and the plan, and starts nothing.
//
// Emulated numbers it prints name the board's configuration label and the
// `lp-emu` commit. A wall-clock figure from a socket is never a gate.

import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { findChrome, StudioDriver } from "./studio-driver.mjs";
import {
  PACKAGED_C6_MERGED,
  RELEASE_BUNDLE,
  SERVED_FIRMWARE,
  boardRegistry,
  bridgeDoorBytes,
  serveStudioBundle,
  startDoor,
  startRecordSink,
  stopDoor,
  walkPort,
} from "./emulated-lane.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const LP_CLI = path.join(ROOT, "target/debug/lp-cli");

export const LAN = "home";
const BOARDS = ["c6-a", "c6-b"];
const [A, B] = BOARDS;

/// TEST VALUES ONLY: committed, printed, and named in the emulator's output.
export const NET = { ssid: "lp-walk-net", password: "correct-horse-42", dbm: -50 };
/// In range, secured; W6 adds it with the wrong password.
const GUEST = { ssid: "lp-walk-guest", password: "staple-battery-7", wrong: "wrong-horse-0", dbm: -65 };
/// No access point has it (W7).
const NOWHERE = { ssid: "lp-walk-nowhere", password: "no-such-net-1" };

/// The virtual LAN's access points.
/// As built: `emu serve --lan <name>=<fixture>` reads P11's fixture format
/// (`lp-emu/esp/lp-emu-esp-common/testdata/virtual_lan.toml`; parser
/// `lp-cli/src/commands/emu/lan_fixture.rs`): one `[[access_point]]` per
/// network, `name`, `password` (absent = open), `signal_dbm`, `hidden`; any
/// other key is refused.
export const FIXTURE = `# walk-wifi-emu-lan.mjs: made-up test values only, never a real network.
[[access_point]]
name = "${NET.ssid}"
password = "${NET.password}"
signal_dbm = ${NET.dbm}

[[access_point]]
name = "${GUEST.ssid}"
password = "${GUEST.password}"
signal_dbm = ${GUEST.dbm}
`;

/// W3's project, from Studio's gallery (walk-no-board's, for its measured
/// reason: `fyeah-sign` overruns the emulated watchdog).
const WALK_PROJECT = process.env.WALK_PROJECT ?? "Peach (1D)";
/// W8's project: a Radio node (`radio:local:0`) beside a shader. Not
/// `fyeah-sign` (the emulated graphics-stage defect, as above).
const RADIO_PROJECT = process.env.WALK_RADIO_PROJECT ?? "projects/test/button-sign";
/// fw-esp32-common `net::radio_rule::RADIO_OFF_FOR_WIFI`, verbatim (P03).
/// Nothing in Studio holds this string: on the page it can only have come
/// from the board.
const RADIO_OFF_FOR_WIFI =
  "Radio is off while this board uses Wi-Fi. Turn Wi-Fi off for this board to use Radio.";

/// Studio's words (`lpa-studio-core/src/app/network/wifi_words.rs`). They
/// say WHEN to look; the board's status says what happened.
export const WORDS = {
  wifiRow: "Wi‑Fi", // U+2011, the card's Wi‑Fi row
  wifiLine: "Wi‑Fi · ", // U+2011, the card's LAN line (`UiLanLink::line`, `UiLinkKind::Wifi`'s word)
  connectToANetwork: "Connect to a network",
  otherNetwork: "Other network",
  connect: "Connect",
  checkingPassword: "Checking the password",
  wrongPassword: "Wrong password",
  notInRange: "Not in range",
  cloudRelay: "Cloud relay",
};

export const STUDIO_LOAD_MS = 420_000;
/// Wedged-run deadlines, never measurements: emulated boards boot, scan,
/// join and render at emulated speed on a shared box.
export const STEP_MS = 300_000;
const JOIN_MS = 420_000;
const STATUS_POLL_MS = 2_000;
/// W10: requests timed. Enough for a p90 to mean something.
const RTT_COUNT = 40;

export const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;
export const PANEL = `document.querySelector('[id^="ux-popover-panel"]')`;
export const PANEL_TEXT = `(${PANEL}?.innerText || '')`;

// --- arguments -------------------------------------------------------------

function usage() {
  return (
    "usage: node scripts/emu/walk-wifi-emu.mjs lan [--out <dir>] [--keep-open] [--dry-run] [--skip W10,…]\n" +
    "       node scripts/emu/walk-wifi-emu-lan.mjs [--out <dir>] [--keep-open] [--dry-run]"
  );
}

export function parseArgs(argv) {
  const rest = argv[0] === "lan" ? argv.slice(1) : argv;
  const options = { out: path.join(ROOT, "target/walk-wifi-emu/lan"), keepOpen: false, dryRun: false, skip: [] };
  for (let i = 0; i < rest.length; i += 1) {
    const arg = rest[i];
    if (arg === "--out") {
      const value = rest[++i];
      if (!value) throw new Error(`--out needs a directory\n${usage()}`);
      options.out = path.resolve(value);
    } else if (arg === "--keep-open") options.keepOpen = true;
    else if (arg === "--dry-run") options.dryRun = true;
    // `--skip W10`: leave named steps out (W10's `link rtt` alone can run
    // past a session's 10-minute command cap). A skipped step is reported
    // as skipped, never as passed.
    else if (arg === "--skip") {
      const value = rest[++i];
      if (!value) throw new Error(`--skip needs step ids (W10 or W3,W10)\n${usage()}`);
      options.skip.push(...value.split(",").map((id) => id.trim().toUpperCase()).filter(Boolean));
    }
    else if (arg === "--help" || arg === "-h") {
      options.help = true;
    } else {
      throw new Error(`unknown argument ${JSON.stringify(arg)}\n${usage()}`);
    }
  }
  return options;
}

// --- the walk's own plumbing ---------------------------------------------

/// What this run needs on disk, and whether it is there.
export function prerequisites() {
  return [
    ["a debug lp-cli (cargo build -p lp-cli)", LP_CLI],
    ["the release Studio bundle (just studio-web-story-build)", path.join(ROOT, RELEASE_BUNDLE)],
    ["the packaged firmware (just studio-firmware-package-served)", path.join(ROOT, SERVED_FIRMWARE)],
    ["the packaged merged C6 image (just studio-firmware-package-served)", path.join(ROOT, PACKAGED_C6_MERGED)],
    ["the Radio project", path.join(ROOT, RADIO_PROJECT)],
  ].map(([what, at]) => ({ what, at, present: existsSync(at) }));
}

/// The `emu serve` boards and flags for this walk.
///
/// As built (P12 §3): a LAN is declared once with `--lan
/// <name>=<fixture.toml>`, and a board joins it with `,lan=<name>` in its
/// `--board` spec. Boards naming the same LAN share it; each board's MAC is
/// its own by default (`02:4c:50:00:00:<seat>`).
export function doorSpec(fixturePath) {
  return {
    boards: BOARDS.map((id) => `${id}={merged},kind=rom-up,lan=${LAN}`),
    extraArgs: ["--lan", `${LAN}=${fixturePath}`],
  };
}

/// A board's forward, `127.0.0.1:<port>`, out of its `/boards` entry.
///
/// As built: each `/boards` entry carries `lan` (the LAN's name),
/// `forward` (`lan:127.0.0.1:<port>`) and `address` (the board's LAN
/// address once DHCP bound one, else null), all null for a board on no
/// served LAN. The thrown message carries the entry.
export function forwardOf(entry) {
  const match = typeof entry?.forward === "string" ? entry.forward.match(/^lan:(127\.0\.0\.1:\d+)$/) : null;
  if (match) return match[1];
  throw new Error(`no LAN forward in the door's entry for ${entry?.id}: ${JSON.stringify(entry)}`);
}

/// Studio on the walk's own bundle, reaching both boards over the LAN and
/// streaming its device events to the sink. No `?emu=`: nothing in this
/// page touches a USB door or a control channel, so the walk's own tools
/// can hold them (W9's `renumber`/`reset` need the control channel, and the
/// shim holds every board's while its page is open).
export function studioUrlForLan({ studioPort, forwards, sinkUrl, route = "/devices" }) {
  const query = new URLSearchParams();
  query.set("lan", forwards.map((forward) => `ws://${forward}/link`).join(","));
  query.set("record", sinkUrl);
  return `http://localhost:${studioPort}${route}?${query.toString()}`;
}

/// The second page W6 and W7 use: Studio over the door's USB shim
/// (`?emu=`), in its own browser.
///
/// As built (P07), a board reached on the LAN wears no Connections group,
/// so its card has no Wi‑Fi row (`device_roster_card.rs`: "the network card
/// is M8's"); the Wi‑Fi panel and its in-row test are reachable over USB
/// alone. The plan allows either ("Studio, over USB or LAN"). A separate
/// browser rather than `?emu=` on the LAN page: the shim opens every
/// board's control channel while its page is up (its banner's detach /
/// attach / power), which would lock W9 out of `renumber` and `reset`; and
/// the LAN page keeps its own view of c6-b, whose LAN link drops while the
/// board tries the new networks.
export function studioUrlForUsb({ studioPort, doorAddr, sinkUrl, route = "/devices" }) {
  const query = new URLSearchParams();
  query.set("emu", `ws://${doorAddr}`);
  query.set("record", sinkUrl);
  return `http://localhost:${studioPort}${route}?${query.toString()}`;
}

/// Run `lp-cli <args>`; a password goes in on stdin, never in argv.
export function lpCli(args, { stdin = null, timeoutMs = STEP_MS, env = {} } = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(LP_CLI, args, { cwd: ROOT, env: { ...process.env, ...env }, stdio: ["pipe", "pipe", "pipe"] });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk) => {
      stdout += chunk;
    });
    child.stderr.on("data", (chunk) => {
      stderr += chunk;
    });
    const timer = setTimeout(() => {
      child.kill("SIGTERM");
      reject(new Error(`lp-cli ${args.join(" ")} did not finish within ${timeoutMs / 1000} s:\n${stderr.slice(-2000)}`));
    }, timeoutMs);
    child.on("error", (error) => {
      clearTimeout(timer);
      reject(error);
    });
    child.on("exit", (code) => {
      clearTimeout(timer);
      resolve({ code, stdout, stderr });
    });
    if (stdin !== null) child.stdin.write(`${stdin}\n`);
    child.stdin.end();
  });
}

/// The last JSON object `lp-cli … --json` printed.
function lastJson(stdout) {
  const lines = stdout.split("\n").filter((line) => line.trim().startsWith("{"));
  if (!lines.length) throw new Error(`no JSON in:\n${stdout.slice(-1000)}`);
  return JSON.parse(lines[lines.length - 1]);
}

/// The board's own status answer over `target`.
export async function wifiStatus(target) {
  const { code, stdout, stderr } = await lpCli(["wifi", "status", target, "--json"], { timeoutMs: 120_000 });
  if (code !== 0) throw new Error(`wifi status ${target} → exit ${code}: ${stderr.trim().split("\n").slice(-3).join(" | ")}`);
  return lastJson(stdout);
}

/// A second LAN dial turned away by a board whose one LAN link is in use:
/// lp-cli's words for WebSocket 1013 (`lpa-client`'s `lan_error.rs`; since
/// #999 "busy with another connection — try again later").
export function refusedAsInUse(stderr) {
  return /busy with another connection \u2014 try again later/.test(stderr);
}

/// `station` as `{ kind, ...fields }` (`"notConnected"` or `{ connected: {…} }`).
export function stationOf(status) {
  const station = status?.station;
  if (typeof station === "string") return { kind: station };
  if (station && typeof station === "object") {
    const [kind] = Object.keys(station);
    return { kind, ...station[kind] };
  }
  return { kind: "unknown" };
}

/// A saved network's `last` attempt, as the board reports it.
function lastAttempt(status, ssid) {
  return (status?.networks ?? []).find((network) => network.ssid === ssid)?.last ?? null;
}

/// Ask the board until `test(status)` holds. Failures to reach it are
/// expected while it is off its network (W6, W7, W9) and are retried; the
/// last one is said if the deadline passes.
export async function awaitStatus(target, test, { timeoutMs = JOIN_MS, what }) {
  const deadline = Date.now() + timeoutMs;
  let last = null;
  for (;;) {
    try {
      const status = await wifiStatus(target);
      last = status;
      if (test(status)) return status;
    } catch (error) {
      last = error.message;
    }
    if (Date.now() > deadline) {
      throw new Error(`the board never said ${what} over ${target}; last: ${typeof last === "string" ? last : JSON.stringify(last)}`);
    }
    await delay(STATUS_POLL_MS);
  }
}

export const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

/// One board's decoded console, as `lp-cli link capture` writes it a line
/// at a time. Waits read the file; the board writing it is the event.
///
/// As built (seen on the first run): with the network seam answering (P10)
/// the firmware logs the lines this walk keys on from code above the frame
/// device, so the seam and the radio print the same words:
/// `[wifi] trying <ssid>` and `[wifi] address a.b.c.d`
/// (`fw-esp32c6/src/net/station_task.rs`), `[lan] link <id> from <peer>:
/// secure session opening (…)` and `[lan] link <id>: closed (…)`
/// (`lan_endpoint_task.rs`; the id is a `LinkId`'s Display, `link1`, not a
/// bare number), and `Project loaded: <name>` (`lpa-server`'s
/// `project_manager.rs`, as walk-no-board reads it).
///
/// A console is one file per capture: when the walk lets go of a board's USB
/// link to ask it something over that door (`usbStatus`) and takes it back,
/// the new capture writes a new file, and the text is all of them in order,
/// so a mark taken before the hand-over still points at the same line.
export class BoardConsole {
  constructor(board, file) {
    this.board = board;
    this.files = [file];
  }

  get file() {
    return this.files[this.files.length - 1];
  }

  /// The next capture's file: `<id>.link.<n>.log` beside the first.
  nextFile() {
    const next = this.files[0].replace(/\.link\.log$/, `.link.${this.files.length + 1}.log`);
    this.files.push(next);
    return next;
  }

  text() {
    return this.files
      .map((file) => {
        try {
          return readFileSync(file, "utf8");
        } catch {
          return "";
        }
      })
      .join("");
  }

  /// A position to read "since" from.
  mark() {
    return this.text().length;
  }

  since(mark) {
    return this.text().slice(mark);
  }

  /// The first line from `mark` on that `match` (a string, a RegExp or a
  /// predicate) finds.
  async waitFor(match, { from = 0, timeoutMs = STEP_MS, what }) {
    const test =
      typeof match === "function"
        ? match
        : typeof match === "string"
          ? (line) => line.includes(match)
          : (line) => match.test(line);
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const hit = this.since(from).split("\n").find(test);
      if (hit) return hit;
      if (Date.now() > deadline) throw new Error(`${this.board}'s console never said ${what} (${this.file})`);
      await delay(500);
    }
  }

  /// The heartbeats' `frame_count`s from `mark` on.
  frameCounts(from = 0) {
    return [...this.since(from).matchAll(/"heartbeat":\{[^\n]*?"frame_count":(\d+)/g)].map((m) => Number(m[1]));
  }

  /// The last heartbeat's `loaded_projects` from `mark` on.
  loadedProjects(from = 0) {
    const all = [...this.since(from).matchAll(/"loaded_projects":(\[[^\]]*\])/g)];
    return all.length ? all[all.length - 1][1] : null;
  }

  /// Wait for `count` more heartbeats from `mark` on, with a frame counter
  /// that moved between the first and the last.
  async framesAdvance({ from, count = 2, timeoutMs = STEP_MS }) {
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const counts = this.frameCounts(from);
      if (counts.length >= count && counts[counts.length - 1] > counts[0]) return counts;
      if (Date.now() > deadline) {
        throw new Error(`${this.board}'s frame counter did not advance over ${count} heartbeats: ${JSON.stringify(counts)}`);
      }
      await delay(1_000);
    }
  }
}

/// Hold a board's USB link for the rest of the walk and write its decoded
/// console: a TCP bridge to the door's `/bytes`, and `lp-cli link capture`
/// on it.
///
/// `console` continues an earlier hold's console in a new file (`usbStatus`).
export async function holdConsole({ doorAddr, board, file, console: continued = null }) {
  const boardConsole = continued ?? new BoardConsole(board, file);
  const target = continued ? continued.nextFile() : file;
  const bridge = await bridgeDoorBytes({ doorAddr, board });
  const child = spawn(
    LP_CLI,
    ["link", "capture", `tcp://127.0.0.1:${bridge.port}`, "--console", target, "--seconds", "14400", "--json-replies"],
    { cwd: ROOT, stdio: ["ignore", "ignore", "pipe"] },
  );
  let stderr = "";
  child.stderr.on("data", (chunk) => {
    stderr = (stderr + chunk).slice(-4000);
  });
  return { board, bridge, child, stderr: () => stderr, console: boardConsole };
}

/// Let go of a board's USB link: stop its capture and its bridge, and wait
/// for the capture to exit so the door is free for the next client.
export async function releaseConsole(hold) {
  if (hold.released) return;
  hold.released = true;
  const exited = new Promise((resolve) => {
    if (hold.child.exitCode !== null) resolve();
    else hold.child.once("exit", resolve);
  });
  hold.child.kill("SIGINT");
  await Promise.race([exited, delay(15_000)]);
  await new Promise((resolve) => hold.bridge.server.close(() => resolve()));
}

/// A door control line (`/board/<id>/control`), and its one reply line.
///
/// As built: the control channel takes `renumber`, "this board's next DHCP
/// lease is a different address" (`VirtualLan::renumber_next_lease`), the
/// one verb the door answers itself, in its place in the reply order:
/// `ok renumber lan=<name> board=<id> …`, or `err renumber: …` for a board
/// on no served LAN.
export async function control(doorAddr, board, line) {
  const ws = new WebSocket(`ws://${doorAddr}/board/${board}/control`);
  const reply = await new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error(`no reply to \`${line}\` on ${board}'s control channel`)), 30_000);
    ws.addEventListener("open", () => ws.send(`${line}\n`));
    ws.addEventListener("message", (event) => {
      clearTimeout(timer);
      resolve(String(event.data).trim());
    });
    ws.addEventListener("error", () => {
      clearTimeout(timer);
      reject(new Error(`${board}'s control channel refused the connection`));
    });
  });
  ws.close();
  if (!reply.startsWith("ok")) throw new Error(`\`${line}\` on ${board}: ${reply}`);
  return reply;
}

/// The LAN probe's answer to a DNS-SD browse, as the door relays it.
///
/// As built (`lp-cli/src/commands/emu/serve/lan_browse.rs`): the door
/// answers `GET /lans/<name>/browse?service=_lightplayer._tcp.local[&wait_ms=N]`
/// once every running board on the LAN has answered with its instance and
/// TXT (or the wait, 10 s by default, is up) with
/// `{ lan, service, waited_ms, instances: [{ instance, host, port, address,
/// txt, mac }], answers: [{ name, type, ttl, data }] }`; `mac` is the TXT
/// record's `mac=<12 hex>` (the firmware's TXT,
/// `fw-esp32-common/src/net/mdns/mdns_answer.rs`). Read loosely: the walk
/// looks for each board's MAC and a distinct instance name per board.
async function probeBrowse(doorAddr) {
  const url = `http://${doorAddr}/lans/${LAN}/browse?service=${encodeURIComponent("_lightplayer._tcp.local")}`;
  const response = await fetch(url, { signal: AbortSignal.timeout(60_000) });
  if (!response.ok) throw new Error(`GET ${url} → ${response.status} (the door's LAN browse: is --lan ${LAN}=… declared?)`);
  return response.json();
}

/// The lp-emu commit numbers are quoted against (AGENTS.md: name it inline).
export function lpEmuCommit() {
  try {
    const sha = execFileSync("git", ["log", "-1", "--format=%h", "--", "lp-emu"], { cwd: ROOT, encoding: "utf8" }).trim();
    const dirty = execFileSync("git", ["status", "--porcelain", "--", "lp-emu"], { cwd: ROOT, encoding: "utf8" }).trim();
    return dirty ? `${sha}+dirty` : sha;
  } catch {
    return "unknown";
  }
}

// --- the page ----------------------------------------------------------------

/// The card of the Wi‑Fi board Studio reached at `forward`: the element
/// holding its "Wi-Fi · <address>" line, widened until it is one card among
/// several (or the whole list, when it is the only one). Recomputed on every
/// use: Dioxus may replace the nodes between two looks.
export function cardOf(forward) {
  return `(() => {
    const re = new RegExp(${JSON.stringify(escapeRegExp(`${WORDS.wifiLine}${forward}`))} + '(?!\\\\d)');
    const main = document.querySelector('#main');
    if (!main) return null;
    const hits = [...main.querySelectorAll('*')].filter((el) => re.test(el.textContent || ''));
    const leaf = hits.find((el) => ![...el.children].some((c) => re.test(c.textContent || '')));
    if (!leaf) return null;
    let el = leaf;
    while (el.parentElement && el.parentElement !== main) {
      const cards = [...el.parentElement.children].filter((c) => (c.textContent || '').includes(${JSON.stringify(WORDS.wifiLine)}));
      if (cards.length > 1) return el;
      el = el.parentElement;
    }
    return el;
  })()`;
}

function escapeRegExp(text) {
  return text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

export const cardText = (forward) => `(${cardOf(forward)}?.innerText || '')`;

/// The one Wi‑Fi row on the page, a button whose text starts "Wi‑Fi": the
/// USB card's (a LAN card has none, as built).
const WIFI_ROW_ANYWHERE = `[...(document.querySelector('#main')?.querySelectorAll('button') ?? [])].find((b) => (b.innerText || '').trim().startsWith(${JSON.stringify(WORDS.wifiRow)}))`;

/// Type `text` into the panel's input `selector`, the way a keyboard does
/// for Dioxus (`input` events carry the value).
export function typeInto(selector, text) {
  return `(() => {
    const el = ${PANEL}?.querySelector(${JSON.stringify(selector)});
    if (!el) return false;
    el.focus();
    el.value = ${JSON.stringify(text)};
    el.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  })()`;
}

export class Page {
  constructor(driver) {
    this.driver = driver;
  }

  async load(url) {
    await this.driver.navigate(url);
    await this.driver.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: STUDIO_LOAD_MS, what: "Studio to finish loading" });
  }

  /// Wait for a control in `forward`'s card, then click it.
  async clickInCard(forward, text, { exact = false } = {}) {
    await this.driver.waitFor(
      `(() => { const c = ${cardOf(forward)}; if (!c) return false;
                return [...c.querySelectorAll('button, [role="button"], a')].some((el) => !el.disabled &&
                  (el.textContent || '').replace(/\\s+/g, ' ').trim().toLowerCase().includes(${JSON.stringify(text.toLowerCase())})); })()`,
      { timeoutMs: STEP_MS, what: `${JSON.stringify(text)} in the card at ${forward}` },
    );
    return this.driver.click(text, { scope: cardOf(forward), exact });
  }

  async cardSays(forward, words, what) {
    return this.driver.waitFor(`${cardText(forward)}.includes(${JSON.stringify(words)})`, {
      timeoutMs: STEP_MS,
      what: what ?? `the card at ${forward} to say ${JSON.stringify(words)}`,
    });
  }

  /// The first of `words` the card says (a card whose board runs a
  /// degraded project, W8's Radio node, says "Degraded" where an idle one
  /// says "Ready"; both mean the link is up and the board answered).
  async cardSaysAny(forward, words, what) {
    const found = await this.driver.waitFor(
      `(() => { const t = ${cardText(forward)}; return ${JSON.stringify(words)}.find((w) => t.includes(w)) ?? false; })()`,
      { timeoutMs: STEP_MS, what: what ?? `the card at ${forward} to say one of ${JSON.stringify(words)}` },
    );
    return found;
  }

  /// The MAC the card shows: the board's own, from its hello.
  async cardMac(forward) {
    return this.driver.evaluate(`(() => { const m = ${cardText(forward)}.match(/[0-9a-f]{2}(:[0-9a-f]{2}){5}/i); return m ? m[0].toLowerCase() : null; })()`);
  }

  /// Open the Wi‑Fi panel's first page from `row` (an expression for the
  /// row's button). The row toggles the popover, so a panel still open (on
  /// a sub-page, or after an Escape it ignored) is closed first: clicking
  /// the row on an open panel would close it.
  async openWifi(row = WIFI_ROW_ANYWHERE, where = "the USB card") {
    await this.driver.waitFor(`Boolean(${row})`, { timeoutMs: STEP_MS, what: `the Wi‑Fi row in ${where}` });
    if (await this.driver.evaluate(`Boolean(${PANEL})`)) {
      if (await this.driver.evaluate(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.cloudRelay)})`)) return;
      await this.closePanel();
      const closed = await this.driver
        .waitFor(`!${PANEL}`, { timeoutMs: 5_000, what: "the panel to close" })
        .then(() => true)
        .catch(() => false);
      if (!closed) {
        await this.driver.evaluate(`${row}.click()`);
        await this.driver.waitFor(`!${PANEL}`, { timeoutMs: 10_000, what: "the panel to close on its row" });
      }
    }
    await this.driver.evaluate(`${row}.click()`);
    await this.driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.cloudRelay)})`, { timeoutMs: STEP_MS, what: "the Wi‑Fi panel" });
    await this.driver
      .waitFor(`getComputedStyle(${PANEL}).opacity === '1'`, { timeoutMs: 10_000, what: "the panel to finish fading in" })
      .catch(() => null);
  }

  async closePanel() {
    await this.driver.evaluate(`document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`);
  }

  async panelClick(text, options = {}) {
    await this.driver.waitFor(
      `[...(${PANEL}?.querySelectorAll('button') ?? [])].some((b) => !b.disabled && (b.innerText || '').includes(${JSON.stringify(text)}))`,
      { timeoutMs: STEP_MS, what: `${JSON.stringify(text)} in the Wi‑Fi panel` },
    );
    return this.driver.click(text, { scope: PANEL, ...options });
  }

  /// Whether the in-row test shows "Checking the password" crossed (drawn
  /// in the error colour), on the panel as it is now.
  async checkingPasswordCrossed() {
    return this.driver.evaluate(`(() => {
      const panel = ${PANEL}; if (!panel) return null;
      const el = [...panel.querySelectorAll('*')].find((n) => n.childElementCount === 0 && (n.textContent || '').includes(${JSON.stringify(WORDS.checkingPassword)}));
      if (!el) return null;
      for (let n = el, i = 0; n && i < 4; n = n.parentElement, i += 1) {
        if (/status-error/.test(String(n.className?.baseVal ?? n.className ?? ''))) return true;
      }
      return false;
    })()`);
  }

  /// The first editor slider, moved to its other end; its value comes back
  /// from the board's panel state, so the change is the board's echo.
  async turnFirstKnob() {
    const knob = `document.querySelector('#main [role="slider"]')`;
    await this.driver.waitFor(`Boolean(${knob})`, { timeoutMs: STEP_MS, what: "the editor's first knob" });
    const before = await this.driver.evaluate(`${knob}.getAttribute('aria-valuenow')`);
    await this.driver.evaluate(`(() => {
      const knob = ${knob};
      knob.focus();
      const key = Number(knob.getAttribute('aria-valuenow')) >= Number(knob.getAttribute('aria-valuemax')) ? 'Home' : 'End';
      knob.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
    })()`);
    await this.driver.waitFor(`${knob}.getAttribute('aria-valuenow') !== ${JSON.stringify(before)}`, {
      timeoutMs: STEP_MS,
      what: "the board's panel state to come back with the new value",
    });
    return `${before} → ${await this.driver.evaluate(`${knob}.getAttribute('aria-valuenow')`)}`;
  }

  /// Back to the Devices page without a reload (a reload would open new
  /// LAN links and hide whether the old ones stayed up).
  async toDevices(url) {
    const clicked = await this.driver.evaluate(`(() => {
      const a = [...document.querySelectorAll('a[href]')].find((l) => new URL(l.href, location.href).pathname === '/devices');
      if (!a) return false; a.click(); return true; })()`);
    if (!clicked) await this.load(url);
    await this.driver.waitFor(`location.pathname === '/devices'`, { timeoutMs: STEP_MS, what: "the Devices page" });
    return clicked ? "in-app" : "reloaded";
  }
}

// --- the walk ------------------------------------------------------------------

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
  const spec = doorSpec(fixturePath);
  const needs = prerequisites();
  const chrome = findChrome();

  if (options.dryRun) {
    const plan = {
      out,
      fixture: fixturePath,
      door: { boards: spec.boards, extraArgs: spec.extraArgs },
      studio: studioUrlForLan({ studioPort: "<walk port>", forwards: ["127.0.0.1:<fwd a>", "127.0.0.1:<fwd b>"], sinkUrl: "http://127.0.0.1:<sink>/ingest" }),
      usbPage: studioUrlForUsb({ studioPort: "<walk port>", doorAddr: "<door>", sinkUrl: "http://127.0.0.1:<sink>/ingest" }),
      steps: [
        `W1 lp-cli wifi add serial:ws://<door>/board/<id>/bytes ${NET.ssid} --password-stdin --json   (each board)`,
        "W1 lp-cli wifi status serial:ws://<door>/board/<id>/bytes --json   until connected",
        "   lp-cli link capture tcp://127.0.0.1:<bridge to /bytes> --console console/<id>.link.log   (held to the end)",
        "W2 Studio ?lan=… → two Wi‑Fi cards; each console: [lan] link … secure session opening",
        `W3 push ${WALK_PROJECT} to ${A} over the LAN; console: Project loaded, frames advance; edit a knob`,
        `W4 ${B}'s card Ready with its own MAC; no [lan] link … closed on either console; a second LAN dial to ${B} turned away (one LAN link); ${B} answers over its USB door`,
        `W5 GET /lans/${LAN}/browse?service=_lightplayer._tcp.local → both MACs`,
        `W6 LAN page off (about:blank; each console: link closed); release ${B}'s capture; Studio via USB → ${B}; ${GUEST.ssid} with a wrong password → last wrongPassword, back on ${NET.ssid} (status over ${B}'s forward)`,
        `W7 ${B}: ${NOWHERE.ssid} → last notFound`,
        `W8 lp-cli upload ${RADIO_PROJECT} lan:<fwd a> (page still off) → loaded, frames advance; LAN page back → the Radio node's words`,
        `W10 LAN page off; lp-cli link rtt lan:<fwd a> --count ${RTT_COUNT} --json rtt-lan.json → p50/p90 frames   (before W9)`,
        `W9 LAN page back; control ${A}: renumber, reset → a different address; Studio's link back through <fwd a> with traffic; status over USB at the new address`,
      ],
      prerequisites: needs,
      chrome,
    };
    writeFileSync(path.join(out, "walk-plan.json"), JSON.stringify(plan, null, 2));
    console.log("THE EMULATED WI‑FI WALK (lan) — dry run: nothing started");
    console.log(`  out        ${out}`);
    console.log(`  fixture    ${fixturePath}`);
    console.log(`  door       emu serve ${spec.boards.map((b) => `--board ${b}`).join(" ")} ${spec.extraArgs.join(" ")}`);
    console.log(`  page       ${plan.studio}`);
    for (const line of plan.steps) console.log(`  ${line}`);
    for (const need of needs) console.log(`  ${need.present ? "✓" : "✗"} ${need.what}: ${need.at}`);
    console.log(`  ${chrome ? "✓" : "✗"} Chrome: ${chrome ?? "none found (CHROME_BIN)"}`);
    console.log(`  plan → ${path.join(out, "walk-plan.json")}`);
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
  const port = walkPort(ROOT, "walk-wifi-emu-lan");
  const server = await serveStudioBundle({ root: ROOT, port });
  const sink = startRecordSink();
  const sinkUrl = await sink.listen();
  const door = await startDoor({
    root: ROOT,
    id: "walk-wifi-lan",
    boards: spec.boards,
    extraArgs: spec.extraArgs,
    stateDir,
    consoleDir,
    logFile: path.join(out, "serve.log"),
    fresh: true,
  });
  const usb = (id) => `serial:ws://${door.addr}/board/${id}/bytes`;

  const registry = await boardRegistry(door.addr);
  const entry = (id) => registry.find((b) => b.id === id);
  for (const id of BOARDS) if (!entry(id)) throw new Error(`the door holds no board ${id}: ${JSON.stringify(registry)}`);
  const fwd = Object.fromEntries(BOARDS.map((id) => [id, forwardOf(entry(id))]));
  const lan = (id) => `lan:${fwd[id]}`;
  const mac = Object.fromEntries(BOARDS.map((id) => [id, String(entry(id).mac).toLowerCase()]));
  // Read again after W1: at the door's first answer the boards have not
  // booted far enough to engage a seam, so the label is still the bare
  // grade (`lp-emu:esp32c6:t1`); once the firmware engages the network
  // seam the door says `…+net=lan`.
  let configuration = entry(A).configuration ?? "unknown";
  const url = studioUrlForLan({ studioPort: port, forwards: BOARDS.map((id) => fwd[id]), sinkUrl });
  const usbUrl = studioUrlForUsb({ studioPort: port, doorAddr: door.addr, sinkUrl });

  console.log("\nTHE EMULATED WI‑FI WALK (lan)");
  console.log(`  boards       ${BOARDS.map((id) => `${id} ${mac[id]} → ${lan(id)}`).join(" · ")}`);
  console.log(`  door         http://${door.addr}/boards   (pid ${door.pid})`);
  console.log(`  LAN          ${LAN}: ${fixturePath}`);
  console.log(`  config       ${configuration} · lp-emu ${commit}`);
  console.log(`  the page     ${url}\n`);

  const report = {
    configuration,
    lpEmu: commit,
    door: door.addr,
    url,
    boards: Object.fromEntries(BOARDS.map((id) => [id, { mac: mac[id], forward: fwd[id] }])),
    steps: [],
  };
  const holds = {};
  const consoles = () => Object.fromEntries(Object.entries(holds).map(([id, hold]) => [id, hold.console]));
  let driver = null;
  let page = null;
  /// W6/W7's page (Studio over USB), while it is open; their shots are its.
  let usbDriver = null;

  /// The board's own status over its USB door, while its console capture
  /// holds that door: the capture lets go, `lp-cli wifi status` asks, and a
  /// new capture takes the door back (its console continues in a new file).
  /// A board has one LAN link and Studio holds it, so this is how the walk
  /// asks a board Studio is connected to.
  const usbStatus = async (id, test = () => true, what = "its status") => {
    await releaseConsole(holds[id]);
    let status;
    try {
      status = await awaitStatus(usb(id), test, { what, timeoutMs: STEP_MS });
    } finally {
      const from = holds[id].console.mark();
      holds[id] = await holdConsole({ doorAddr: door.addr, board: id, console: holds[id].console });
      await holds[id].console.waitFor("[link] up (session", { from, what: "its USB link up for the capture again" });
    }
    return status;
  };

  /// Take the LAN page off both boards (`about:blank`), so the walk's own
  /// LAN tools have each board's one LAN link; waits for every captured
  /// board to say its link closed.
  let lanPageOn = true;
  const pageOff = async () => {
    if (!lanPageOn) return "already off";
    const marks = Object.fromEntries(Object.entries(consoles()).map(([board, c]) => [board, c.mark()]));
    await driver.navigate("about:blank");
    lanPageOn = false;
    const closed = {};
    for (const [board, c] of Object.entries(consoles())) {
      if (holds[board].released) continue;
      closed[board] = (await c.waitFor(/\[lan\] link \S+: closed/, { from: marks[board], what: "Studio's LAN link closed" })).trim();
    }
    return closed;
  };

  /// The LAN page back on both boards; waits for `board`'s console to say a
  /// new secure session opened, then its card.
  const pageOn = async (board) => {
    const from = holds[board].console.mark();
    await page.load(url);
    lanPageOn = true;
    const line = await holds[board].console.waitFor(/\[lan\] link \S+ .*secure session opening/, {
      from,
      what: "Studio's LAN link opening",
    });
    await page.cardSaysAny(fwd[board], ["Ready", "Degraded"]);
    return line.trim();
  };

  /// One W-step: run it, screenshot it, keep what each console and the sink
  /// gained while it ran.
  ///
  /// A failed step is recorded and the walk goes on to the next one, so one
  /// product failure does not hide what the later steps would have shown.
  /// `fatal` steps (W1, W2: everything after them needs the boards joined
  /// and the page holding both links) stop the walk instead. `seen` is what
  /// a step's body learned on its way, kept in the summary whether or not
  /// the step got to the end (a failed W3 still says the board loaded).
  const step = async (id, describe, body, { fatal = false } = {}) => {
    if (options.skip.includes(id)) {
      console.log(`— ${id}: ${describe}\n  – skipped (--skip)`);
      report.steps.push({ id, describe, ok: true, skipped: true, error: null, note: null, seen: {}, shot: null, consoleLines: {}, records: [] });
      return null;
    }
    console.log(`— ${id}: ${describe}`);
    const recordsBefore = sink.records.length;
    const marks = Object.fromEntries(Object.entries(consoles()).map(([board, c]) => [board, c.mark()]));
    let error = null;
    let note = null;
    const seen = {};
    try {
      note = await body(marks, seen);
    } catch (failure) {
      error = failure;
    }
    const shot = path.join(shots, `wifi-lan-${report.steps.length + 1}-${id}.png`);
    let shotTaken = null;
    const shotFrom = usbDriver ?? driver;
    if (shotFrom) {
      try {
        await shotFrom.screenshot(shot);
        shotTaken = shot;
      } catch {
        // the page may be gone
      }
    }
    const consoleLines = Object.fromEntries(
      Object.entries(consoles()).map(([board, c]) => [board, c.since(marks[board] ?? 0).split("\n").filter(Boolean).length]),
    );
    const records = sink.records.slice(recordsBefore);
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"}${note ? `  ${typeof note === "string" ? note : JSON.stringify(note)}` : ""}`);
    report.steps.push({ id, describe, ok: !error, error: error?.message ?? null, note, seen, shot: shotTaken, consoleLines, records });
    if (error && fatal) throw error;
    return note;
  };

  const joined = {};
  /// W6/W7's page, set up in W6.
  const usbTab = { driver: null, page: null };
  let fatal = null;
  try {
    await step("W1", "over each board's USB door, add the fixture's network: each board joins", async () => {
      // As built: lp-cli's `serial:ws://<door>/board/<id>/bytes` transport
      // opens the door's port without resetting the board. The bytes socket
      // has no control lines (`lpa-client/src/stream/ws_stream.rs`:
      // `set_signals` is a no-op and the provider skips the reset-on-open
      // for `ws://`, as for `tcp://`); the emulator's DTR/RTS are on the
      // separate `/control` socket. On a desk board the serial readiness
      // reset restarts the station (`desk-walk-wifi-c6.md`); here it does
      // not, so the `wifi add` below is tried while the board stays up.
      //
      // The boards are still booting ROM-up when this runs: since the OTA
      // merge (wire 39) a C6 checks its engine's digest before it answers a
      // hello (`[OTA] engine guard: … matches its digest (1111 ms)`), and
      // lp-cli's readiness gives up on a board that has not said hello yet
      // ("device did not become ready: timed out waiting for the device
      // hello"). The board saying hello IS the readiness, so a refusal of
      // that one kind is asked again, a few times; anything else fails.
      for (const id of BOARDS) {
        let added = null;
        for (let attempt = 1; attempt <= 4; attempt += 1) {
          added = await lpCli(["wifi", "add", usb(id), NET.ssid, "--password-stdin", "--json"], { stdin: NET.password });
          if (added.code === 0 || !added.stderr.includes("did not become ready")) break;
          console.log(`  (${id} had not said hello yet, attempt ${attempt}: asking again)`);
        }
        if (added.code !== 0) throw new Error(`wifi add on ${id} → exit ${added.code}: ${added.stderr.trim().split("\n").slice(-3).join(" | ")}`);
      }
      for (const id of BOARDS) {
        const status = await awaitStatus(usb(id), (s) => stationOf(s).kind === "connected" || stationOf(s).kind === "failed", {
          what: "connected (or failed)",
        });
        const station = stationOf(status);
        if (station.kind !== "connected") throw new Error(`${id} did not join: ${JSON.stringify(status.station)}`);
        if (station.ssid !== NET.ssid || !station.ip || !/^lp-[0-9a-f]{4}\.local$/.test(station.host ?? "")) {
          throw new Error(`${id}'s connected status is incomplete: ${JSON.stringify(status.station)}`);
        }
        joined[id] = { ip: station.ip, host: station.host, rssi: station.rssi };
      }
      if (joined[A].ip === joined[B].ip) throw new Error(`both boards hold ${joined[A].ip}`);
      if (joined[A].host === joined[B].host) throw new Error(`both boards are ${joined[A].host}`);
      // From here on each board's USB link is held by its console capture.
      for (const id of BOARDS) {
        holds[id] = await holdConsole({ doorAddr: door.addr, board: id, file: path.join(consoleDir, `${id}.link.log`) });
      }
      for (const id of BOARDS) {
        await holds[id].console.waitFor("[link] up (session", { what: "its USB link up for the capture" });
      }
      report.joined = joined;
      const now = await boardRegistry(door.addr);
      configuration = now.find((b) => b.id === A)?.configuration ?? configuration;
      report.configuration = configuration;
      return `${BOARDS.map((id) => `${id} ${joined[id].ip} (${joined[id].host})`).join(" · ")} · ${configuration}`;
    }, { fatal: true });

    driver = await StudioDriver.launch({ width: 1100, height: 900 });
    // Yona reads screenshots at 2× (memory: screenshots-for-yona-2x-zoom).
    await driver.cdp.send(
      "Emulation.setDeviceMetricsOverride",
      { width: 1100, height: 900, deviceScaleFactor: 2, mobile: false },
      driver.sessionId,
    );
    page = new Page(driver);

    await step("W2", "Studio with ?lan=: both boards appear as Wi‑Fi boards, each says hello over its secure LAN link", async (marks) => {
      await page.load(url);
      const seen = [];
      for (const id of BOARDS) {
        // The board's words first: its console shows the secure session
        // opening on a LAN link.
        const line = await holds[id].console.waitFor(/\[lan\] link \S+ .*secure session opening/, {
          from: marks[id] ?? 0,
          what: "a LAN link's secure session opening",
        });
        // Then the page, which only says where to look: the card at this
        // board's forward, Ready, showing THIS board's MAC (its hello's).
        // As built (seen on the first run): a Wi‑Fi board's card says
        // "Ready" and shows the MAC its hello carried, as a USB board's
        // does, with "Wi-Fi · 127.0.0.1:<forward>" leading its info line.
        await page.cardSays(fwd[id], "Ready");
        const shown = await page.cardMac(fwd[id]);
        if (shown !== mac[id]) throw new Error(`the card at ${fwd[id]} shows ${shown}, not ${id}'s ${mac[id]}`);
        seen.push(`${id}: ${line.trim()}`);
      }
      return seen.join(" · ");
    }, { fatal: true });
    const lanMarks = Object.fromEntries(BOARDS.map((id) => [id, holds[id].console.mark()]));

    await step("W3", `push ${WALK_PROJECT} to ${A} over the LAN, open it, edit a value`, async (marks, seen) => {
      const card = cardText(fwd[A]);
      const face = await driver.waitFor(
        `(() => { const t = ${card}; return t.includes('Remove project') ? 'running' : t.includes('to choose from') ? 'empty' : false; })()`,
        { timeoutMs: STEP_MS, what: `${A}'s card to say what it runs` },
      );
      if (face === "running") {
        await page.clickInCard(fwd[A], "Remove project");
        await driver.click("Remove project", { scope: cardOf(fwd[A]) });
        await driver.waitFor(`${card}.includes('to choose from')`, { timeoutMs: STEP_MS, what: `${A}'s empty face` });
      }
      await page.clickInCard(fwd[A], "to choose from");
      await driver.waitFor(`Boolean(${PANEL})`, { timeoutMs: STEP_MS, what: "the project popover" });
      await driver.click(WALK_PROJECT, { scope: PANEL });
      await page.clickInCard(fwd[A], "Put it on the board");
      // The board's words: the load on its console, then its frame counter
      // moving on the heartbeats after it.
      seen.loadLine = (await holds[A].console.waitFor("Project loaded", { from: marks[A], what: "`Project loaded`" })).trim();
      const counts = await holds[A].console.framesAdvance({ from: marks[A] });
      const loaded = holds[A].console.loadedProjects(marks[A]);
      seen.frameCounts = counts;
      seen.loaded = loaded;
      await page.clickInCard(fwd[A], "Open in editor");
      await driver.waitFor(`!${MAIN_TEXT}.includes('Connecting project') && Boolean(document.querySelector('#main [role="slider"]'))`, {
        timeoutMs: STEP_MS,
        what: "the project to open on the board",
      });
      const knob = await page.turnFirstKnob();
      return { loaded, frameCounts: counts, knob };
    });

    await step("W4", `${B}'s card while ${A} stays connected: both links stay up`, async (marks, seen) => {
      const how = await page.toDevices(url);
      // Both cards Ready, each showing ITS board's MAC: the hello each link
      // carried (the page only says where to look; the MAC is the board's).
      for (const id of BOARDS) {
        await page.cardSays(fwd[id], "Ready");
        const shown = await page.cardMac(fwd[id]);
        if (shown !== mac[id]) throw new Error(`the card at ${fwd[id]} shows ${shown}, not ${id}'s ${mac[id]}`);
      }
      const noneClosed = () => {
        if (how !== "in-app") return;
        for (const id of BOARDS) {
          const closed = holds[id].console.since(lanMarks[id]).split("\n").filter((l) => /\[lan\] link \S+: closed/.test(l));
          if (closed.length) throw new Error(`${id} closed a LAN link: ${closed[0]}`);
        }
      };
      // Since W2 opened them, neither board has closed a LAN link.
      noneClosed();
      // The board has ONE LAN link and Studio holds it: a second dial is
      // turned away in the board's words, and Studio's link stays up.
      const second = await lpCli(["wifi", "status", lan(B), "--json"], { timeoutMs: 120_000 });
      seen.secondDial = { code: second.code, stderrTail: second.stderr.trim().split("\n").slice(-2) };
      if (second.code === 0 || !refusedAsInUse(second.stderr)) {
        throw new Error(`a second LAN dial to ${B} was not turned away as in use: exit ${second.code}: ${second.stderr.trim().split("\n").slice(-2).join(" | ")}`);
      }
      const turnedAway = (
        await holds[B].console.waitFor("[lan] every LAN link is in use", { from: marks[B], what: "the second LAN dial turned away" })
      ).trim();
      seen.turnedAway = turnedAway;
      // c6-b answers over its USB door: its own status, at its own W1
      // address, its LAN link still Studio's.
      const status = await usbStatus(B, (s) => stationOf(s).kind === "connected", "connected");
      if (stationOf(status).ip !== joined[B].ip) throw new Error(`${B} answered over its USB door as ${JSON.stringify(status.station)}`);
      noneClosed();
      await page.cardSays(fwd[B], "Ready");
      return `back on Devices ${how}; both cards Ready with their own MACs; a second LAN dial to ${B} turned away (${turnedAway.replace(/^.*\] /, "")}); ${B} answered over USB at ${joined[B].ip}; no LAN link closed${how === "in-app" ? "" : " (not checked: the page reloaded)"}`;
    });

    await step("W5", "the LAN probe asks for _lightplayer._tcp: both boards answer, each with its own name and MAC", async () => {
      const answer = await probeBrowse(door.addr);
      const text = JSON.stringify(answer);
      const found = {};
      for (const id of BOARDS) {
        const hex = mac[id].replaceAll(":", "");
        if (!text.toLowerCase().includes(`mac=${hex}`)) throw new Error(`no answer carries ${id}'s mac=${hex}: ${text.slice(0, 2000)}`);
        found[id] = hex;
      }
      const instances = [...new Set([...text.matchAll(/([^"\\]+)\._lightplayer\._tcp\.local/g)].map((m) => m[1]))];
      if (instances.length < 2) throw new Error(`fewer than two instance names: ${JSON.stringify(instances)}`);
      report.probe = answer;
      return { instances, macs: found };
    });

    await step("W6", `${B}: add ${GUEST.ssid} with a wrong password from Studio over USB: "Wrong password", and the board stays on ${NET.ssid}`, async (_marks, seen) => {
      // The Wi‑Fi panel is on a USB card only (see studioUrlForLan), so
      // the walk hands c6-b's USB door from its capture to the page: the
      // console capture ends here and c6-b's evidence from now on is its
      // status answers over its LAN forward. Those need c6-b's one LAN
      // link, so the LAN page lets go of both boards first (it comes back
      // in W8), each board saying its link closed.
      seen.pageOff = await pageOff();
      await releaseConsole(holds[B]);
      usbDriver = await StudioDriver.launch({ width: 1100, height: 900 });
      await usbDriver.cdp.send(
        "Emulation.setDeviceMetricsOverride",
        { width: 1100, height: 900, deviceScaleFactor: 2, mobile: false },
        usbDriver.sessionId,
      );
      const usbPage = new Page(usbDriver);
      usbTab.driver = usbDriver;
      usbTab.page = usbPage;
      await usbPage.load(usbUrl);
      // The shim's banner covers the cards' lower rows in a shot.
      await usbDriver.click("hide").catch(() => null);
      await usbDriver.clickWhenReady("via USB", { timeoutMs: STEP_MS });
      await usbDriver.pickBoard(B, { timeoutMs: STEP_MS });
      // Studio's readiness may reset the board on open (the shim carries
      // DTR/RTS to the door); either way the board's answer over its
      // forward says when it is back on the good network.
      await awaitStatus(lan(B), (s) => stationOf(s).kind === "connected" && stationOf(s).ssid === NET.ssid, {
        what: `connected to ${NET.ssid} after Studio opened its USB door`,
      });
      await usbTab.page.openWifi();
      await usbTab.page.panelClick(WORDS.connectToANetwork);
      // The board heard it: its scan answer is what lists it. The first
      // run found the list empty for good: Studio sent no `wifi.scan` at
      // all (two `Network/Scan` commands, both while the panel's own
      // `wifi.status` read was in flight, which `network_controller.rs`
      // drops without asking again), while `lp-cli wifi scan` over the
      // board's forward heard both networks. A person would press the
      // panel's own "refresh"; the walk does too, once, and says so.
      let scanNeededRefresh = false;
      const listed = `${PANEL_TEXT}.includes(${JSON.stringify(GUEST.ssid)})`;
      await usbTab.driver.waitFor(listed, { timeoutMs: 30_000, what: `${GUEST.ssid} in the board's scan` }).catch(async () => {
        scanNeededRefresh = true;
        seen.scanNeededRefresh = true;
        await usbTab.page.panelClick("refresh");
        await usbTab.driver.waitFor(listed, { timeoutMs: STEP_MS, what: `${GUEST.ssid} in the board's scan after refresh` });
      });
      await usbTab.driver.click(GUEST.ssid, { scope: PANEL });
      if (!(await usbTab.driver.evaluate(typeInto('input[type="password"]', GUEST.wrong)))) throw new Error("no password field");
      await usbTab.page.panelClick(WORDS.connect, { exact: true });
      // The board's words: the network refused the password, and it is
      // back on the good one (its status over its forward).
      const status = await awaitStatus(
        lan(B),
        (s) => lastAttempt(s, GUEST.ssid) === "wrongPassword" && stationOf(s).kind === "connected" && stationOf(s).ssid === NET.ssid,
        { what: `${GUEST.ssid} last=wrongPassword and connected to ${NET.ssid}` },
      );
      // Now look at the page: the row (the test, or the saved row after a
      // reconnect) says Wrong password.
      await usbTab.driver
        .waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.wrongPassword)})`, { timeoutMs: 30_000, what: "Wrong password" })
        .catch(async () => {
          await usbTab.page.openWifi();
          await usbTab.driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.wrongPassword)})`, {
            timeoutMs: STEP_MS,
            what: "Wrong password after reopening the panel",
          });
        });
      const crossed = await usbTab.page.checkingPasswordCrossed();
      if (crossed === false) throw new Error(`"${WORDS.checkingPassword}" is shown but not crossed`);
      return {
        scanNeededRefresh,
        station: status.station,
        last: lastAttempt(status, GUEST.ssid),
        inRowTest: crossed === true ? "Checking the password crossed" : "the in-row test was not on the panel (the link dropped while the board tried)",
      };
    });

    await step("W7", `${B}: add ${NOWHERE.ssid}, which no access point has: "Not in range"`, async () => {
      await usbTab.page.closePanel();
      await usbTab.page.openWifi();
      await usbTab.page.panelClick(WORDS.connectToANetwork);
      await usbTab.page.panelClick(WORDS.otherNetwork);
      if (!(await usbTab.driver.evaluate(typeInto('input[placeholder="Network name"]', NOWHERE.ssid)))) throw new Error("no network name field");
      if (!(await usbTab.driver.evaluate(typeInto('input[type="password"]', NOWHERE.password)))) throw new Error("no password field");
      await usbTab.page.panelClick(WORDS.connect, { exact: true });
      const status = await awaitStatus(
        lan(B),
        (s) => lastAttempt(s, NOWHERE.ssid) === "notFound" && stationOf(s).kind === "connected" && stationOf(s).ssid === NET.ssid,
        { what: `${NOWHERE.ssid} last=notFound and connected to ${NET.ssid}` },
      );
      await usbTab.driver
        .waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.notInRange)})`, { timeoutMs: 30_000, what: "Not in range" })
        .catch(async () => {
          await usbTab.page.openWifi();
          await usbTab.driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.notInRange)})`, {
            timeoutMs: STEP_MS,
            what: "Not in range after reopening the panel",
          });
        });
      await usbTab.page.closePanel();
      return { station: status.station, last: lastAttempt(status, NOWHERE.ssid) };
    });
    // W9 needs c6-a's control channel, which the USB page's shim holds.
    if (usbDriver) {
      const shim = usbDriver;
      usbDriver = null;
      await shim.close().catch(() => null);
    }

    await step("W8", `a project with a Radio node on ${A}: the node says why the Radio is off, and the rest runs`, async (marks, seen) => {
      // The LAN page is still off (W6): the upload has c6-a's one LAN link.
      const upload = await lpCli(["upload", RADIO_PROJECT, lan(A)], { timeoutMs: STEP_MS });
      seen.upload = { code: upload.code, stderrTail: upload.stderr.trim().split("\n").slice(-6) };
      // As built (run 7): the board takes the project, runs it with its
      // Radio node faulted, and lp-cli's deploy check reports that fault
      // and exits 1 ("deploy was acked, but the deployed project failed to
      // run: open control radio …: Radio is off while this board uses
      // Wi-Fi. …"). The message is the board's (a string only the firmware
      // holds), so that exit is W8's expected answer, not a failed upload;
      // any other failure is.
      const radioRefused = upload.code !== 0 && upload.stderr.includes("deploy was acked") && upload.stderr.includes(RADIO_OFF_FOR_WIFI);
      seen.uploadSaidRadioOff = radioRefused;
      if (upload.code !== 0 && !radioRefused) throw new Error(`upload ${RADIO_PROJECT} ${lan(A)} → exit ${upload.code}: ${upload.stderr.trim().split("\n").slice(-4).join(" | ")}`);
      const slug = path.basename(RADIO_PROJECT);
      seen.loadLine = (await holds[A].console.waitFor(`Project loaded: ${slug}`, { from: marks[A], what: `\`Project loaded: ${slug}\`` })).trim();
      const counts = await holds[A].console.framesAdvance({ from: marks[A] });
      seen.frameCounts = counts;
      const loaded = holds[A].console.loadedProjects(marks[A]);
      if (!loaded?.includes(slug)) throw new Error(`${A}'s heartbeat lists ${loaded}, not ${slug}`);
      // The rest runs: the frame counter moved, and the heartbeat's fault
      // names the Radio node and nothing else.
      const faulted = [...new Set([...holds[A].console.since(marks[A]).matchAll(/"nodes":\[\{"path":"([^"]+)"/g)].map((m) => m[1]))];
      seen.faultedNodes = faulted;
      if (faulted.some((node) => !/radio/i.test(node))) throw new Error(`${A}'s heartbeat faults more than the Radio node: ${JSON.stringify(faulted)}`);
      // The node's words: on the console if the firmware logs them, and on
      // the page wherever Studio draws the node (a string only the firmware
      // holds).
      // ASSUMES: the editor draws a node's unavailable reason as page text
      // somewhere on opening the project (the node card, or its status line).
      const onConsole = holds[A].console.since(marks[A]).includes(RADIO_OFF_FOR_WIFI);
      // Studio back on the LAN, for the editor: c6-a's console says the
      // page's link opened again. The page is where a person reads the
      // message, but the board's words above already decide W8, so a page
      // that does not come back is recorded (`seen.page`), not the step's
      // failure.
      const radioOnPage = `(document.body.innerText || '').includes(${JSON.stringify(RADIO_OFF_FOR_WIFI)})`;
      const onPage = await (async () => {
        seen.relinked = await pageOn(A);
        await page.clickInCard(fwd[A], "Open in editor");
        return driver
          .waitFor(radioOnPage, { timeoutMs: 60_000, what: "the Radio node's message" })
          .then(() => true)
          .catch(async () => {
            // Not drawn until the node is chosen: choose it (the project's
            // node is `radio.json`), and look again.
            await driver.click("radio", { exact: true }).catch(() => null);
            return driver
              .waitFor(radioOnPage, { timeoutMs: STEP_MS, what: "the Radio node's message" })
              .then(() => true)
              .catch(() => false);
          });
      })().catch((error) => {
        seen.page = error.message.split("\n")[0];
        return false;
      });
      if (!radioRefused && !onConsole && !onPage) throw new Error("the Radio node's message is not in the board's deploy reply, on its console, or in the editor");
      return { loaded, frameCounts: counts, faultedNodes: faulted, radioMessage: { inDeployReply: radioRefused, onConsole, onPage } };
    });

    // W10 runs BEFORE W9, out of the plan's order: W9 resets c6-a onto a
    // new lease, and W10 measures the link of a board that has not just
    // been reset (the first full run lost W10 to what W9 left behind, the
    // forward no longer reaching the board). The step ids keep the plan's
    // numbering.
    await step("W10", `lp-cli link rtt ${lan(A)}: request p50 and p90 in frames`, async (marks, seen) => {
      // `link rtt` needs c6-a's one LAN link: the page lets go first.
      seen.pageOff = await pageOff();
      const json = path.join(out, "rtt-lan.json");
      // A previous run's report would read as this run's.
      rmSync(json, { force: true });
      rmSync(path.join(out, "rtt-lan.console.log"), { force: true });
      const run = await lpCli(
        [
          "link", "rtt", lan(A),
          "--json", json,
          "--console", path.join(out, "rtt-lan.console.log"),
          "--count", String(RTT_COUNT),
          // The idle window is WALL seconds on a `lan:` target, and its
          // frame rate needs two heartbeats (5 s of BOARD time apart). A
          // board held to a connected host's pace and rendering a project
          // runs slower than the wall (run 7: 5 s of board time in ≈17 s;
          // run 10, on a box at load 73, 5 s in 76 s), so the default 20 s
          // window caught one heartbeat and no rate.
          "--idle-s", "180",
          "--label", `${configuration} lp-emu ${commit} via ${lan(A)}`,
        ],
        { timeoutMs: 900_000 },
      );
      if (run.code !== 0) throw new Error(`link rtt → exit ${run.code}: ${run.stderr.trim().split("\n").slice(-4).join(" | ")}`);
      // What the board rendered while it was timed: the `loaded_projects`
      // of the heartbeats its USB console carried DURING the run (W8's
      // project, or `[]` if W8 did not load one). The rtt report's own
      // heartbeats carry frame counts, not projects.
      seen.loaded = holds[A].console.loadedProjects(marks[A]);
      const rtt = JSON.parse(readFileSync(json, "utf8"));
      const frames = rtt.request_rtt_frames ?? {};
      if (frames.n !== RTT_COUNT) throw new Error(`${frames.n ?? 0} of ${RTT_COUNT} requests timed in frames (idle fps ${rtt.idle_fps})`);
      if (rtt.link_resets !== 0) throw new Error(`${rtt.link_resets} link reset(s) during the run`);
      // `request_rtt_frames` multiplies a WALL-clock round trip by the
      // board's EMULATED frame rate (its heartbeats' uptime). The same
      // round trips in frames the board drew per wall second, from the
      // heartbeats' host arrival times, sit beside it: on an emulator that
      // runs slower than silicon the two differ, and only the second is
      // what a person at the page would count.
      const beats = (rtt.heartbeats ?? []).filter((b) => Number.isFinite(b.at_us) && Number.isFinite(b.frame_count));
      let wallFps = null;
      if (beats.length >= 2) {
        const first = beats[0];
        const last = beats[beats.length - 1];
        const seconds = (last.at_us - first.at_us) / 1e6;
        if (seconds > 0) wallFps = (last.frame_count - first.frame_count) / seconds;
      }
      const wallFrames = wallFps === null ? null : quantiles((rtt.requests ?? []).map((r) => (r.rtt_us / 1e6) * wallFps));
      report.rtt = { configuration, lpEmu: commit, frames, wallFrames, idleFps: rtt.idle_fps, wallFps, loaded: seen.loaded };
      return `request RTT p50 ${frames.p50} / p90 ${frames.p90} frames (board fps ${rtt.idle_fps}); ${
        wallFrames ? `p50 ${wallFrames.p50} / p90 ${wallFrames.p90} frames drawn per wall second` : "no wall frame rate"
      } — rendering ${seen.loaded ?? "nothing"} — ${configuration}, lp-emu ${commit}`;
    });
    await step("W9", `reset ${A} from the control port with the next-lease option on: a new address, Studio back through the same forward`, async (_marks, seen) => {
      // Studio back on the LAN first (W10 had it off), holding c6-a's link
      // when the board goes away.
      seen.linkedBefore = await pageOn(A);
      const marks = Object.fromEntries(BOARDS.filter((id) => !holds[id].released).map((id) => [id, holds[id].console.mark()]));
      const old = joined[A].ip;
      const renumbered = await control(door.addr, A, "renumber");
      const reset = await control(door.addr, A, "reset");
      const addressLine = await holds[A].console.waitFor(/\[wifi\] address \d+\.\d+\.\d+\.\d+/, {
        from: marks[A],
        timeoutMs: JOIN_MS,
        what: "a new address",
      });
      const ip = addressLine.match(/(\d+\.\d+\.\d+\.\d+)/)[1];
      if (ip === old) throw new Error(`${A} came back on ${old}, the old address`);
      seen.address = addressLine.trim();
      // Studio's link back through the same forward. Nothing else of the
      // walk's dials this board's LAN, so the line is Studio's link.
      const afterAddress = marks[A] + holds[A].console.since(marks[A]).indexOf(addressLine);
      const lanLine = await holds[A].console.waitFor(/\[lan\] link \S+ .*secure session opening/, {
        from: afterAddress,
        what: "Studio's LAN link opening again after the new address",
      });
      seen.relinked = lanLine.trim();
      // …and carrying traffic both ways, in the board's words: the link's
      // lp-link session up (`radio link <id>: session N up`, which needs the
      // host's half of the handshake), or its own counters with frames in
      // and out (logged every other heartbeat, so later on a slow box).
      const linkId = lanLine.match(/\[lan\] link (\S+) /)[1];
      const afterRelink = afterAddress + holds[A].console.since(afterAddress).indexOf(lanLine);
      const traffic = await holds[A].console.waitFor(
        (line) => {
          if (new RegExp(`radio link ${escapeRegExp(linkId)}: session \\d+ up`).test(line)) return true;
          const m = line.match(new RegExp(`\\[lan\\] link ${escapeRegExp(linkId)}: frames in (\\d+) out (\\d+)`));
          return Boolean(m) && Number(m[1]) > 0 && Number(m[2]) > 0;
        },
        { from: afterRelink, what: `traffic both ways on ${linkId}` },
      );
      seen.traffic = traffic.trim();
      // The board's status at its new address, over its USB door (Studio
      // holds its one LAN link).
      const status = await usbStatus(A, (s) => stationOf(s).kind === "connected" && stationOf(s).ip === ip, `connected at ${ip}`);
      seen.station = status.station;
      joined[A] = { ...joined[A], ip, before: old };
      // Last, the page: the card back on its link, "Ready" (or "Degraded"
      // when W8's project runs with its Radio node off — the board's own
      // fault, relayed). A card that stays on neither after a relink is
      // Studio's finding 5 (PR B's), and does not hide the board's
      // evidence above: the step passes on the board's words and says so.
      await page.toDevices(url);
      const face = await page.cardSaysAny(fwd[A], ["Ready", "Degraded"]).catch(() => null);
      seen.cardFace = face;
      const card = face
        ? `card ${face}`
        : `board-side pass, Studio finding 5: the card says ${JSON.stringify((await driver.evaluate(cardText(fwd[A]))).replace(/\s+/g, " ").slice(0, 160))}`;
      return { renumbered, reset, before: old, after: ip, station: status.station, relinked: seen.relinked, traffic: seen.traffic, card };
    });

  } catch (error) {
    fatal = error;
  }

  if (usbDriver) await usbDriver.close().catch(() => null);
  const lines = driver ? driver.consoleLines() : [];
  const pageErrors = lines.filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  const panics = pageErrors.filter((l) => l.includes("panicked at"));
  const leaked = lines.filter((l) => [NET.password, GUEST.password, GUEST.wrong, NOWHERE.password].some((pw) => l.includes(pw)));
  report.pageErrors = pageErrors;
  report.passwordInPageConsole = leaked.length;
  report.registry = await boardRegistry(door.addr).catch(() => null);
  writeFileSync(path.join(out, "walk.jsonl"), sink.raw.join("\n") + "\n");
  writeFileSync(path.join(out, "walk-wifi-emu-lan.json"), JSON.stringify(report, null, 2));

  console.log("\n=== the emulated Wi‑Fi walk (lan), step by step");
  for (const s of report.steps) {
    console.log(`  ${s.skipped ? "–" : s.ok ? "✓" : "✗"} ${s.id.padEnd(4)} ${s.records.length} record(s)   ${s.shot ? path.basename(s.shot) : "(no shot)"}`);
  }
  if (pageErrors.length) {
    console.log("\n  page console errors:");
    for (const line of pageErrors.slice(-8)) console.log(`    ${line.slice(0, 300)}`);
  }
  console.log(`  a password in the page console: ${leaked.length} line(s)`);
  console.log(`\n  summary → ${path.join(out, "walk-wifi-emu-lan.json")}`);
  console.log(`  records → ${path.join(out, "walk.jsonl")}  (${sink.records.length})`);
  console.log(`  consoles → ${consoleDir}`);

  if (!options.keepOpen) {
    if (driver) await driver.close();
    for (const hold of Object.values(holds)) await releaseConsole(hold);
    await stopDoor(door);
    sink.server.close();
    server.close();
  } else {
    console.log(`\n  --keep-open: the door (pid ${door.pid}), the captures and the browser are still up.`);
  }

  if (fatal) {
    console.error(`\nThe emulated Wi‑Fi walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  const failed = report.steps.filter((s) => !s.ok);
  if (failed.length) {
    console.error(`\nThe emulated Wi‑Fi walk ran to the end, but ${failed.map((s) => s.id).join(", ")} failed:`);
    for (const s of failed) console.error(`  ${s.id}: ${s.error.split("\n")[0]}`);
    process.exit(1);
  }
  if (panics.length || leaked.length) {
    console.error(`\nEvery step passed, but the page panicked ${panics.length} time(s) and printed a password ${leaked.length} time(s).`);
    process.exit(1);
  }
  const skipped = report.steps.filter((s) => s.skipped).map((s) => s.id);
  console.log(
    `\n✓ the emulated Wi‑Fi walk finished W1–W10${skipped.length ? ` except ${skipped.join(", ")} (skipped, NOT run)` : ""} (${configuration}, lp-emu ${commit}).`,
  );
}

/// n, p50, p90 of `xs`, rounded to thousandths (the rtt report's own rule).
function quantiles(xs) {
  if (!xs.length) return { n: 0 };
  const v = [...xs].sort((a, b) => a - b);
  const q = (p) => Math.round(v[Math.round((v.length - 1) * p)] * 1000) / 1000;
  return { n: v.length, p50: q(0.5), p90: q(0.9) };
}

// Run as the `lan` lane (the dispatcher imports this file) or on its own;
// imported by another lane (`walk-wifi-emu-studio-lan.mjs`) for its
// helpers, it runs nothing.
if (process.argv[2] === "lan" || path.basename(process.argv[1] ?? "") === "walk-wifi-emu-lan.mjs") await main();
