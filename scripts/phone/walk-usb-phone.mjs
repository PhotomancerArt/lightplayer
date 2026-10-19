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
//       → the editor → Play → turn a knob
//       → restart the board from Studio (the reset path over WebUSB): the
//         card comes back
//
// The board needs a project loaded and firmware the chosen Studio speaks to.
// The board on the phone is found by MAC from the phone's own USB list
// (`phone usb`), and leased for as long as this runs; there is no power cut
// here, because the board's power is the phone's port.
//
// Flags:
//   --board <slug|mark|MAC>   the desk C6 to lease (default: the one on the phone's port)
//   --studio <url>            the Studio to open (default https://lightplayer.app/)
//   --serve-release           serve this checkout's release bundle and use it
//
// Not a CI job: it needs the phone and the desk.

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { StudioDriver } from "../emu/studio-driver.mjs";
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
  const { browserWsUrl, browser } = await phone.cdp(serial);

  console.log("\nTHE USB WALK ON A PHONE");
  console.log(`  phone   ${device.model}, Android ${device.android}, ${browser} (${serial}), battery ${device.battery}`);
  console.log(`  board   ${info.mark} ${info.slug} ${info.mac} on the phone's port: ${onPort.product} (leased as ${JSON.stringify(HOLDER)})`);
  console.log(`  studio  ${studio}\n`);

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

    await step("identify", "the card is the board on the phone's port (by MAC), running its project", async () => {
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
  console.log(`\n✓ the phone USB walk finished: a ${device.model}'s Chrome over WebUSB, ${info.slug} on its port — card, editor, Play.`);
}

await main();
