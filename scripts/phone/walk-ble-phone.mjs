#!/usr/bin/env node
// THE BLUETOOTH WALK ON A REAL PHONE AND A REAL BOARD (`just walk-ble-phone`).
//
// The emulated walk (`just walk-ble-emu`) proves Studio's transport, UI and
// Play over `?ble=emu`, which rides the emulated board's USB link: no
// radio. This is the same story with nothing shimmed: Studio in the desk's
// Android phone's own Chrome (`scripts/phone/phone.mjs`), its own
// `navigator.bluetooth`, the real chooser answered over CDP, against a desk
// C6 leased through `board`:
//
//     add over Bluetooth (the real chooser) → identify (the card is the
//       leased board, by MAC; Bluetooth locked on over itself)
//       → Connect on the card, turn its first control → Done
//       → the card's picture, moving, at the Bluetooth pace
//       → the editor (the control reads held: the board's word)
//       → Play → turn a knob
//       → the board's power cut (`board power-cycle`) under Play: the page
//         stays on Play behind Reconnecting and the same knob works after
//
// Its numbers are a Pixel's Chrome against a C6 at desk distance: a phone
// number, not Bluefy's and not a Mac's. The reconnect time after the power
// cut is wall-clock on silicon, reported and never gated.
//
// The board needs a project loaded and firmware the Studio it opens speaks
// to. `--studio` picks the Studio: lightplayer.app by default (the board on
// the latest release: `lp-cli firmware install --release latest`), or a
// local one (`http://127.0.0.1:<port>`, a running `just studio-dev`, put on
// the phone's own localhost by `adb reverse`; the board on this branch's
// firmware). A dev server's debug wasm is ~100 MB over the phone's Wi-Fi
// debugging link; `just studio-web-story-build` and `--serve-release` serve
// the release bundle instead.
//
// Flags:
//   --board <slug|mark|MAC>   the desk C6 to lease (default fixture-c6)
//   --name <LP-…>             the advertised name to pick (default: the
//                             board's own `LP-<last four MAC hex>`; a board
//                             with a project loaded advertises `LP-<project>`)
//   --studio <url>            the Studio to open (default https://lightplayer.app/)
//   --serve-release           serve this checkout's release bundle and use it
//   --snoop                   pull the phone's Bluetooth packet log after
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
/// How long the card's picture is watched for movement.
const CARD_WINDOW_MS = Number(process.env.BLE_CARD_WINDOW_MS ?? 15_000);

const args = process.argv.slice(2);
function opt(name, dflt) {
  const i = args.indexOf(name);
  return i < 0 ? dflt : args[i + 1];
}
const BOARD = opt("--board", "fixture-c6");
const HOLDER = process.env.BOARD_HOLDER ?? "walk-ble-phone";

const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;
const CARD = `document.querySelector('[data-board-card]')`;
const SLIDER = `document.querySelector('#main [role="slider"]')`;
/// A digest of every canvas under #main, so a changed render reads as a
/// changed digest.
const CANVAS_DIGEST = `(() => {
  let h = 0;
  for (const c of document.querySelectorAll('#main canvas')) {
    let s = '';
    try { s = c.toDataURL(); } catch { s = 'tainted'; }
    for (let i = 0; i < s.length; i += 7) h = (h * 31 + s.charCodeAt(i)) | 0;
  }
  return String(h);
})()`;

function boardCmd(...rest) {
  return execFileSync("board", rest, { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] }).trim();
}

async function main() {
  const out = path.join(ROOT, "target", "walk-ble-phone");
  const shots = path.join(out, "shots");
  mkdirSync(shots, { recursive: true });

  // The board: leased for as long as this process lives.
  const info = JSON.parse(boardCmd("show", BOARD, "--json"));
  const mac = info.mac.toLowerCase();
  const macHex = mac.replace(/:/g, "");
  boardCmd("take", BOARD, "--as", HOLDER, "--for", "the phone Bluetooth walk", "--pid", String(process.pid));
  const name = opt("--name", `LP-${macHex.slice(-4)}`);

  // The Studio, on the phone's own localhost when it is local.
  const bundle = args.includes("--serve-release")
    ? await serveStudioBundle({ root: ROOT, port: walkPort(ROOT, "walk-ble-phone") })
    : null;
  let studio = bundle ? `http://localhost:${bundle.address().port}/` : opt("--studio", "https://lightplayer.app/");
  const local = new URL(studio);
  if (["127.0.0.1", "localhost"].includes(local.hostname)) {
    phone.serve(Number(local.port));
    local.hostname = "localhost"; // a secure context on the phone, too
    studio = local.toString();
  }

  const serial = phone.pickPhone();
  const device = phone.awake(serial);
  const { browserWsUrl, browser } = await phone.cdp(serial);

  console.log("\nTHE BLUETOOTH WALK ON A PHONE");
  console.log(`  phone   ${device.model}, Android ${device.android}, ${browser} (${serial}), battery ${device.battery}`);
  console.log(`  board   ${info.mark} ${info.slug} ${info.mac}, advertising ${JSON.stringify(name)} (leased as ${JSON.stringify(HOLDER)})`);
  console.log(`  studio  ${studio}\n`);

  const driver = await StudioDriver.attach({ browserWsUrl });
  const report = { phone: device, browser, board: { slug: info.slug, mark: info.mark, mac: info.mac, name }, studio, steps: [] };
  const shot = async (stepName) => {
    const file = path.join(shots, `phone-${report.steps.length + 1}-${stepName}.png`);
    try {
      await driver.screenshot(file);
    } catch {
      /* the page may be gone */
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
    const before = await driver.evaluate(`${SLIDER}.getAttribute('aria-valuenow')`);
    await driver.evaluate(`(() => {
      const knob = ${SLIDER};
      knob.focus();
      const key = Number(knob.getAttribute('aria-valuenow')) >= Number(knob.getAttribute('aria-valuemax')) ? 'Home' : 'End';
      knob.dispatchEvent(new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true }));
    })()`);
    await driver.waitFor(`${SLIDER}.getAttribute('aria-valuenow') !== ${JSON.stringify(before)}`, {
      timeoutMs: STEP_DEADLINE_MS,
      what: "the knob to take the turn",
    });
    return `${before} → ${await driver.evaluate(`${SLIDER}.getAttribute('aria-valuenow')`)}`;
  };

  let fatal = null;
  try {
    await driver.navigate(studio, { loadTimeoutMs: STUDIO_LOAD_DEADLINE_MS });
    await driver.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: STUDIO_LOAD_DEADLINE_MS, what: "Studio to finish loading" });
    console.log(`  studio is up — page visibility: ${await driver.tabVisible()}\n`);

    await step("add", `Connect a board › Bluetooth, and pick ${name} in the phone's own chooser`, async () => {
      const picked = await driver.connectBluetooth({ name, timeoutMs: 60_000 });
      report.chooser = picked;
      return `picked ${picked.name}; the chooser offered ${picked.seen.map((d) => d.name).join(", ")}`;
    });

    await step("identify", "the card is the leased board (by MAC), ready, with Bluetooth locked on over itself", async () => {
      const face = await driver.boardRuns({ board: mac, timeoutMs: STEP_DEADLINE_MS });
      if (face === "empty") throw new Error(`${info.slug} has no project loaded; load one (over USB) and walk again`);
      await driver.openBar("connection", { board: mac, timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(
        `(() => { const panel = ${CARD}?.querySelector('[data-bar="connection"] [id^="ux-popover-panel"]');
                  return Boolean(panel) && (panel.innerText || '').includes('turn off by USB')
                    && Boolean(panel.querySelector('button[role="switch"][aria-label="Bluetooth"][aria-checked="true"]:disabled')); })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the connection details' locked Bluetooth switch (this link is Bluetooth)" },
      );
      await driver.closeDetails({ board: mac });
      return `the card is ${info.mac}, running a project, on Bluetooth`;
    });

    let written = null;
    await step("card-connect", "Connect on the card: the board's panel in its bars' place; turn its first control", async () => {
      await driver.pressOffer("connect", { board: mac, timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`location.pathname === '/' && Boolean(${CARD}?.querySelector('[data-board-panel] [data-panel-channel]'))`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the board's panel on its card, the page still at `/`",
      });
      written = await driver.evaluate(`(() => {
        const control = ${CARD}.querySelector('[data-board-panel] [data-panel-channel]');
        const out = { scope: control.getAttribute('data-panel-scope'), channel: control.getAttribute('data-panel-channel') };
        const knob = control.querySelector('[role="slider"]');
        const range = control.querySelector('input[type="range"]');
        const toggle = control.querySelector('button[role="switch"]');
        if (knob) {
          knob.focus();
          out.how = Number(knob.getAttribute('aria-valuenow')) >= Number(knob.getAttribute('aria-valuemax')) ? 'Home' : 'End';
          knob.dispatchEvent(new KeyboardEvent('keydown', { key: out.how, bubbles: true, cancelable: true }));
        } else if (range) {
          const to = Number(range.value) >= Number(range.max) ? range.min : range.max;
          Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(range, to);
          range.dispatchEvent(new Event('input', { bubbles: true }));
          out.how = 'to ' + to;
        } else if (toggle) {
          toggle.click();
          out.how = 'flipped';
        }
        return out;
      })()`);
      if (!written.how) throw new Error(`the card's first control (${written.channel}) has no widget the walk can turn`);
      return `turned ${written.scope} ${written.channel} (${written.how})`;
    });

    await step("card-done", "Done: the session closes and the card shows its facts", async () => {
      await driver.pressOffer("done", { board: mac, timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`Boolean(${CARD}) && !${CARD}.querySelector('[data-board-panel]') && ${CARD}.querySelectorAll('[data-bar]').length === 5`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the card's five bars",
      });
      return "the card's five bars again";
    });

    await step("card", `the card's picture at the Bluetooth pace, moving over ${CARD_WINDOW_MS / 1000} s`, async () => {
      await driver.openCorner({ board: mac, timeoutMs: STEP_DEADLINE_MS });
      const pill = await driver.waitFor(
        `(() => { const dt = [...(${CARD}?.querySelectorAll('[data-board-corner] [id^="ux-popover-panel"] dt') ?? [])]
                    .find((el) => (el.textContent || '').trim().toLowerCase() === 'picture');
                  const line = (dt?.nextElementSibling?.textContent || '').trim();
                  return line.includes('over Bluetooth') ? line : false; })()`,
        { timeoutMs: STEP_DEADLINE_MS, what: "the card's picture fact, live at the Bluetooth pace" },
      );
      await driver.closeDetails({ board: mac });
      const d0 = await driver.evaluate(CANVAS_DIGEST);
      await new Promise((resolve) => setTimeout(resolve, CARD_WINDOW_MS));
      const d1 = await driver.evaluate(CANVAS_DIGEST);
      if (d0 === d1) throw new Error("the card's picture did not move while watched");
      return `picture "${pill}", moving`;
    });

    await step("editor", "open the board in the editor: the control turned on the card reads held (the board's word)", async () => {
      await driver.pressOffer("edit", { board: mac, timeoutMs: STEP_DEADLINE_MS });
      await driver.waitFor(`!${MAIN_TEXT}.includes('Connecting project') && Boolean(${SLIDER})`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the project to open on the board",
      });
      const selector = `[data-panel-scope=${JSON.stringify(written.scope)}][data-panel-channel=${JSON.stringify(written.channel)}]`;
      await driver.waitFor(
        `[...document.querySelectorAll(${JSON.stringify(selector)})].some((el) => el.getAttribute('data-panel-state') === 'engaged')`,
        { timeoutMs: STEP_DEADLINE_MS, what: `${written.channel} to read held in the new session` },
      );
      return `${written.channel} held`;
    });

    await step("play", "switch the editor to Play and turn a knob", async () => {
      await driver.evaluate(`document.querySelector('a[title^="Play mode"]').click()`);
      await driver.waitFor(`location.pathname.endsWith('/play') && Boolean(${SLIDER})`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the Play route and its knobs",
      });
      return `knob ${await turnKnob()}`;
    });

    await step("power-cut", "cut the board's power under Play: the page stays on Play and says Reconnecting", async () => {
      const route = await driver.evaluate("location.pathname + location.search");
      await driver.evaluate(`(() => {
        window.__lpWalkTimeoutShown = null;
        window.__lpWalkObserver?.disconnect();
        window.__lpWalkObserver = new MutationObserver(() => {
          const text = document.body.textContent;
          const at = text.indexOf('did not respond');
          if (at >= 0 && window.__lpWalkTimeoutShown === null) window.__lpWalkTimeoutShown = text.slice(Math.max(0, at - 80), at + 60);
        });
        window.__lpWalkObserver.observe(document.body, { subtree: true, childList: true, characterData: true });
      })()`);
      const cut = Date.now();
      boardCmd("power-cycle", BOARD, "--as", HOLDER, "--off-secs", "2");
      await driver.waitFor(`Boolean(document.querySelector('[data-reconnecting="true"]'))`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the Reconnecting strip",
      });
      const now = await driver.evaluate("location.pathname + location.search");
      if (now !== route) throw new Error(`the route moved: ${route} → ${now}`);
      report.powerCut = { route, cutAt: cut, stripAfterMs: Date.now() - cut };
      return `still at ${now}; strip after ${report.powerCut.stripAfterMs} ms`;
    });

    await step("power-back", "the board boots; the strip goes and the same Play takes a knob turn", async () => {
      await driver.waitFor(`!document.querySelector('[data-reconnecting="true"]')`, {
        timeoutMs: STEP_DEADLINE_MS,
        what: "the strip to go (the board back)",
      });
      const backMs = Date.now() - report.powerCut.cutAt;
      const timeout = await driver.evaluate("window.__lpWalkTimeoutShown");
      if (timeout) throw new Error(`a request's timeout reached the page: …${timeout}…`);
      const now = await driver.evaluate("location.pathname + location.search");
      if (now !== report.powerCut.route) throw new Error(`the route moved: ${report.powerCut.route} → ${now}`);
      const knob = await turnKnob();
      report.powerCut = { ...report.powerCut, backAfterMs: backMs, knob };
      return `back ${(backMs / 1000).toFixed(1)} s after the cut (2 s of it off); knob ${knob}`;
    });
  } catch (error) {
    fatal = error;
  }

  const consoleErrors = driver.consoleLines().filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  const panics = consoleErrors.filter((l) => l.includes("panicked at"));
  report.consoleErrors = consoleErrors;
  try {
    report.phoneScreen = phone.shot(path.join(shots, "phone-screen-end.png"), serial).file;
  } catch {
    /* best effort */
  }
  await driver.close();
  bundle?.close();
  if (args.includes("--snoop")) {
    console.log("\n  pulling the phone's Bluetooth packet log (a bug report; a minute or two)…");
    report.snoop = phone.snoop(path.join(out, "snoop"), serial);
  }
  writeFileSync(path.join(out, "walk-ble-phone.json"), JSON.stringify(report, null, 2));
  writeFileSync(path.join(out, "page-console.log"), driver.consoleLines().join("\n") + "\n");
  try {
    boardCmd("drop", BOARD, "--as", HOLDER);
  } catch {
    /* the --pid lease ends with us anyway */
  }

  console.log("\n=== the phone Bluetooth walk, step by step");
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${s.name.padEnd(12)} ${String(s.seconds).padStart(6)} s  ${path.basename(s.shot)}`);
  if (consoleErrors.length) {
    console.log("\n  page console errors:");
    for (const line of consoleErrors.slice(-8)) console.log(`    ${line.slice(0, 300)}`);
  }
  console.log(`\n  report → ${path.join(out, "walk-ble-phone.json")}`);
  if (report.snoop?.btsnoop?.length) console.log(`  btsnoop → ${report.snoop.btsnoop.join(", ")}`);

  if (fatal) {
    console.error(`\nThe phone Bluetooth walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  if (panics.length) {
    console.error(`\nThe walk's steps passed, but the page panicked ${panics.length} time(s).`);
    process.exit(1);
  }
  console.log(`\n✓ the phone Bluetooth walk finished: a ${device.model}'s Chrome, the real chooser, ${info.slug} over the air — card, editor, Play, and a power cut ridden out.`);
}

await main();
