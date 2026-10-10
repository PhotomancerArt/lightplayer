#!/usr/bin/env node
// THE MIGRATION WALK (C6 repartition, P08; `just walk-migration-emu <scenario>`).
//
// Real Studio, in headless Chrome, updating an emulated ESP32-C6 that is a
// "fielded board": the current firmware on the frozen pre-2026-10 partition
// table, its files in a 240-block littlefs at 0x310000 — the board every
// shipped C6 is today. Studio's own Update (the board card's
// `update-firmware` offer) runs the layout migration through esptool-js,
// over the `?emu=` virtual serial port, into the emulated mask ROM's
// download console.
//
// One scenario per invocation (a migration is ~180 s emulated and a run is
// several minutes of wall clock; a scenario must fit one foreground command):
//
//   W1  happy path: Update → the question → Continue → files moved
//   W2  a board already on the new layout: Update asks nothing
//   W3  a board whose files do not fit: refused, nothing written
//   W4  Cancel at the question: nothing written
//   W5  cable pulled during the FIRMWARE write: the card leads back to a
//       completed migration with every file
//   W9  a board flashed with the new image by a path that skipped the
//       migration (a bare `esptool write_flash 0x0`): it boots HOLDING its
//       files, and the firmware bar's Finish update moves them
//   W7a cable pulled during the filesystem write: the board comes back
//       formatted with its backup in this browser
//   W7b  …then Restore files puts them back (W8: the tab was
//       closed in between — a new Chrome on the same profile)
//   W12 W1 with the board a Worker in the page (`?emu=tab`, no door): the
//       fixture chip is put into it through the page, and its chip and
//       console are read back out of the page (`/__walk/*` on this
//       script's own server)
//
// Every claim keys off the BOARD's words or the CHIP's bytes, never a Studio
// string another component could satisfy (walk-no-board's binding rule):
// the door's console for the firmware's own `[INIT]`/`[FS]` lines, and, after
// the door has written the chip back, `lp-cli hardware lpfs report --image`
// of the chip file against the fixture's own report, file by file (SHA-256).
// Studio is read only to know when, and what, to press — and only through
// the board card's hooks (`lp-app/lpa-studio-web/src/app/board_card/mod.rs`,
// "Walk hooks"): the offers core publishes on the card (`data-offer-path`:
// `update-firmware`, `finish-update`, `restore-files`, `continue-update`,
// `cancel-update`, `flash`), the firmware bar's work (the update's steps,
// `lpa_devices::FlashStep`), the firmware details core raises while the
// layout question is open (`UiDetailPanel::Layout` — no longer a dialog),
// the access and connection details, and the board's terminal in the status
// corner. Never the card's face text.
//
// NOT CI (same rule as walk-no-board). Needs: the release Studio bundle
// (`just studio-web-story-build`), the packaged firmware
// (`just studio-firmware-package-served`), a debug `lp-cli`, Chrome. It
// serves the bundle itself on an ephemeral port — no dev server, so nothing
// here can adopt a sibling worktree's listener.

import { spawnSync } from "node:child_process";
import { createReadStream, cpSync, existsSync, mkdirSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { createServer } from "node:http";
import path from "node:path";
import process from "node:process";

import { PANEL, StudioDriver, boardPath } from "./studio-driver.mjs";
import { serveStudioBundle, startDoor, stopDoor, studioUrlFor, walkPort } from "./emulated-lane.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const PUBLIC = path.join(ROOT, "target/dx/lpa-studio-web/release/web/public");
const FIRMWARE = path.join(ROOT, "target/studio-web-assets/firmware");
const LP_CLI = path.join(ROOT, "target/debug/lp-cli");
/// The door's board id; the tab lane (W12) sets it to the tab's own board.
let BOARD = "c6-a";
const MAC = "60:55:f9:0a:0b:0c";
const UID = "dev0000000000000011";
/// The board model the fixture is (its stamped `/hardware.json`): what the
/// board pick is answered with when the way back is a flash.
const BOARD_MODEL = "XIAO ESP32-C6";
const STUDIO_LOAD_MS = 420_000;
const STEP_MS = 180_000;
/// A wedged-run guard for a whole migration, never a measurement.
/// `WALK_MIGRATION_MS` shortens it for a run that is EXPECTED to wedge (a
/// reproduction), so it fails inside one foreground command.
const MIGRATION_MS = Number(process.env.WALK_MIGRATION_MS ?? 900_000);

const SCENARIOS = {
  W1: { fixture: "legacy", describe: "Update → the question → Continue → files moved" },
  W2: { fixture: "current", describe: "a board already on the new layout: Update asks nothing" },
  W3: { fixture: "overfull", describe: "a board whose files do not fit: refused, nothing written" },
  W4: { fixture: "legacy", describe: "Cancel at the question: nothing written" },
  W5: { fixture: "legacy", describe: "cable pulled mid firmware write: the card leads back to a completed migration" },
  W9: { fixture: "bypassed", describe: "a bypassed flash holds the files; Finish update moves them" },
  W7a: { fixture: "legacy", describe: "cable pulled mid filesystem write: formatted, backup in this browser" },
  W7b: { fixture: null, describe: "a new Chrome on the same profile restores the backup" },
  W12: { fixture: "legacy", tab: true, describe: "W1 with the board a Worker in the page (?emu=tab, no door)" },
};

function args() {
  const argv = process.argv.slice(2);
  const scenario = argv.find((a) => !a.startsWith("--"));
  if (!scenario || !SCENARIOS[scenario]) {
    console.error(`usage: node scripts/emu/walk-migration-emu.mjs <${Object.keys(SCENARIOS).join("|")}> [--out <dir>] [--full-access]`);
    process.exit(2);
  }
  const outAt = argv.indexOf("--out");
  // The b halves continue their a half: same state, same browser profile.
  const family = scenario.replace(/[ab]$/, "");
  const out = outAt >= 0 ? argv[outAt + 1] : path.join(ROOT, "target/walk-migration-emu", family);
  // G1's spare C6 (2026-10-03): a device store at its 16-entry cap, anyone
  // nearby on, none of the entries this browser's — so the connect's add of
  // this browser's key is refused. A b half inherits its a half's board.
  FULL_ACCESS = argv.includes("--full-access");
  return { scenario, family, out };
}

/// `--full-access`: the fixture's device store is full (see `args`).
let FULL_ACCESS = false;

/// The device store the fixture board holds: one browser key (the walk's
/// own fixture), or, with `--full-access`, sixteen play passwords — the cap
/// — with anyone nearby on.
function fixtureAccessJson() {
  if (!FULL_ACCESS) {
    return '{"version":2,"bleEnabled":true,"open":false,"secrets":[{"label":"walk browser","kind":"browser","tier":"edit","salt":"AAECAwQFBgcICQoLDA0ODw==","iterations":1,"k":"ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8="}]}';
  }
  const secrets = Array.from({ length: 16 }, (_, n) => ({
    label: `guest ${n}`,
    kind: "password",
    tier: "play",
    salt: Buffer.alloc(16, 100 + n).toString("base64"),
    iterations: 1,
    k: Buffer.alloc(32, n + 1).toString("base64"),
  }));
  return JSON.stringify({ version: 2, bleEnabled: true, open: true, secrets });
}

/// What the card's access details must say once the board is back: who gets
/// in with no password, as the board's own store has it — the "This board"
/// section's "Open to" (`access_bar.rs`, `open_summary` in core's
/// `ui_access_view.rs`; main's #929 panel, which replaced the old "Who has
/// access N" count). Core draws it with the access panel, and says
/// "password" there before the board has answered its list too, so it is
/// judged only once the Bluetooth switch — locked until that answer — is on
/// and usable ([`awaitAccess`]). The default fixture's store is a version-2
/// file with `open: false` (nobody: "password"); the full store has `open:
/// true` (play: "anyone can play").
function expectedOpenSummary() {
  return FULL_ACCESS ? "anyone can play" : "password";
}

// --- the fixture board ---------------------------------------------------

function copyDir(from, prefix, files) {
  for (const entry of readdirSorted(from)) {
    const at = path.join(from, entry);
    if (statSync(at).isDirectory()) copyDir(at, `${prefix}/${entry}`, files);
    else files.push([`${prefix}/${entry}`, readFileSync(at)]);
  }
}

function readdirSorted(dir) {
  return spawnSync("ls", ["-A", dir], { encoding: "utf8" }).stdout.split("\n").filter(Boolean).sort();
}

function noise(len, seed) {
  let state = (0x9e3779b9 ^ seed) >>> 0;
  const out = Buffer.alloc(len);
  for (let i = 0; i < len; i += 1) {
    state ^= state << 13; state >>>= 0;
    state ^= state >>> 17;
    state ^= state << 5; state >>>= 0;
    out[i] = state & 0xff;
  }
  return out;
}

/// The board's files (plan Q4): two projects, the stamped XIAO C6 manifest,
/// the identity, an access file with one browser key, a > 1-block file.
function fixtureTree(dir, overFull) {
  rmSync(dir, { recursive: true, force: true });
  const files = [];
  copyDir(path.join(ROOT, "projects/test/basic"), "/projects/basic", files);
  copyDir(path.join(ROOT, "catalog/projects/playful-choker"), "/projects/playful-choker", files);
  files.push(["/hardware.json", readFileSync(path.join(ROOT, "lp-core/lpc-hardware/boards/seeed/xiao-esp32-c6.json"))]);
  files.push(["/.lp/device.json", Buffer.from(JSON.stringify({ uid: UID, name: "Porch" }))]);
  files.push(["/.lp/access.json", Buffer.from(fixtureAccessJson())]);
  files.push(["/notes/long.txt", noise(9000, 1)]);
  if (overFull) files.push(["/projects/big/blob.bin", noise(720 * 1024, 2)]);
  for (const [file, bytes] of files) {
    const at = path.join(dir, file);
    mkdirSync(path.dirname(at), { recursive: true });
    writeFileSync(at, bytes);
  }
}

function run(cmd, cmdArgs) {
  const result = spawnSync(cmd, cmdArgs, { cwd: ROOT, encoding: "utf8", maxBuffer: 64 << 20 });
  if (result.status !== 0) {
    throw new Error(`${cmd} ${cmdArgs.join(" ")} exited ${result.status}:\n${result.stdout}\n${result.stderr}`);
  }
  return result.stdout;
}

/// The fixture chip: the packaged firmware at 0x0, the table at 0x8000
/// (legacy unless `current`), the files at that table's lpfs.
function buildFixture(kind, out) {
  const tree = path.join(out, "fixture-tree");
  fixtureTree(tree, kind === "overfull");
  const merged = path.join(FIRMWARE, "esp32c6-4mb/fw-esp32c6-merged.bin");
  const chip = path.join(out, "fixture.bin");
  const cliArgs = ["hardware", "lpfs", "fixture", "--merged", merged, "--tree", tree, "--out", chip];
  if (kind === "current") cliArgs.push("--table", path.join(ROOT, "lp-fw/fw-esp32c6/partitions.csv"));
  console.log("  " + run(LP_CLI, cliArgs).trim());
  if (kind === "bypassed") {
    // `esptool write_flash 0x0 <merged.bin>` onto the fielded board: the new
    // image (and its new table at 0x8000) over the old, nothing else — the
    // old filesystem stays at 0x310000, and nothing is at the new lpfs.
    const bytes = readFileSync(chip);
    // What the board held, for the report: its table still says where.
    writeFileSync(path.join(out, "fixture-before-bypass.bin"), bytes);
    readFileSync(merged).copy(bytes, 0);
    writeFileSync(chip, bytes);
    console.log("  …then the new image written over 0x0 with no migration (a bypassed flash)");
  }
  return chip;
}

/// `lp-cli hardware lpfs report --image <chip> --json`: the files a chip holds.
function report(chip) {
  return JSON.parse(run(LP_CLI, ["hardware", "lpfs", "report", "--image", chip, "--json"]));
}

// --- the bundle, served by this script ----------------------------------

/// The walk's own endpoints beside the release bundle (`serveStudioBundle`),
/// on this worktree's stable slot for the walk — a browser's OPFS (where
/// Studio keeps a board's backup) belongs to the ORIGIN, and W7b comes back
/// in a new Chrome on the same profile expecting the same origin. The two
/// routes are the tab lane's (W12), whose chip lives in the page:
/// `GET /__walk/fixture.bin` seeds it, and `POST /__walk/chip?name=<file>`
/// brings it back out to `walk.out`.
function serveBundle(walk) {
  const route = (request, response, url) => {
    if (url.pathname === "/__walk/fixture.bin") {
      response.writeHead(200, { "content-type": "application/octet-stream" });
      createReadStream(walk.fixtureChip).pipe(response);
      return true;
    }
    if (url.pathname === "/__walk/chip" && request.method === "POST") {
      // A name under `out`, never outside it.
      const name = path.normalize(url.searchParams.get("name") ?? "chip.bin").replace(/^(\.\.[/\\])+/, "");
      const chunks = [];
      request.on("data", (chunk) => chunks.push(chunk));
      request.on("end", () => {
        writeFileSync(path.join(walk.out, name), Buffer.concat(chunks));
        response.writeHead(204);
        response.end();
      });
      return true;
    }
    return false;
  };
  return serveStudioBundle({ root: ROOT, port: walkPort(ROOT, "walk-migration-emu"), route });
}

function startSink() {
  const server = createServer((request, response) => {
    request.resume();
    request.on("end", () => {
      response.writeHead(204, { "access-control-allow-origin": "*" });
      response.end();
    });
  });
  return new Promise((resolve) => server.listen(0, "127.0.0.1", () => resolve(server)));
}

// --- the page --------------------------------------------------------------

const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;

/// The board's card, `devices/mac-<hex>` by the MAC the shim lists for
/// `BOARD` (`main` sets it once the shim is up); `null` when the shim names
/// none, and then the walk reads the one linked card ([`liveBoard`]).
let CARD = null;

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/// The firmware verbs a settled card offers this fixture: Update (the
/// firmware bar's action on a board older than this Studio, otherwise a verb
/// in the firmware details — `firmware_bar.rs`), Finish update on a board
/// holding its files, Restore files on a board whose files are in a backup
/// (both the firmware bar's action: `device_layout_view.rs`'s
/// `FINISH_UPDATE`, `RESTORE_FILES`).
const SETTLED_VERBS = ["update-firmware", "finish-update", "restore-files"];

/// Page-side: `layer`'s open details on the card at `board` (the popover
/// panel, a descendant of the bar), or null.
function barPanel(board, layer) {
  return `document.querySelector(${JSON.stringify(`[data-board-card="${board}"] [data-bar="${layer}"] [id^="ux-popover-panel"]`)})`;
}

/// Page-side: the button that presses `verb` on the card at `board` — the
/// last button inside its `AgentMark`, on the card's face or (`inDetails`)
/// inside its open details: the button the driver's `pressOffer` clicks.
function offerButton(board, verb, inDetails) {
  return `((() => {
    const card = document.querySelector(${JSON.stringify(`[data-board-card="${board}"]`)});
    const mark = card ? [...card.querySelectorAll(${JSON.stringify(`[data-offer-path$="/${verb}"]`)})]
      .find((m) => Boolean(m.closest('.ux-popover-layer')) === ${Boolean(inDetails)}) : null;
    const all = mark ? mark.querySelectorAll('button') : [];
    return all.length ? all[all.length - 1] : null;
  })())`;
}

/// The card the board is on now: its own (`CARD`) while that card is
/// linked, else the one linked card on the page — a board that comes back
/// from a pulled cable not running LightPlayer can be a new card until it
/// says who it is. Linked: its connection bar does not read "Offline · …"
/// (`connection_bar.rs`, `offline_words`).
async function liveBoard(driver) {
  const paths = await driver.cardPaths();
  const linked = [];
  for (const board of paths) {
    const line = await driver.barText("connection", { board });
    if (line && !line.startsWith("Offline")) linked.push(board);
  }
  if (CARD && linked.includes(CARD)) return CARD;
  if (linked.length === 1) return linked[0];
  if (CARD && paths.includes(CARD)) return CARD;
  if (paths.length === 1) return paths[0];
  throw new Error(`no one card for the board (cards: ${paths.join(", ") || "none"}; linked: ${linked.join(", ") || "none"})`);
}

/// The first of `verbs` a card offers right now, and where: on a card's face
/// (the firmware bar's action), else inside a card's firmware details (where
/// Update sits on a board that is not older than this Studio). Returns
/// `{ board, verb, bar }`, or null. Opens and closes the details it looks in.
async function findFirmwareVerb(driver, verbs) {
  const paths = await driver.cardPaths();
  for (const board of paths) {
    for (const verb of verbs) {
      if (await driver.offered(verb, { board })) return { board, verb, bar: null };
    }
  }
  for (const board of paths) {
    try {
      await driver.openBar("firmware", { board, timeoutMs: 10_000 });
    } catch {
      continue;
    }
    try {
      for (const verb of verbs) {
        if (await driver.offered(verb, { board, inDetails: true })) return { board, verb, bar: "firmware" };
      }
    } finally {
      await driver.closeDetails({ board }).catch(() => {});
    }
  }
  return null;
}

/// Until a card offers one of `verbs` ([`findFirmwareVerb`]), polled every
/// second: a verb in a bar's details is drawn only while they are open, so
/// no single page-side wait can see it arrive.
async function waitFirmwareVerb(driver, verbs, { timeoutMs, what }) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const found = await findFirmwareVerb(driver, verbs);
    if (found) return found;
    if (Date.now() > deadline) throw new Error(`waiting for ${what}: no card offered ${verbs.join(", ")}`);
    await sleep(1_000);
  }
}

/// Connect over USB, and wait for the card to settle with a firmware verb
/// (returns where it is, [`findFirmwareVerb`]).
async function connect(driver) {
  await driver.pressConnect("USB", { timeoutMs: STEP_MS });
  await driver.pickBoard(BOARD, { timeoutMs: STEP_MS });
  return waitFirmwareVerb(driver, SETTLED_VERBS, { timeoutMs: STEP_MS, what: "the card to settle with a firmware verb" });
}

/// A Lasting verb arms on its first press and acts on its second (the
/// armed button wears `ux-armed`, `ActionButton`). `{ board, verb, bar }`
/// says where it is ([`findFirmwareVerb`]): on the card's face (`bar`
/// null), or in that bar's details.
async function pressLasting(driver, { board, verb, bar }) {
  await driver.pressOffer(verb, { board, bar, timeoutMs: STEP_MS });
  await driver.waitFor(`Boolean(${offerButton(board, verb, Boolean(bar))}?.classList.contains('ux-armed'))`, {
    timeoutMs: 10_000,
    what: `${verb} to arm`,
  });
  await driver.pressOffer(verb, { board, bar, timeoutMs: STEP_MS });
}

/// A verb that may or may not arm: pressed once, and once more when its
/// button is a Lasting one (`ux-armed-chip`). `flash` — the Install a card
/// offers a board that is not running LightPlayer — is the board pick in
/// verb mode: picking the board is the press.
async function pressMaybeArmed(driver, { board, verb, bar }) {
  if (verb === "flash") {
    await driver.pressOffer("flash", { board, timeoutMs: STEP_MS });
    await driver.waitFor(`Boolean(${PANEL})`, { timeoutMs: STEP_MS, what: "the board-model picker" });
    await driver.click(BOARD_MODEL, { scope: PANEL });
    return;
  }
  await driver.waitOffer(verb, { board, bar, timeoutMs: STEP_MS });
  const arms = await driver.evaluate(`Boolean(${offerButton(board, verb, Boolean(bar))}?.classList.contains('ux-armed-chip'))`);
  if (arms) await pressLasting(driver, { board, verb, bar });
  else await driver.pressOffer(verb, { board, bar, timeoutMs: STEP_MS });
}

/// The firmware bar's words for each step of an update
/// (`lpa_devices::FlashStep`, G1 walk 2026-10-03): the bar's work names the
/// step it is on (`board_card/bar_work.rs`), so the walk reads which ones it
/// showed, in order. Read off every card's firmware bar — a board that came
/// back from a pulled cable can be a new card until it says who it is.
const STEP_LABELS = ["Reading the board…", "Waiting for your answer…", "Flashing firmware…", "Moving files…", "Checking the files…"];
/// Page-side: every card's firmware bar, as its line reads (its work's
/// words while it carries work).
const FIRMWARE_LINES = `[...document.querySelectorAll('[data-board-card] [data-bar="firmware"] button[aria-label="Firmware details"]')].map((el) => (el.textContent || '').replace(/\\s+/g, ' ').trim())`;
const STEPS_NOW = `(() => { const lines = ${FIRMWARE_LINES}; return ${JSON.stringify(STEP_LABELS)}.filter((l) => lines.some((t) => t.includes(l))); })()`;
/// Page-side: the firmware bars' lines, joined — a diagnostic.
const FIRMWARE_LINE = `${FIRMWARE_LINES}.filter(Boolean).join(' | ')`;

/// The layout question's words, as core writes them
/// (`device_layout_view.rs`): its two titles, and the refusal's. They sit in
/// the firmware details, which core raises (`UiBarDetails.raised`) while the
/// question or the refusal is open — no longer a dialog.
const QUESTION_WORDS = ["Move this board's files to the new layout", "Put this board's files back", "don't fit the new firmware"];
/// Page-side: the text of the firmware details holding the question (or the
/// refusal), or ''.
const QUESTION_NOW = `(() => {
  const words = ${JSON.stringify(QUESTION_WORDS)};
  for (const panel of document.querySelectorAll('[data-board-card] [data-bar="firmware"] [id^="ux-popover-panel"]')) {
    const text = (panel.textContent || '').replace(/\\s+/g, ' ').trim();
    if (words.some((w) => text.includes(w))) return text;
  }
  return '';
})()`;

/// Whether any wait saw the layout question (or the refusal) open.
let questionSeen = false;

/// The steps the firmware bar names until `until` holds, each once, in the
/// order it first showed them. Polled (every 250 ms): a step can be shorter
/// than one of the page's own mutation-driven waits would notice.
async function watchSteps(driver, until, { timeoutMs, what }) {
  const seen = [];
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    for (const label of await driver.evaluate(STEPS_NOW)) {
      if (seen.includes(label)) continue;
      seen.push(label);
      await onStepShown(label);
    }
    if (!questionSeen && (await driver.evaluate(QUESTION_NOW))) questionSeen = true;
    if (await driver.evaluate(until)) return seen;
    if (Date.now() > deadline) throw new Error(`${what} never came (the card's steps so far: ${seen.join(" → ") || "none"})`);
    await sleep(250);
  }
}

/// Whether `want` appears in `seen` in this order (others may sit between).
function inOrder(seen, want) {
  let at = 0;
  for (const label of seen) if (label === want[at]) at += 1;
  return at === want.length;
}

/// The steps the card showed, across one scenario's waits.
const cardSteps = [];

/// Called once per step the first time the card shows it (`main` points it
/// at a screenshot, so a run leaves one picture per step label).
let onStepShown = async () => {};

/// Until the firmware details hold the layout question (or the refusal);
/// returns their text.
async function awaitQuestion(driver) {
  for (const label of await watchSteps(driver, `Boolean(${QUESTION_NOW})`, { timeoutMs: MIGRATION_MS, what: "the layout question (or refusal)" })) cardSteps.push(label);
  // Behind an open question the firmware bar says it is waiting for the
  // answer.
  for (const label of await driver.evaluate(STEPS_NOW)) if (cardSteps.at(-1) !== label) cardSteps.push(label);
  return driver.evaluate(QUESTION_NOW);
}

/// The card whose open firmware details offer `verb` — the question's own
/// verbs (`continue-update`, `cancel-update`) live there while core raises
/// them.
async function cardAsking(driver, verb) {
  return driver.waitFor(
    `(() => { const mark = [...document.querySelectorAll(${JSON.stringify(`[data-board-card] [data-bar="firmware"] [data-offer-path$="/${verb}"]`)})]
                .find((m) => Boolean(m.closest('.ux-popover-layer')));
              return mark ? mark.closest('[data-board-card]').getAttribute('data-board-card') : false; })()`,
    { timeoutMs: 15_000, what: `the question's \`${verb}\` in the raised firmware details` },
  );
}

/// The firmware details ARE the question, so Continue acts on its one
/// press — it never arms a "Confirm continue" (G1 walk 2026-10-03, Yona:
/// "they already committed to it once"; `ActionButton`'s
/// `asked_by_surface`). One press, and the question closes: core stops
/// offering `continue-update` and lowers the details.
async function pressContinue(driver) {
  const board = await cardAsking(driver, "continue-update");
  await driver.pressOffer("continue-update", { board, bar: "firmware", timeoutMs: 15_000 });
  await driver.waitFor(`!document.querySelector('[data-board-card] [data-offer-path$="/continue-update"]')`, {
    timeoutMs: 15_000,
    what: "the question to close on ONE press of Continue (did it arm instead?)",
  });
}

/// Until the update ends; returns the board terminal's outcome line
/// (`firmware installed — <board>`), or "" when it never came.
async function awaitInstalled(driver) {
  // The activity ends when the firmware bar stops naming a step; whether it
  // succeeded is read after (a terminal line can say "failed" mid-run —
  // esptool-js's own connect retries do).
  await driver.waitFor(`${STEPS_NOW}.length > 0`, { timeoutMs: STEP_MS, what: "the update to start" });
  for (const label of await watchSteps(driver, `${STEPS_NOW}.length === 0`, { timeoutMs: MIGRATION_MS, what: "the update to end" })) cardSteps.push(label);
  // The activity's OUTCOME line is the last thing it writes (after the
  // board-manifest stamp), on the board's terminal in the status corner's
  // details; give it its moment.
  const board = await liveBoard(driver).catch(() => null);
  if (!board) return "";
  return driver.boardSaid("firmware installed", { board, timeoutMs: 60_000 }).catch(() => "");
}

/// Say each new reading of the firmware bar while `waiting` runs: the
/// moment the walk waits for is a progress reading, and a run that misses it
/// should show which readings it saw.
async function narrateFirmwareBar(driver, waiting) {
  let done = false;
  let said = "";
  const narrator = (async () => {
    while (!done) {
      const line = await driver.evaluate(FIRMWARE_LINE).catch(() => "");
      if (line && line !== said) {
        said = line;
        console.log(`    card: ${line}`);
      }
      await sleep(250);
    }
  })();
  try {
    return await waiting;
  } finally {
    done = true;
    await narrator;
  }
}

/// Page-side: every card as a walk reads it — its ref, each bar's line, and
/// the verbs on its face (a record, never a predicate).
const CARDS_NOW = `[...document.querySelectorAll('[data-board-card]')].map((card) => ({
  card: card.getAttribute('data-board-card'),
  bars: Object.fromEntries([...card.querySelectorAll('[data-bar]')].map((bar) => [bar.getAttribute('data-bar'),
    (bar.querySelector('button[aria-label$=" details"]')?.textContent || '').replace(/\\s+/g, ' ').trim()])),
  offers: [...card.querySelectorAll('[data-offer-path]')].filter((m) => !m.closest('.ux-popover-layer'))
    .map((m) => m.getAttribute('data-offer-path').split('/').pop()),
}))`;

/// Page-side: the Bluetooth switch in the card's open connection details
/// (`UiDetailPanel::Bluetooth`, drawn only while the card holds the board's
/// access list or the link is Bluetooth), as `{ on, disabled }`, or null.
function bluetoothSwitch(board) {
  return `(() => { const sw = ${barPanel(board, "connection")}?.querySelector('button[role="switch"][aria-label="Bluetooth"]');
    return sw ? { on: sw.getAttribute('aria-checked') === 'true', disabled: sw.disabled } : null; })()`;
}

/// Page-side: one fact in the card's open `layer` details (`dt` → `dd`), or
/// null.
function detailsLine(board, layer, label) {
  return `(() => { const dt = [...(${barPanel(board, layer)}?.querySelectorAll('dt') ?? [])]
      .find((el) => (el.textContent || '').trim().toLowerCase() === ${JSON.stringify(label.toLowerCase())});
    const dd = dt?.nextElementSibling;
    return dd ? (dd.textContent || '').replace(/\\s+/g, ' ').trim() : null; })()`;
}

/// The card's access rows, read where they live now: the Bluetooth switch
/// (the connection details); who gets in with no password, the access
/// details' "Open to" (null while the card holds no list); and whether the
/// access panel still says it is reading the list.
async function readAccess(driver, board) {
  try {
    await driver.openBar("connection", { board, timeoutMs: STEP_MS });
    const bluetooth = await driver.evaluate(bluetoothSwitch(board));
    await driver.openBar("access", { board, timeoutMs: STEP_MS });
    const open = await driver.evaluate(detailsLine(board, "access", "Open to"));
    const reading = await driver.evaluate(`(${barPanel(board, "access")}?.textContent || '').includes("Reading the device's list")`);
    return { bluetooth, open, reading };
  } finally {
    await driver.closeDetails({ board }).catch(() => {});
  }
}

/// Until the card's access rows are the board's, `timeoutMs` in all: the
/// Bluetooth switch on and usable (it is locked until the board has answered
/// its list, `ui_bluetooth_switch.rs`), then "Open to" saying `want`.
async function awaitAccess(driver, board, want, timeoutMs) {
  const deadline = Date.now() + timeoutMs;
  const left = () => Math.max(1_000, deadline - Date.now());
  try {
    await driver.openBar("connection", { board, timeoutMs: left() });
    await driver.waitFor(`(() => { const sw = ${bluetoothSwitch(board)}; return Boolean(sw) && sw.on && !sw.disabled; })()`, {
      timeoutMs: left(),
      what: "the Bluetooth switch on and usable",
    });
    await driver.openBar("access", { board, timeoutMs: left() });
    await driver.waitFor(`${detailsLine(board, "access", "Open to")} === ${JSON.stringify(want)}`, {
      timeoutMs: left(),
      what: `the access details' Open to: ${want}`,
    });
    return true;
  } catch {
    return false;
  } finally {
    await driver.closeDetails({ board }).catch(() => {});
  }
}

/// The cable comes out of a USB-powered board: power and data together.
async function pullCable(driver) {
  return driver.evaluate(
    `(async () => {
       const bus = window.__lpEmuSerial.bus;
       const port = bus.requireLivePort(${JSON.stringify(BOARD)});
       // The lines go with the cable (a power-cycle with DTR/RTS still held
       // in the bootloader dance would come back in the download strap),
       // then the power.
       await bus.detach(${JSON.stringify(BOARD)});
       await port.emulator.command("power-cycle");
       // A power-on brings the emulated USB host back attached; the cable is
       // still out until the walk plugs it back in.
       await port.emulator.detach().catch(() => {});
       return "pulled";
     })()`,
    { awaitPromise: true },
  );
}

// --- the tab lane (W12): the board is a Worker in the page ------------------

/// Put the fixture chip into the tab board and power it on from it, and start
/// keeping what the board says (its USB bytes and its UART0 text, in arrival
/// order — the door's console file holds the same two). Its chip and console
/// live in the page, so the walk reads both back out of it.
async function seedTabBoard(driver) {
  return driver.evaluate(
    `(async () => {
       const emu = window.__lpEmuSerial.bus.requireLivePort(${JSON.stringify(BOARD)}).emulator;
       window.__walkEmu = emu;
       const latin1 = new TextDecoder("latin1");
       window.__walkConsole = "";
       emu._hub.onBytes((bytes) => { window.__walkConsole += latin1.decode(bytes); });
       emu._hub.onConsole((text) => { window.__walkConsole += text; });
       const bytes = new Uint8Array(await (await fetch("/__walk/fixture.bin", { cache: "no-store" })).arrayBuffer());
       await emu.putFlash(bytes);
       await emu.command("power-cycle");
       return bytes.length;
     })()`,
    { awaitPromise: true, timeoutMs: 120_000 },
  );
}

/// The tab board's whole chip, written to `<out>/<name>` by the bundle server.
async function saveTabChip(driver, name) {
  return driver.evaluate(
    `(async () => {
       const bytes = await window.__walkEmu.getFlash();
       const answer = await fetch("/__walk/chip?name=" + encodeURIComponent(${JSON.stringify(name)}), { method: "POST", body: bytes });
       if (!answer.ok) throw new Error("the walk server answered " + answer.status);
       return bytes.length;
     })()`,
    { awaitPromise: true, timeoutMs: 120_000 },
  );
}

// --- the verdicts ------------------------------------------------------------

function boardConsole(door) {
  try {
    return readFileSync(path.join(door.consoleDir, `${BOARD}.console.log`), "utf8");
  } catch {
    return "";
  }
}

/// Every `"<key>":"<value>"` the board said, value as written on the
/// console. The console holds the link's raw frames, so a frame boundary
/// can fall INSIDE a value: its CRC and the next header ride between the
/// letters (`"fs":"\x0cmounted<crc>j(…"`), and some of those bytes are
/// printable. So a value is everything up to the next quote, read by the
/// callers below as letters that must CONTAIN what they look for, in order.
function framedValues(said, key) {
  return [...said.matchAll(new RegExp(`"${key}":"([^"]{0,64})"`, "g"))].map((m) => m[1]);
}

/// The hello's filesystem state in a framed value: the one known state
/// whose letters it holds in order (the longest, should two fit).
function fsState(value) {
  const letters = value.replace(/[^a-z_]/g, "");
  return ["legacy_held", "formatted", "mounted", "memory"].find((state) => subsequence(state, letters));
}

/// Whether `needle`'s characters appear in `hay` in order.
function subsequence(needle, hay) {
  let at = 0;
  for (const char of hay) if (char === needle[at]) at += 1;
  return at === needle.length;
}

/// The fixture's files against the chip's, by SHA-256.
function compareFiles(expected, actual) {
  const want = new Map(expected.files.map((f) => [f.path, f.sha256]));
  const got = new Map(actual.files.map((f) => [f.path, f.sha256]));
  const missing = [...want.keys()].filter((p) => !got.has(p));
  const changed = [...want.keys()].filter((p) => got.has(p) && got.get(p) !== want.get(p));
  const extra = [...got.keys()].filter((p) => !want.has(p));
  return { missing, changed, extra, same: missing.length === 0 && changed.length === 0 };
}

async function main() {
  const { scenario, family, out } = args();
  const spec = SCENARIOS[scenario];
  for (const [what, at] of [["the release Studio bundle (just studio-web-story-build)", PUBLIC], ["the packaged firmware (just studio-firmware-package-served)", path.join(FIRMWARE, "esp32c6-4mb")], ["a debug lp-cli (cargo build -p lp-cli)", LP_CLI]]) {
    if (!existsSync(at)) throw new Error(`missing ${what}: ${at}`);
  }
  const continuing = spec.fixture === null;
  if (!continuing) rmSync(out, { recursive: true, force: true });
  mkdirSync(out, { recursive: true });
  const shots = path.join(out, `shots-${scenario}`);
  mkdirSync(shots, { recursive: true });
  const stateDir = path.join(out, "state");
  const profileDir = path.join(out, "chrome-profile");

  console.log(`\nTHE MIGRATION WALK — ${scenario}: ${spec.describe}`);
  let fixtureChip = path.join(out, "fixture.bin");
  if (!continuing) fixtureChip = buildFixture(spec.fixture, out);
  const beforeBypass = path.join(out, "fixture-before-bypass.bin");
  const fixtureReport = report(spec.fixture === "bypassed" ? beforeBypass : fixtureChip);
  console.log(`  fixture: ${fixtureReport.layout} — ${fixtureReport.fileCount} files, ${fixtureReport.totalBytes} B`);

  if (spec.tab) BOARD = "tab-c6";
  const door = spec.tab ? null : await startDoor({
    root: ROOT,
    id: "migration",
    boards: [`${BOARD}=${fixtureChip},kind=rom-up,mac=${MAC}`],
    stateDir,
    consoleDir: path.join(out, "console"),
    logFile: path.join(out, `serve-${scenario}.log`),
    fresh: !continuing,
  });
  const bundle = await serveBundle({ fixtureChip, out });
  const sink = await startSink();
  const url = studioUrlFor({
    studioPort: bundle.address().port,
    doorAddr: door?.addr ?? null,
    sinkUrl: `http://127.0.0.1:${sink.address().port}/ingest`,
  });
  console.log(`  ${door ? `door ${door.addr}` : "no door: the board is a Worker in the page"} · studio ${url}`);

  const verdict = { scenario, describe: spec.describe, steps: [], ok: false };
  const step = (name, ok, detail) => {
    verdict.steps.push({ name, ok, detail });
    console.log(`  ${ok ? "✓" : "✗"} ${name}${detail ? ` — ${detail}` : ""}`);
    if (!ok) throw new Error(`${name}: ${detail}`);
  };
  const driver = await StudioDriver.launch({ profileDir });
  const shot = async (name) => {
    try {
      await driver.screenshot(path.join(shots, `${verdict.steps.length + 1}-${name}.png`));
    } catch { /* the page may be gone */ }
  };
  onStepShown = async (label) => shot(`step-${label.replace(/[^a-z]+/gi, "-").replace(/^-|-$/g, "").toLowerCase()}`);
  let fatal = null;
  // What the board held when the update began (see the question step);
  // the fixture until then.
  let baseline = fixtureReport;
  // The card's access rows (judged last; null = not judged).
  let accessOk = null;
  // What the board has said: the door's console file, or (tab) the page's.
  const consoleText = async () => (door ? boardConsole(door) : await driver.evaluate(`window.__walkConsole ?? ""`));
  try {
    await driver.navigate(url);
    // The board's card is named by its MAC (`BoardRef`: `mac-<hex>`), as the
    // shim lists it — the door's `mac=`, or the tab board's own.
    const shimBoards = await driver.awaitShim();
    const mac = shimBoards?.find((b) => b.boardId === BOARD)?.mac ?? (door ? MAC : null);
    CARD = mac ? boardPath(mac) : null;
    await driver.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: STUDIO_LOAD_MS, what: "Studio to load" });
    if (spec.tab) {
      const seeded = await seedTabBoard(driver);
      step("the tab board holds the fixture chip", seeded === readFileSync(fixtureChip).length, `${seeded} B`);
      // Its own power-on boot, to the server loop, before Studio is asked
      // to find it.
      // Polled from here: the page's own waits re-check on DOM mutations,
      // and the board's words are not in the DOM.
      const bootBy = Date.now() + STEP_MS;
      while (!(await consoleText()).includes("starting server loop")) {
        if (Date.now() > bootBy) throw new Error("the seeded board never reached its server loop");
        await new Promise((r) => setTimeout(r, 1_000));
      }
    }
    const settled = await connect(driver);
    await shot("connected");
    step("connected", true, `the card settled (${settled.verb}${settled.bar ? ` in its ${settled.bar} details` : ""} on ${settled.board})`);

    // The door writes the console back every 2 s: let what the board has
    // said so far land before marking where the update's words begin.
    await new Promise((r) => setTimeout(r, 5_000));
    const before = await consoleText();
    if (scenario === "W9") {
      const said = await consoleText();
      step(
        "the board held its files",
        said.includes("holding: not formatting") || said.includes("legacy-layout filesystem found"),
        (said.match(/\[FS\][^\n]*/) ?? [""])[0].slice(0, 120),
      );
      // A held board's device store waits with its files: it keeps Bluetooth
      // off (it must not be more open than that store), and Studio neither
      // writes nor lists access for it (G1 rehearsal, 2026-10-03).
      step(
        "the held board kept Bluetooth off",
        said.includes("[ble] off (files held") && !said.includes("[ble] enabled"),
        (said.match(/\[ble\][^\n]*/) ?? ["(no [ble] line)"])[0].slice(0, 120),
      );
      // No access list: the access details carry no "This board" section,
      // whose "Open to" core draws only while it holds the board's list
      // (`access_bar.rs`), beside the access panel itself.
      const listed = await driver.detailsFact("access", "Open to", { board: settled.board, timeoutMs: STEP_MS });
      step("the card shows no access list for it", listed === null, listed === null ? "" : `Open to: ${listed}`);
      const finish = await findFirmwareVerb(driver, ["finish-update"]);
      step("the card offers Finish update", Boolean(finish), finish ? `\`finish-update\` on ${finish.bar ? `its ${finish.bar} details` : "its firmware bar"}` : "");
      await pressLasting(driver, finish);
    } else if (scenario === "W7b") {
      const restore = await findFirmwareVerb(driver, ["restore-files"]);
      step("formatted board offers its backup", Boolean(restore), restore ? `\`restore-files\` on ${restore.bar ? `its ${restore.bar} details` : "its firmware bar"}` : "");
      await pressLasting(driver, restore);
    } else {
      // Update: the firmware bar's action on a board older than this
      // Studio, otherwise a verb in its firmware details.
      await pressLasting(driver, await waitFirmwareVerb(driver, ["update-firmware"], { timeoutMs: STEP_MS, what: "Update on the card" }));
    }

    if (scenario === "W2") {
      const text = await awaitInstalled(driver);
      await shot("updated");
      // The firmware details never held the question, at any reading.
      step("no question asked", !questionSeen, "");
      step("update installed", text.includes("firmware installed"), text.match(/firmware installed[^\n]*/)?.[0] ?? "");
    } else {
      const page = await awaitQuestion(driver);
      await shot("question");
      // The board's files as the update found them: the door wrote the chip
      // back when the inspection's bootloader session closed, and nothing
      // has been written yet. (Connecting over USB legitimately changes one
      // file before this — the browser's key goes into /.lp/access.json — so
      // THIS, not the fixture, is what a migration must carry.)
      const asFound = path.join(out, `as-found-${scenario}.bin`);
      const live = path.join(stateDir, `${BOARD}.flash.bin`);
      // The door writes the chip back every 2 s WHEN IT IS DIRTY: no file
      // after a few cadences means nothing was ever written — the chip is
      // still the fixture it was seeded from.
      await new Promise((r) => setTimeout(r, 5_000));
      if (door) cpSync(existsSync(live) ? live : fixtureChip, asFound);
      else await saveTabChip(driver, path.basename(asFound));
      try {
        baseline = report(asFound);
        verdict.connectChanged = compareFiles(fixtureReport, baseline);
      } catch {
        // A held board's table names a filesystem that is not there yet
        // (its files are at the old offset, untouched, and the board ran on
        // a memory filesystem): what it holds is still the fixture's.
        verdict.connectChanged = "the chip's own table names no mountable filesystem (held)";
      }
      if (scenario === "W3") {
        step("refused", page.includes("don't fit the new firmware"), "");
      } else {
        step("asked", page.includes("Move this board's files") || page.includes("Put this board's files back"), "");
        if (scenario === "W4") {
          // Cancel, in the raised firmware details: it loses nothing, so it
          // acts on one press (pressed again only should it ever arm).
          await pressMaybeArmed(driver, { board: await cardAsking(driver, "cancel-update"), verb: "cancel-update", bar: "firmware" });
          await driver.waitFor(`!${QUESTION_NOW}`, { timeoutMs: STEP_MS, what: "the question to close" });
          await waitFirmwareVerb(driver, ["update-firmware"], { timeoutMs: MIGRATION_MS, what: "the board back on its firmware" });
          await shot("cancelled");
        } else {
          await pressContinue(driver);
          step("Continue acted on one press", true, "the question closed on the first press; nothing armed");
          if (scenario === "W5") {
            // The firmware's own write: esptool-js writes the merged image
            // from 0x0 first and the filesystem (0x35…) only after it, so a
            // reading inside the app's range is the firmware mid-write. Its
            // `Writing at 0x…` lines are on the board's terminal (the status
            // corner's details); the addresses only climb, so the first line
            // in [0x100000, 0x300000) is the last one written when it lands.
            const writing = await driver
              .boardSaid(/Writing at 0x[12][0-9a-f]{5}(?![0-9a-f])/, { board: await liveBoard(driver), timeoutMs: MIGRATION_MS })
              .catch((error) => {
                throw new Error(`the moment to pull the cable never came (${error.message})`);
              });
            await shot("before-pull");
            step("cable pulled during the firmware write", (await pullCable(driver)) === "pulled", writing);
            await driver.attach(BOARD);
            await connect(driver).catch(() => {});
            // What the board does on its power-on boot (a part-written app),
            // in its own words, before anything is asked of it again.
            await sleep(15_000);
            verdict.afterPull = { card: await driver.evaluate(CARDS_NOW), board: (await consoleText()).slice(-1500).replace(/[\x00-\x09\x0b-\x1f\x7f-\xff]/g, "") };
            await shot("after-pull");
            // A part-written app does not boot ("No bootable app partitions"),
            // so the card meets a board that is not running LightPlayer: its
            // way back is the card's primary for that, Install (`flash`, the
            // board pick) — the same verb a blank board offers.
            let way = await findFirmwareVerb(driver, ["finish-update", "restore-files", "update-firmware"]);
            if (!way) {
              for (const board of await driver.cardPaths()) {
                if (await driver.offered("flash", { board })) {
                  way = { board, verb: "flash", bar: null };
                  break;
                }
              }
            }
            step(
              "the card offers a way back",
              Boolean(way),
              way ? `\`${way.verb}\`${way.bar ? ` in its ${way.bar} details` : ""} on ${way.board}` : JSON.stringify(verdict.afterPull.card).slice(0, 300),
            );
            await pressMaybeArmed(driver, way);
            const again = await awaitQuestion(driver);
            await shot("question-again");
            step("asked again", again.includes("Move this board's files") || again.includes("Put this board's files back"), "");
            await pressContinue(driver);
            const text = await awaitInstalled(driver);
            await shot("installed");
            step("update installed", text.includes("firmware installed"), text.match(/firmware installed[^\n]*/)?.[0] ?? "");
          } else if (scenario === "W7a") {
            // The firmware bar names the step, not its address; the board's
            // terminal carries esptool-js's own lines: the filesystem body
            // (block 2 on, so 0x352000) is being written once esptool-js
            // says so.
            // Say what the firmware bar reads while waiting: the moment is
            // a progress reading, and a run that misses it should show which
            // readings it saw.
            const board = await liveBoard(driver);
            await narrateFirmwareBar(driver, driver.boardSaid("Writing at 0x352000", { board, timeoutMs: MIGRATION_MS })).catch((error) => {
              throw new Error(`the moment to pull the cable never came (${error.message})`);
            });
            await shot("before-pull");
            const reading = await driver.evaluate(FIRMWARE_LINE);
            step("cable pulled", (await pullCable(driver)) === "pulled", reading);
            await driver.attach(BOARD);
            await connect(driver).catch(() => {});
            // The board's own power-on boot, to its end (a port must be
            // open for the boot's words to flow): the card can settle on an
            // early reading of the link.
            const bootBy = Date.now() + STEP_MS;
            for (;;) {
              // Its power-on banner went out with nobody listening (the
              // cable was out); what came after the bootloader session is
              // the firmware's own start.
              const text = await consoleText();
              if (text.lastIndexOf("starting server loop") > text.lastIndexOf("waiting for download")) break;
              if (Date.now() > bootBy) throw new Error("the board never finished its power-on boot after the pull");
              await new Promise((r) => setTimeout(r, 1_000));
            }
            await waitFirmwareVerb(driver, ["finish-update", "restore-files", "update-firmware"], { timeoutMs: STEP_MS, what: "the board back after the pull" });
            await shot("after-pull");
          } else {
            const text = await awaitInstalled(driver);
            await shot("installed");
            step("update installed", text.includes("firmware installed"), text.match(/firmware installed[^\n]*/)?.[0] ?? "");
          }
        }
      }
    }

    // Wait for the board's own last boot to finish (its words, not
    // Studio's): the card can come back before the firmware has said what
    // its filesystem did.
    const settleBy = Date.now() + STEP_MS;
    for (;;) {
      const text = await consoleText();
      const last = text.lastIndexOf("ESP-ROM:");
      const tail = last >= 0 ? text.slice(last) : "";
      if (tail.includes("starting server loop") || tail.includes("waiting for download")) break;
      if (Date.now() > settleBy) {
        verdict.note = "the board's last boot had not finished when the walk stopped";
        break;
      }
      await new Promise((r) => setTimeout(r, 1_000));
    }
    // The card's access rows, once the board is back on its own firmware
    // (G1, 2026-10-03: after the spare's migration the Bluetooth switch was
    // locked, "Who has access 0" on a fresh profile — the connect's refused
    // add had thrown away the list it read). Studio reads the list once per
    // connection; wait for that, then say what the card shows. W7a's board
    // is formatted, so its list is a different one and is not judged here.
    // The rows live in the card's details now: the Bluetooth switch in the
    // connection details, who gets in with no password in the access
    // details ("Open to"), the access panel beside it.
    if (scenario !== "W7a") {
      const board = await liveBoard(driver).catch(() => CARD);
      const ready = board ? await awaitAccess(driver, board, expectedOpenSummary(), 60_000) : false;
      const seen = board ? await readAccess(driver, board).catch((error) => ({ error: error.message })) : { error: "no card for the board" };
      verdict.access = seen;
      if (FULL_ACCESS) {
        // The access panel says why this browser is not on the list (core's
        // `FULL_SENTENCE`, `device_access_ops.rs`), in the access details.
        verdict.access.full = false;
        if (board) {
          try {
            await driver.openBar("access", { board, timeoutMs: STEP_MS });
            verdict.access.full = await driver
              .waitFor(`(${barPanel(board, "access")}?.textContent || '').includes("device is full")`, { timeoutMs: 10_000, what: "the access panel to say the device is full" })
              .then(() => true, () => false);
            await shot("access-panel");
          } catch {
            /* judged below: `full` stays false */
          } finally {
            await driver.closeDetails({ board }).catch(() => {});
          }
        }
      }
      // Judged after the files (below): a run that fails here still says
      // whether every file moved.
      accessOk = ready && (!FULL_ACCESS || verdict.access.full);
      console.log(`    card access rows: ${JSON.stringify(verdict.access)}`);
    }

    // The tab's chip and console are the page's: read both before it goes.
    let tabConsole = "";
    if (!door) {
      tabConsole = await consoleText();
      mkdirSync(stateDir, { recursive: true });
      await saveTabChip(driver, path.join("state", `${BOARD}.flash.bin`));
    }
    writeFileSync(path.join(out, "page-console.log"), driver.consoleLines().join("\n"));
    // What the Mac serial model (on for a page on a Mac) dropped: on a run
    // that passed, the update's reads must have lost nothing.
    verdict.macTtyDrops = driver.consoleLines("Mac serial model dropped").length;
    await driver.close();
    // The door writes the chip and the console back on shutdown (and every
    // 2 s before it): read both after it has stopped.
    if (door) {
      await stopDoor(door);
      await new Promise((r) => setTimeout(r, 3_000));
    }
    // The board's own words since the update began. The link's framing
    // interleaves binary headers with the JSON; drop control bytes and read
    // the hello's own fields.
    // Everything from the first boot AFTER the update began: the boots
    // before it are counted by their ROM banners (the door rewrites the
    // transcript whole, so neither a length nor a text tail is a safe
    // anchor — a heartbeat repeats).
    const whole = door ? boardConsole(door) : tabConsole;
    const bootsBefore = before.split("ESP-ROM:").length - 1;
    let from = -1;
    for (let i = 0; i <= bootsBefore; i += 1) {
      from = whole.indexOf("ESP-ROM:", from + 1);
      if (from < 0) break;
    }
    const said = (from >= 0 ? whole.slice(from) : "").replace(/[\x00-\x09\x0b-\x1f\x7f-\xff]/g, "");
    verdict.console = {
      fs: [...new Set(framedValues(said, "fs").map(fsState).filter(Boolean))],
      deviceUid: [...new Set(framedValues(said, "deviceUid").filter((v) => subsequence(UID, v)).map(() => UID))],
      lines: said.split("\n").filter((l) => /\[FS\]|\[INIT\] Flash filesystem|legacy-layout|found \d+ entries in \/projects/.test(l)).map((l) => l.trim().slice(-120)),
    };
    const written = path.join(stateDir, `${BOARD}.flash.bin`);
    // Never written back = never written: the chip is still the fixture.
    const chip = existsSync(written) ? written : fixtureChip;
    const after = report(chip);
    verdict.after = { layout: after.layout, files: after.fileCount, bytes: after.totalBytes };
    // A b half carries the files its a half's update found on the board —
    // they went into the backup (W7) in between.
    if (continuing) baseline = report(path.join(out, `as-found-${family}a.bin`));
    const files = compareFiles(baseline, after);
    verdict.files = files;
    const bytes = readFileSync(chip);
    const legacySuperblock = bytes.subarray(0x310008, 0x310010).toString() === "littlefs";
    switch (scenario) {
      case "W1":
      case "W5":
      case "W12":
      case "W9":
      case "W7b": {
        // A held board (W9) gets no access write while it holds its files
        // (its store waits with them), so the connect's ordinary add of this
        // browser's key lands AFTER the move, on the real store: the one
        // file Studio itself writes may differ from the fixture, and the
        // card's count below (fixture entries + this browser) says the old
        // entries survived. Everywhere else the add came before the
        // as-found read, so every byte must match.
        const studiosAfterHeld = scenario === "W9" && files.missing.length === 0 && files.extra.length === 0
          && files.changed.length > 0 && files.changed.every((p) => p === "/.lp/access.json");
        step(
          studiosAfterHeld ? "files moved, byte for byte (but /.lp/access.json: Studio's key add after the move)" : "files moved, byte for byte",
          files.same || studiosAfterHeld,
          JSON.stringify(files),
        );
        step("the chip is on the new layout", /0x350000/.test(after.layout) || !/pre-2026-10/.test(after.layout), after.layout);
        step("old superblock retired", !legacySuperblock, "");
        // The card named the update's steps as they happened (G1 walk,
        // 2026-10-03: it said "Flashing firmware…" from the first read to
        // the last check). W5's first pass was cut short by the cable; its
        // second pass still shows every step, in this order.
        step(
          "the card named each step",
          inOrder(cardSteps, ["Reading the board…", "Waiting for your answer…", "Flashing firmware…", "Moving files…", "Checking the files…"]),
          cardSteps.join(" → "),
        );
        step(
          "the board mounted them, as itself",
          said.includes("Flash filesystem mounted") && !said.includes("holding: not formatting")
            && verdict.console.fs.at(-1) === "mounted" && verdict.console.deviceUid.includes(UID),
          JSON.stringify(verdict.console),
        );
        break;
      }
      case "W2": {
        // A plain update moves nothing; Studio's own documented writes are
        // the only changes: the browser's key into /.lp/access.json on a
        // USB connect, and the board manifest it stamps after any update.
        const studios = new Set(["/.lp/access.json", "/hardware.json"]);
        step(
          "no file lost, moved or added; only Studio's own stamps changed",
          files.missing.length === 0 && files.extra.length === 0 && files.changed.every((p) => studios.has(p)),
          JSON.stringify(files),
        );
        step("the chip is still on the new layout", /0x350000/.test(after.layout), after.layout);
        break;
      }
      case "W3":
      case "W4": {
        step("the card read the board before it answered", cardSteps.includes("Reading the board…"), cardSteps.join(" → "));
        // Against the chip as the update found it (after the connect's own
        // access-file write): a refusal or a cancel writes NOTHING.
        const fixture = readFileSync(path.join(out, `as-found-${scenario}.bin`));
        const differs = [];
        for (let at = 0; at < fixture.length; at += 0x1000) {
          if (!fixture.subarray(at, at + 0x1000).equals(bytes.subarray(at, at + 0x1000))) differs.push(`0x${at.toString(16)}`);
        }
        verdict.differingSectors = differs;
        step("files untouched", files.same, JSON.stringify(files));
        step("the chip is as the update found it", differs.length === 0, differs.length ? `${differs.length} sectors differ: ${differs.slice(0, 8).join(", ")}` : "byte-identical");
        step(
          "the board went back to its firmware and its files",
          said.includes("Flash filesystem mounted") && !said.includes("legacy-layout filesystem found"),
          JSON.stringify(verdict.console),
        );
        break;
      }
      case "W7a":
        verdict.note = "the outcome depends on where in the filesystem write the cable came out";
        step(
          "the board came back with a fresh filesystem (its files are in the backup)",
          said.includes("Formatted and mounted fresh filesystem") || verdict.console.fs.includes("formatted"),
          JSON.stringify(verdict.console),
        );
        break;
      default:
        break;
    }
    if (accessOk !== null) {
      step(
        "the card's access rows are the board's: Bluetooth on and usable, its store's access read",
        accessOk,
        JSON.stringify(verdict.access),
      );
    }
    verdict.ok = true;
  } catch (error) {
    fatal = error;
    await shot("failure");
    console.error(`\n✗ ${error.message}`);
    // Studio's own console, for the why (esptool-js's errors, the Mac
    // serial model's drops) — it dies with the page.
    try {
      writeFileSync(path.join(out, "page-console.log"), driver.consoleLines().join("\n"));
    } catch { /* the page may be gone */ }
    // The tab's console dies with the page: keep it beside the verdict, the
    // way the door keeps its console file.
    if (!door) {
      try {
        writeFileSync(path.join(out, `${BOARD}.console.log`), await consoleText(), "latin1");
      } catch { /* the page may be gone */ }
    }
    try { await driver.close(); } catch { /* gone */ }
    if (door) await stopDoor(door);
  } finally {
    bundle.close();
    sink.close();
    writeFileSync(path.join(out, `verdict-${scenario}.json`), JSON.stringify(verdict, null, 2));
    console.log(`  verdict → ${path.join(out, `verdict-${scenario}.json`)} · screenshots → ${shots}`);
  }
  process.exit(fatal ? 1 : 0);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
