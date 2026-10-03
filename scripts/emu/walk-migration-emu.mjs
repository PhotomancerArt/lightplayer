#!/usr/bin/env node
// THE MIGRATION WALK (C6 repartition, P08; `just walk-migration-emu <scenario>`).
//
// Real Studio, in headless Chrome, updating an emulated ESP32-C6 that is a
// "fielded board": the current firmware on the frozen pre-2026-10 partition
// table, its files in a 240-block littlefs at 0x310000 — the board every
// shipped C6 is today. Studio's own Update firmware runs the layout
// migration through esptool-js, over the `?emu=` virtual serial port, into
// the emulated mask ROM's download console.
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
//       files, and the card's Finish update moves them
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
// Studio's words are used only to know when to click.
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

import { StudioDriver } from "./studio-driver.mjs";
import { serveStudioBundle, startDoor, stopDoor, studioUrlFor, walkPort } from "./emulated-lane.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const PUBLIC = path.join(ROOT, "target/dx/lpa-studio-web/release/web/public");
const FIRMWARE = path.join(ROOT, "target/studio-web-assets/firmware");
const LP_CLI = path.join(ROOT, "target/debug/lp-cli");
/// The door's board id; the tab lane (W12) sets it to the tab's own board.
let BOARD = "c6-a";
const MAC = "60:55:f9:0a:0b:0c";
const UID = "dev0000000000000011";
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
    console.error(`usage: node scripts/emu/walk-migration-emu.mjs <${Object.keys(SCENARIOS).join("|")}> [--out <dir>]`);
    process.exit(2);
  }
  const outAt = argv.indexOf("--out");
  // The b halves continue their a half: same state, same browser profile.
  const family = scenario.replace(/[ab]$/, "");
  const out = outAt >= 0 ? argv[outAt + 1] : path.join(ROOT, "target/walk-migration-emu", family);
  return { scenario, family, out };
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
  files.push([
    "/.lp/access.json",
    Buffer.from(
      '{"version":2,"bleEnabled":true,"open":false,"secrets":[{"label":"walk browser","kind":"browser","tier":"edit","salt":"AAECAwQFBgcICQoLDA0ODw==","iterations":1,"k":"ICEiIyQlJicoKSorLC0uLzAxMjM0NTY3ODk6Ozw9Pj8="}]}',
    ),
  ]);
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
const PAGE_TEXT = `(document.body?.innerText || '')`;

async function connect(driver) {
  await driver.clickWhenReady("via USB", { timeoutMs: STEP_MS });
  await driver.pickBoard(BOARD, { timeoutMs: STEP_MS });
  await driver.waitFor(`${MAIN_TEXT}.includes('Update firmware') || ${MAIN_TEXT}.includes('Finish update') || ${MAIN_TEXT}.includes('Restore files')`, {
    timeoutMs: STEP_MS,
    what: "the card to settle with a firmware verb",
  });
}

/// A Lasting action arms on its first click and acts on its second.
async function pressLasting(driver, text) {
  await driver.clickWhenReady(text, { timeoutMs: STEP_MS });
  await driver.waitFor(`[...document.querySelectorAll('.ux-armed')].length > 0`, { timeoutMs: 10_000, what: `${text} to arm` });
  await driver.click(text);
}

/// A verb that may or may not arm (the not-LightPlayer face's Flash is a
/// one-click row action today): click, and click again only if it armed.
async function pressMaybeArmed(driver, text) {
  await driver.clickWhenReady(text, { timeoutMs: STEP_MS });
  const armed = await driver
    .waitFor(`[...document.querySelectorAll('.ux-armed')].length > 0`, { timeoutMs: 3_000, what: `${text} to arm` })
    .then(() => true, () => false);
  if (armed) await driver.click(text);
}

async function awaitQuestion(driver) {
  await driver.waitFor(`${PAGE_TEXT}.includes("Move this board's files to the new layout") || ${PAGE_TEXT}.includes("Put this board's files back") || ${PAGE_TEXT}.includes("don't fit the new firmware")`, {
    timeoutMs: MIGRATION_MS,
    what: "the layout question (or refusal)",
  });
  return driver.evaluate(PAGE_TEXT);
}

async function awaitInstalled(driver) {
  // The activity ends when the card stops saying it is flashing; whether it
  // succeeded is read after (a terminal line can say "failed" mid-run —
  // esptool-js's own connect retries do).
  await driver.waitFor(`${MAIN_TEXT}.includes('Flashing firmware')`, { timeoutMs: STEP_MS, what: "the update to start" });
  await driver.waitFor(`!${MAIN_TEXT}.includes('Flashing firmware')`, {
    timeoutMs: MIGRATION_MS,
    what: "the update to end",
  });
  // The activity's OUTCOME line is the last thing it writes (after the
  // board-manifest stamp); give it its moment before anything is closed.
  await driver
    .waitFor(`${MAIN_TEXT}.includes('firmware installed')`, { timeoutMs: 60_000, what: "the outcome line" })
    .catch(() => {});
  return driver.evaluate(MAIN_TEXT);
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
  let fatal = null;
  // What the board held when the update began (see the question step);
  // the fixture until then.
  let baseline = fixtureReport;
  // What the board has said: the door's console file, or (tab) the page's.
  const consoleText = async () => (door ? boardConsole(door) : await driver.evaluate(`window.__walkConsole ?? ""`));
  try {
    await driver.navigate(url);
    await driver.awaitShim();
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
    await connect(driver);
    await shot("connected");
    step("connected", true, "the card settled");

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
      step("the card offers Finish update", (await driver.evaluate(MAIN_TEXT)).includes("Finish update"), "");
      await pressLasting(driver, "Finish update");
    } else if (scenario === "W7b") {
      step("formatted board offers its backup", (await driver.evaluate(MAIN_TEXT)).includes("Restore files"), "");
      await pressLasting(driver, "Restore files");
    } else {
      await pressLasting(driver, "Update firmware");
    }

    if (scenario === "W2") {
      const text = await awaitInstalled(driver);
      await shot("updated");
      step("no question asked", !text.includes("Move this board's files"), "");
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
          await driver.click("Cancel", { scope: `document.querySelector('[role="dialog"]')` });
          await driver.waitFor(`!${PAGE_TEXT}.includes("Move this board's files")`, { timeoutMs: STEP_MS, what: "the question to close" });
          await driver.waitFor(`${MAIN_TEXT}.includes('Update firmware')`, { timeoutMs: MIGRATION_MS, what: "the board back on its firmware" });
          await shot("cancelled");
        } else {
          await pressLasting(driver, "Continue");
          if (scenario === "W5") {
            // The firmware's own write: esptool-js writes the merged image
            // from 0x0 first and the filesystem (0x35…) only after it, so a
            // reading inside the app's range is the firmware mid-write.
            const moment = `(() => {
              const at = [...${MAIN_TEXT}.matchAll(/Writing at 0x([0-9a-f]+)/g)].map((m) => parseInt(m[1], 16));
              const last = at.length ? at[at.length - 1] : 0;
              return last >= 0x100000 && last < 0x300000;
            })()`;
            const deadline = Date.now() + MIGRATION_MS;
            while (!(await driver.evaluate(moment))) {
              if (Date.now() > deadline) throw new Error("the moment to pull the cable never came");
              await new Promise((r) => setTimeout(r, 250));
            }
            await shot("before-pull");
            const writing = (await driver.evaluate(MAIN_TEXT)).match(/Writing at 0x[0-9a-f]+[^\n]*/g)?.pop() ?? "";
            step("cable pulled during the firmware write", (await pullCable(driver)) === "pulled", writing);
            await driver.attach(BOARD);
            await connect(driver).catch(() => {});
            // What the board does on its power-on boot (a part-written app),
            // in its own words, before anything is asked of it again.
            await new Promise((r) => setTimeout(r, 15_000));
            const afterPull = await driver.evaluate(MAIN_TEXT);
            verdict.afterPull = { card: afterPull.slice(0, 1500), board: (await consoleText()).slice(-1500).replace(/[\x00-\x09\x0b-\x1f\x7f-\xff]/g, "") };
            await shot("after-pull");
            // A part-written app does not boot ("No bootable app partitions"),
            // so the card meets a board that is not running LightPlayer: its
            // way back is the face for that, Flash firmware with the board
            // picked — the same verb a blank board offers.
            let verb = ["Finish update", "Restore files", "Update firmware"].find((v) => afterPull.includes(v));
            if (!verb && afterPull.includes("boards fit") && afterPull.includes("Flash firmware")) {
              await driver.clickWhenReady("boards fit", { timeoutMs: STEP_MS });
              await driver.waitFor(`Boolean(document.querySelector('[id^="ux-popover-panel"]'))`, { timeoutMs: STEP_MS, what: "the board-model picker" });
              await driver.click("XIAO ESP32-C6", { scope: `document.querySelector('[id^="ux-popover-panel"]')` });
              verb = "Flash firmware";
            }
            step("the card offers a way back", Boolean(verb), verb ?? afterPull.slice(0, 300));
            await pressMaybeArmed(driver, verb);
            const again = await awaitQuestion(driver);
            await shot("question-again");
            step("asked again", again.includes("Move this board's files") || again.includes("Put this board's files back"), "");
            await pressLasting(driver, "Continue");
            const text = await awaitInstalled(driver);
            await shot("installed");
            step("update installed", text.includes("firmware installed"), text.match(/firmware installed[^\n]*/)?.[0] ?? "");
          } else if (scenario === "W7a") {
            // The card names the step, not its percentage; the terminal
            // carries esptool-js's own lines: the filesystem body (block 2
            // on, so 0x352000) is being written once esptool-js says so.
            const when = `${MAIN_TEXT}.includes('Writing at 0x352000')`;
            // Poll the card (and say what it says) rather than one long
            // wait: the moment is a progress reading, and a run that misses
            // it should show which readings it saw.
            const deadline = Date.now() + MIGRATION_MS;
            let seen = "";
            while (!(await driver.evaluate(when))) {
              const line = (await driver.evaluate(MAIN_TEXT)).match(/(Writing firmware|Moving files|Verifying files|Reading the board's [a-z]+|Resetting the board|Flashing firmware)[^\n]*/)?.[0] ?? "";
              if (line !== seen) {
                seen = line;
                console.log(`    card: ${line}`);
              }
              if (Date.now() > deadline) throw new Error("the moment to pull the cable never came");
              await new Promise((r) => setTimeout(r, 250));
            }
            await shot("before-pull");
            step("cable pulled", (await pullCable(driver)) === "pulled", (await driver.evaluate(MAIN_TEXT)).match(/(Writing firmware|Moving files)[^\n]*/)?.[0] ?? "");
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
            await driver.waitFor(`${MAIN_TEXT}.includes('Finish update') || ${MAIN_TEXT}.includes('Restore files') || ${MAIN_TEXT}.includes('Update firmware')`, { timeoutMs: STEP_MS, what: "the board back after the pull" });
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
      case "W7b":
        step("files moved, byte for byte", files.same, JSON.stringify(files));
        step("the chip is on the new layout", /0x350000/.test(after.layout) || !/pre-2026-10/.test(after.layout), after.layout);
        step("old superblock retired", !legacySuperblock, "");
        step(
          "the board mounted them, as itself",
          said.includes("Flash filesystem mounted") && !said.includes("holding: not formatting")
            && verdict.console.fs.at(-1) === "mounted" && verdict.console.deviceUid.includes(UID),
          JSON.stringify(verdict.console),
        );
        break;
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
