#!/usr/bin/env node
// THE USB WALK ON A REAL PHONE AND A REAL BOARD (`just walk-usb-phone`).
//
// Chrome on Android lists no wired board in Web Serial, so Studio's USB
// square speaks to one over WebUSB there (`usb_cdc_serial.js`,
// `docs/adr/2026-10-18-android-usb-over-webusb.md`). This walks that path
// with nothing shimmed: Studio in the desk phone's own Chrome
// (`scripts/phone/phone.mjs`), a C6 plugged into the PHONE's USB-C port (not
// the desk hub), Chrome's own WebUSB chooser and Android's own "Allow Chrome
// to access …?" dialog, both answered with `adb` taps by what they say:
//
//     add over USB (the real chooser; Android's dialog the first time)
//       → identify (the card is the board on the phone's port, by MAC)
//       → reset: Factory reset from the card (esptool's erase over WebUSB)
//       → install: the blank board's Install firmware, this Studio's build
//         written by the flasher over WebUSB (`readFlashSafely` and
//         `writeFlashChecked` survive what the C6 drops on a phone)
//       → booted: the board says hello on the build Studio wrote
//       → upload: a project from the gallery onto the empty board
//       → the editor → Play → turn a knob
//
// The board needs firmware the chosen Studio speaks to; the walk erases it
// and installs that Studio's build, so what was on it is gone (a test
// board). The board on the phone is found by MAC from the phone's own USB
// list (`phone usb`), and leased for as long as this runs; there is no power
// cut here, because the board's power is the phone's port.
//
// Flags:
//   --board <slug|mark|MAC>   the desk C6 to lease (default: the one on the phone's port)
//   --studio <url>            the Studio to open (default https://lightplayer.app/)
//   --serve-release           serve this checkout's release bundle and use it
//   --no-install              skip reset and install (the board keeps its firmware and project)
//
// Not a CI job: it needs the phone and the desk.

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { PANEL, StudioDriver } from "../emu/studio-driver.mjs";
import { serveStudioBundle, walkPort } from "../emu/emulated-lane.mjs";
import * as phone from "./phone.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const STEP_DEADLINE_MS = 180_000;
const STUDIO_LOAD_DEADLINE_MS = 420_000;

const args = process.argv.slice(2);
function opt(name, dflt) {
  const i = args.indexOf(name);
  return i < 0 ? dflt : args[i + 1];
}
const HOLDER = process.env.BOARD_HOLDER ?? "walk-usb-phone";
const INSTALL = !args.includes("--no-install");
/// The model the blank board's Install firmware picks (the desk's C6s are
/// XIAO boards), and the project put back on it after.
const BOARD_MODEL = process.env.WALK_BOARD_MODEL ?? "XIAO ESP32-C6";
const WALK_PROJECT = process.env.WALK_PROJECT ?? "Peach (1D)";
const FLASH_DEADLINE_MS = 900_000;

/// Page-side: a Firmware row carries running work, or the layout question
/// (`continue-update`) is open in the firmware details.
const ROW_WORKING = `Boolean(document.querySelector('[data-board-card] [data-bar="firmware"][data-bar-work="running"]'))`;
const ASKING = `Boolean([...document.querySelectorAll('[data-board-card] [data-bar="firmware"] [data-offer-path$="/continue-update"]')].find((m) => m.closest('.ux-popover-layer')))`;
/// Page-side: a bar's line as it reads, its pieces joined by one space.
const LINE = `((el) => [...el.children].map((c) => (c.textContent || '').replace(/\\s+/g, ' ').trim()).filter(Boolean).join(' '))`;
/// Page-side: the card offering a blank board's Install firmware (`flash`,
/// pressable), as its `devices/<ref>` (a blank board has said no hello, so
/// `new-<n>`), or false. Not its words: the card says "Firmware none", or
/// "This board doesn't respond" while the sibling fix for a board that
/// never spoke is open; both offer Install.
const BLANK_CARD = `(() => { for (const card of document.querySelectorAll('[data-board-card]')) {
    const mark = card.querySelector('[data-offer-path$="/flash"]');
    const buttons = mark ? mark.querySelectorAll('button') : [];
    if (buttons.length && !buttons[buttons.length - 1].disabled) return card.getAttribute('data-board-card'); }
  return false; })()`;
/// Page-side: the card at `devices/mac-<hex>` for `mac`, or null.
const CARD_OF = (mac) => `document.querySelector('[data-board-card="devices/mac-${mac.replace(/:/g, "")}"]')`;
/// Page-side: the board's own card offers Edit or Add a project, pressable —
/// it said hello on this link (a remembered card offline offers neither).
const SPOKE = (mac) => `[...(${CARD_OF(mac)}?.querySelectorAll('[data-offer-path$="/edit"] button, [data-offer-path$="/push"] button') ?? [])].some((b) => !b.disabled)`;
const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;
const CARD = `document.querySelector('[data-board-card]')`;
const SLIDER = `document.querySelector('#main [role="slider"], #main input[type="range"]')`;
const SLIDER_VALUE = `(() => { const c = ${SLIDER}; return c.getAttribute('aria-valuenow') ?? c.value; })()`;

function boardCmd(...rest) {
  return execFileSync("board", rest, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

async function main() {
  const out = path.join(ROOT, "target", "walk-usb-phone");
  const shots = path.join(out, "shots");
  mkdirSync(shots, { recursive: true });

  const serial = phone.pickPhone();
  const onPort = phone.usbDevice(serial);
  if (!onPort) throw new Error("nothing is plugged into the phone's USB port (`phone usb` lists no device)");
  const BOARD = opt("--board", onPort.serialNumber);
  const info = JSON.parse(boardCmd("show", BOARD, "--json"));
  if (info.mac.toUpperCase() !== String(onPort.serialNumber).toUpperCase()) {
    throw new Error(`${info.slug} (${info.mac}) is not the board on the phone's port (${onPort.serialNumber})`);
  }
  const mac = info.mac.toLowerCase();
  boardCmd("take", info.slug, "--as", HOLDER, "--for", "the phone USB walk", "--pid", String(process.pid));

  const bundle = args.includes("--serve-release")
    ? await serveStudioBundle({ root: ROOT, port: walkPort(ROOT, "walk-usb-phone") })
    : null;
  let studio = bundle ? `http://localhost:${bundle.address().port}/` : opt("--studio", "https://lightplayer.app/");
  const local = new URL(studio);
  if (["127.0.0.1", "localhost"].includes(local.hostname)) {
    phone.serve(Number(local.port), serial);
    local.hostname = "localhost"; // a secure context on the phone, too
    studio = local.toString();
  }

  const device = phone.awake(serial);
  const { browserWsUrl, browser, port: cdpPort } = await phone.cdp(serial);
  // A Studio tab an earlier walk left open still holds the board.
  const stale = await phone.closePages(cdpPort, new URL(studio).origin);

  console.log("\nTHE USB WALK ON A PHONE");
  console.log(`  phone   ${device.model}, Android ${device.android}, ${browser} (${serial}), battery ${device.battery}`);
  console.log(`  board   ${info.mark} ${info.slug} ${info.mac} on the phone's port: ${onPort.product} (leased as ${JSON.stringify(HOLDER)})`);
  console.log(`  studio  ${studio}${stale.length ? ` (closed ${stale.length} Studio tab(s) left open)` : ""}\n`);

  const driver = await StudioDriver.attach({ browserWsUrl });
  const report = { phone: device, browser, board: { slug: info.slug, mark: info.mark, mac: info.mac }, usb: onPort, studio, steps: [] };
  const shot = async (stepName) => {
    const file = path.join(shots, `phone-${report.steps.length + 1}-${stepName}.png`);
    try {
      phone.shot(file, serial);
    } catch {
      /* best effort */
    }
    return file;
  };
  const step = async (stepName, describe, body) => {
    console.log(`— ${stepName}: ${describe}`);
    const t0 = Date.now();
    let error = null;
    let note = null;
    try {
      note = await body();
    } catch (failure) {
      error = failure;
    }
    const seconds = Number(((Date.now() - t0) / 1000).toFixed(1));
    const file = await shot(stepName);
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"} (${seconds} s)${note ? `  ${note}` : ""}`);
    report.steps.push({ name: stepName, describe, ok: !error, seconds, error: error?.message ?? null, note, shot: file });
    if (error) throw error;
  };
  /// Press a firmware verb on the card's face, twice when it is a Lasting
  /// one (it wears `ux-armed-chip` and arms on its first press).
  const pressMaybeArmed = async (verb) => {
    await driver.pressOffer(verb, { board: mac, timeoutMs: STEP_DEADLINE_MS });
    const scope = await driver.card({ board: mac });
    // The mark is `display: contents`; its control is its last button.
    const armed = `(() => { const mark = [...(${scope}?.querySelectorAll('[data-offer-path$="/${verb}"]') ?? [])].find((m) => !m.closest('.ux-popover-layer'));
                           const buttons = mark ? mark.querySelectorAll('button') : [];
                           return Boolean(buttons.length && buttons[buttons.length - 1].classList.contains('ux-armed')); })()`;
    try {
      await driver.waitFor(armed, { timeoutMs: 3_000, what: `${verb} to arm` });
    } catch {
      return "one press";
    }
    await driver.pressOffer(verb, { board: mac, timeoutMs: STEP_DEADLINE_MS });
    return "armed, confirmed";
  };
  /// What the flasher said it did twice (`writeFlashChecked`, `openLoader`).
  const retries = (from = 0) => {
    const said_ = driver.consoleLines().slice(from);
    const lines = said_.filter((line) => /failed \(attempt \d+ of \d+\)/.test(line));
    const reads = said_.filter((line) => /lost a segment/.test(line)).length;
    const said = [reads ? `${reads} read segment(s) read again` : null, ...lines.map((l) => l.slice(0, 140))].filter(Boolean);
    return said.length ? `; retried: ${said.join(" | ")}` : "; nothing done twice";
  };
  const turnKnob = async () => {
    const before = await driver.evaluate(SLIDER_VALUE);
    await driver.evaluate(`(() => {
      const control = ${SLIDER};
      if (control.matches('input[type="range"]')) {
        const lo = Number(control.min), hi = Number(control.max);
        const to = Number(control.value) >= hi ? String((lo + hi) / 2) : control.max;
        Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(control, to);
        control.dispatchEvent(new Event('input', { bubbles: true }));
        return;
      }
      control.focus();
      const key = Number(control.getAttribute('aria-valuenow')) >= Number(control.getAttribute('aria-valuemax')) ? 'Home' : 'End';
      control.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
    })()`);
    await driver.waitFor(`${SLIDER_VALUE} !== ${JSON.stringify(before)}`, { timeoutMs: STEP_DEADLINE_MS, what: "the control to take the turn" });
    return `${before} → ${await driver.evaluate(SLIDER_VALUE)}`;
  };

  let fatal = null;
  /// The blank board's card after the factory reset (`devices/new-<n>`):
  /// it has said no hello, so it has no MAC yet.
  let blank = null;
  try {
    await driver.navigate(studio, { loadTimeoutMs: STUDIO_LOAD_DEADLINE_MS });
    await driver.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: STUDIO_LOAD_DEADLINE_MS, what: "Studio to finish loading" });
    report.backend = await driver.evaluate(`(async () => (await import('/lpa-link/usb_cdc_serial.js').then(() => 'loadable', () => 'missing')))()`, { awaitPromise: true });

    await step("add", "Connect a board › USB, pick the board in Chrome's own chooser, allow Android's access", async () => {
      await driver.pressConnectWithGesture("USB", { timeoutMs: STEP_DEADLINE_MS });
      // Chrome's WebUSB chooser lists the device by its product string.
      const row = await phone.tapText(/USB JTAG|serial debug|ESP32|Espressif/i, { serial, timeoutMs: 30_000 });
      const connect = await phone.tapText(/^Connect$/i, { serial, timeoutMs: 10_000 });
      // Android asks once per device ("Allow Chrome to access …?").
      let allowed = "not asked";
      try {
        await phone.tapText(/^OK$/i, { serial, timeoutMs: 6_000 });
        allowed = "allowed";
      } catch {
        /* already allowed for this device */
      }
      return `picked "${row.text || row.desc}", ${connect.text || connect.desc}; Android's access ${allowed}`;
    });

    // A board with no firmware has no hello and so no MAC card: its first
    // install is the walk's firmware steps, from `install` on.
    blank = await driver
      .waitFor(`${BLANK_CARD} || (${SPOKE(mac)} && 'known')`, { timeoutMs: 60_000, what: "the board's card" })
      .then((found) => (found === "known" ? null : found))
      .catch(() => null);
    if (blank) console.log(`  (the board has no firmware: ${blank}; its first install from the phone)`);

    if (!blank) await step("identify", "the card is the board on the phone's port (by MAC), running its project", async () => {
      try {
        await driver.waitCard({ board: mac, timeoutMs: 60_000 });
      } catch (error) {
        const said = driver.consoleLines().filter((line) => /usb|serial|open|claim/i.test(line) && !line.startsWith("[debug]"));
        throw new Error(`${error.message.split("\n")[0]}; the page said: ${said.slice(-4).join(" | ") || "nothing about USB"}`);
      }
      const face = await driver.boardRuns({ board: mac, timeoutMs: STEP_DEADLINE_MS });
      if (face === "empty") throw new Error(`${info.slug} has no project loaded; load one and walk again`);
      const bar = await driver.barText("connection", { board: mac });
      if (!/USB/.test(bar)) throw new Error(`the card's connection bar says ${JSON.stringify(bar)}, not USB`);
      return `the card is ${info.mac}, running a project; connection "${bar}"`;
    });

    if (INSTALL && !blank) {
      report.before = await driver.detailsFact("firmware", "Build", { board: mac, timeoutMs: STEP_DEADLINE_MS });
      await driver.closeDetails({ board: mac });

      // A board on another build is offered this Studio's over USB
      // (`update-firmware`; Lasting, so it arms): the flasher inspects the
      // board, backs its files up, writes the firmware, and moves the files
      // when the layout changes (asking first). A board already on it is not.
      if (await driver.offered("update-firmware", { board: mac, enabled: true })) {
        await step("update", "Update firmware over USB: the flasher reads the board, writes this Studio's build and keeps its files", async () => {
          const t0 = Date.now();
          const from = driver.consoleLines().length;
          const armed = await pressMaybeArmed("update-firmware");
          let answered = false;
          await driver.waitFor(`${ROW_WORKING} || ${ASKING}`, { timeoutMs: STEP_DEADLINE_MS, what: "the update to start" });
          const deadline = Date.now() + FLASH_DEADLINE_MS;
          for (;;) {
            if (!answered && (await driver.evaluate(ASKING))) {
              // The layout question: the details ARE the question, one press.
              await driver.pressOffer("continue-update", { board: mac, bar: "firmware", timeoutMs: 15_000 });
              answered = true;
            }
            if (!(await driver.evaluate(`${ROW_WORKING} || ${ASKING}`))) break;
            if (Date.now() > deadline) throw new Error(`the update did not finish in ${FLASH_DEADLINE_MS / 1000} s`);
            await new Promise((r) => setTimeout(r, 500));
          }
          report.updateSeconds = Number(((Date.now() - t0) / 1000).toFixed(1));
          await driver.waitCard({ board: mac, timeoutMs: STEP_DEADLINE_MS });
          const face = await driver.boardRuns({ board: mac, timeoutMs: STEP_DEADLINE_MS });
          const build = await driver.detailsFact("firmware", "Build", { board: mac, timeoutMs: STEP_DEADLINE_MS });
          await driver.closeDetails({ board: mac });
          if (build === report.before) throw new Error(`the board still runs ${build} after the update`);
          if (face !== "running") throw new Error(`the board is ${face} after the update: its project was not kept`);
          report.updated = build;
          return `${report.before} → ${build} in ${report.updateSeconds} s (${armed}${answered ? ", files moved" : ""}), project kept${retries(from)}`;
        });
      } else {
        console.log(`  (no USB update offered: the board already runs this Studio's build, ${report.before})`);
      }

      await step("reset", "Factory reset from the card: esptool erases the board over WebUSB", async () => {
        // Lasting: the first press arms, the second erases.
        await driver.pressOffer("erase", { board: mac, bar: "firmware", confirm: true, timeoutMs: STEP_DEADLINE_MS });
        await driver.closeDetails({ board: mac }).catch(() => {});
        await driver.waitFor(`!${ROW_WORKING}`, { timeoutMs: FLASH_DEADLINE_MS, what: "the erase to finish" });
        // The card that erased the board does not see it come back blank
        // (it says "not responding"; a sibling fix, "Offer Install, not
        // Retry, on a board that never spoke"), so the walk reloads, as a
        // person would, and the granted port reconnects with no chooser.
        await driver.navigate(studio, { loadTimeoutMs: STUDIO_LOAD_DEADLINE_MS });
        blank = await driver.waitFor(BLANK_CARD, { timeoutMs: STEP_DEADLINE_MS, what: "a card offering the blank board Install firmware" });
        return `was ${report.before}; after a reload the blank board is ${blank}, offered Install firmware`;
      });

    }

    if (INSTALL || blank) {
      await step("install", `Install firmware on the blank board (${BOARD_MODEL}): the flasher writes this Studio's build over WebUSB`, async () => {
        const t0 = Date.now();
        const from = driver.consoleLines().length;
        // `flashBlank`'s steps, without its "Firmware none" precondition:
        // Install opens the board pick, picking the model presses it, and
        // the Firmware row's work starts and then ends.
        await driver.pressOffer("flash", { board: blank, timeoutMs: STEP_DEADLINE_MS });
        await driver.waitFor(`Boolean(${PANEL})`, { timeoutMs: STEP_DEADLINE_MS, what: "the board-model picker" });
        await driver.click(BOARD_MODEL, { scope: PANEL });
        await driver.waitWork("firmware", "running", { board: blank, timeoutMs: STEP_DEADLINE_MS });
        await driver.waitFor(`!${ROW_WORKING}`, { timeoutMs: FLASH_DEADLINE_MS, what: "the install to finish" });
        report.installSeconds = Number(((Date.now() - t0) / 1000).toFixed(1));
        return `written in ${report.installSeconds} s${retries(from)}`;
      });

      await step("booted", "the board says hello on the build the flasher wrote", async () => {
        await driver.waitCard({ board: mac, timeoutMs: STEP_DEADLINE_MS });
        await driver.waitOffer("push", { board: mac, timeoutMs: STEP_DEADLINE_MS });
        const build = await driver.detailsFact("firmware", "Build", { board: mac, timeoutMs: STEP_DEADLINE_MS });
        await driver.closeDetails({ board: mac });
        if (!build?.startsWith("fw-esp32c6")) throw new Error(`the card's firmware details name no C6 build: ${JSON.stringify(build)}`);
        report.after = build;
        return `Build ${build}${report.before ? ` (was ${report.before})` : " on a board that had none"}`;
      });

      await step("upload", `put ${WALK_PROJECT} on the empty board from the gallery`, async () => {
        await driver.pressOffer("push", { board: mac, timeoutMs: STEP_DEADLINE_MS });
        await driver.waitFor(`Boolean(${PANEL})`, { timeoutMs: STEP_DEADLINE_MS, what: "the project popover" });
        await driver.click(WALK_PROJECT, { scope: PANEL });
        await driver.clickWhenReady("Put it on the board", { scope: PANEL, timeoutMs: STEP_DEADLINE_MS });
        await driver.boardSaid("Project loaded", { board: mac, timeoutMs: STEP_DEADLINE_MS });
        const face = await driver.boardRuns({ board: mac, timeoutMs: STEP_DEADLINE_MS });
        if (face !== "running") throw new Error(`the board says it is ${face} after the push`);
        return `the board said "Project loaded"`;
      });
    }

    await step("editor", "open the board in the editor", async () => {
      await driver.pressOffer("edit", { board: mac, timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`!${MAIN_TEXT}.includes('Connecting project') && Boolean(${SLIDER})`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the project to open on the board",
      });
      return await driver.evaluate("location.pathname");
    });

    await step("play", "switch the editor to Play and turn a knob", async () => {
      await driver.evaluate(`document.querySelector('a[data-play-toggle="enter"]').click()`);
      await driver.waitFor(`location.pathname.endsWith('/play') && Boolean(${SLIDER})`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the Play route and its knobs",
      });
      return `knob ${await turnKnob()}`;
    });
  } catch (error) {
    fatal = error;
  }

  const consoleErrors = driver.consoleLines().filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  const panics = consoleErrors.filter((l) => l.includes("panicked at"));
  report.consoleErrors = consoleErrors;
  await driver.close();
  bundle?.close();
  writeFileSync(path.join(out, "walk-usb-phone.json"), JSON.stringify(report, null, 2));
  writeFileSync(path.join(out, "page-console.log"), driver.consoleLines().join("\n") + "\n");
  try {
    boardCmd("drop", info.slug, "--as", HOLDER);
  } catch {
    /* the --pid lease ends with us anyway */
  }

  console.log("\n=== the phone USB walk, step by step");
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${s.name.padEnd(12)} ${String(s.seconds).padStart(6)} s  ${path.basename(s.shot)}`);
  if (consoleErrors.length) {
    console.log("\n  page console errors:");
    for (const line of consoleErrors.slice(-8)) console.log(`    ${line.slice(0, 300)}`);
  }
  console.log(`\n  report → ${path.join(out, "walk-usb-phone.json")}`);
  if (fatal) {
    console.error(`\nThe phone USB walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  if (panics.length) {
    console.error(`\nThe walk's steps passed, but the page panicked ${panics.length} time(s).`);
    process.exit(1);
  }
  const done = report.steps.map((s) => s.name);
  const installed = [
    done.includes("update") ? `updated to ${report.updated}` : null,
    done.includes("reset") ? "factory reset" : null,
    done.includes("install") ? `${report.after} installed in ${report.installSeconds} s, a project put back` : null,
  ].filter(Boolean).map((x) => `${x}, `).join("");
  console.log(`\n✓ the phone USB walk finished: a ${device.model}'s Chrome over WebUSB, ${info.slug} on its port — card, ${installed}editor, Play.`);
}

await main();
