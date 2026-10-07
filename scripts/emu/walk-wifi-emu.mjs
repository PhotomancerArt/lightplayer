#!/usr/bin/env node
// THE WI‑FI SETTINGS WALK WITH NO BOARD (Wi‑Fi roadmap M5;
// `just walk-wifi-emu <usb|ble>`).
//
// `lan` is a different walk in its own script, `walk-wifi-emu-lan.mjs`
// (Wi‑Fi plan P13: two emulated boards on one virtual LAN, Studio over
// `?lan=`); this file hands `lan` and its arguments straight to it.
//
// Real Studio, headless, against an emulated ESP32-C6 running the shipped
// firmware image — over the `?emu=` USB shim (`usb`) or the `?ble=emu`
// Bluetooth polyfill (`ble`):
//
//     connect → open the card's Wi‑Fi row (nothing saved: it opens on
//       "Add a network by name") → type a made-up network and password →
//       Save → back on the list, the new row says "Saved · this firmware
//       can't connect…" → Done → "+ Connect to a network" → a second
//       network → Save
//       → the board's own flash holds the network file
//       → reload the page, connect again → both networks read back from the
//         board (Studio never stores a password)
//       → Cloud relay off (on by default) → open the first network's page →
//         Forget (two clicks) → only the second is left
//
// ⚠️ WHAT THIS PROVES: the transport, the UI and the board's store. NOT
// access: the emulated firmware sees its trusted link on both lanes (the
// Bluetooth polyfill's header says why), so every request is answered at
// the edit tier. Tiers are proven by `lpa-server`'s host tests. And nothing
// joins a network: no radio, no station (M6).
//
// Every claim keys off the BOARD (its status answer read back after a
// reload, the bytes the door writes back to the chip file), never a Studio
// string something else could satisfy. Studio's words say when to click.
//
// NOT CI. Made-up values only (lp-walk-net / correct-horse-42). Needs: the
// release Studio bundle (`just studio-web-story-build`), the packaged C6
// firmware (`just studio-firmware-package-esp32c6`), a debug `lp-cli`,
// Chrome. It serves the bundle itself on this worktree's stable slot — no
// dev server, nothing to adopt — and runs as one foreground command.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

import { StudioDriver } from "./studio-driver.mjs";
import { serveStudioBundle, startDoor, stopDoor, studioUrlFor, walkPort } from "./emulated-lane.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const BOARD = "c6-a";
const SSID = "lp-walk-net";
const PASSWORD = "correct-horse-42";
const SECOND_SSID = "lp-back-office";
const SECOND_PASSWORD = "staple-battery-7";
const WIFI = "Wi‑Fi";
const STUDIO_LOAD_MS = 420_000;
const STEP_MS = 180_000;

const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;
const PANEL = `document.querySelector('[id^="ux-popover-panel"]')`;
const PANEL_TEXT = `(${PANEL}?.innerText || '')`;
/// The card's Wi‑Fi row: the button whose text starts "Wi‑Fi".
const WIFI_ROW = `[...document.querySelectorAll('button')].find((b) => (b.innerText || '').trim().startsWith(${JSON.stringify(WIFI)}))`;

function args() {
  const lane = process.argv[2];
  if (lane !== "usb" && lane !== "ble") {
    console.error("usage: node scripts/emu/walk-wifi-emu.mjs <usb|ble|lan> (lan: [--out <dir>] [--keep-open] [--dry-run])");
    process.exit(2);
  }
  return { lane, out: path.join(ROOT, "target/walk-wifi-emu", lane) };
}

/// Type `text` into the input `selector` names, the way a keyboard does for
/// Dioxus (`input` events carry the value).
function typeInto(selector, text) {
  return `(() => {
    const el = ${PANEL}?.querySelector(${JSON.stringify(selector)});
    if (!el) return false;
    el.focus();
    el.value = ${JSON.stringify(text)};
    el.dispatchEvent(new Event('input', { bubbles: true }));
    return true;
  })()`;
}

/// Type a network's name and password on the connect page.
async function typeNetworkWith(driver, ssid, password) {
  if (!(await driver.evaluate(typeInto('input[placeholder="Network name"]', ssid)))) {
    throw new Error("no network name field");
  }
  if (!(await driver.evaluate(typeInto('input[type="password"]', password)))) {
    throw new Error("no password field (type=password)");
  }
}

/// Press Save once it is pressable (drawn disabled while a read is in
/// flight; opening the popover asks the board again).
async function saveWith(driver) {
  await driver.waitFor(
    `[...${PANEL}.querySelectorAll('button')].some((b) => !b.disabled && (b.innerText || '').trim() === 'Save')`,
    { timeoutMs: STEP_MS, what: "Save to be pressable" },
  );
  await driver.click("Save", { scope: PANEL });
}

async function main() {
  const { lane, out } = args();
  const shots = path.join(out, "shots");
  mkdirSync(shots, { recursive: true });
  const stateDir = path.join(out, "state");

  const port = walkPort(ROOT, "walk-wifi-emu");
  const server = await serveStudioBundle({ root: ROOT, port });
  const door = await startDoor({
    root: ROOT,
    id: `walk-wifi-${lane}`,
    boards: [`${BOARD}={fw}`],
    stateDir,
    consoleDir: path.join(out, "console"),
    logFile: path.join(out, "serve.log"),
    fresh: true,
  });
  let url = studioUrlFor({ studioPort: port, doorAddr: door.addr, sinkUrl: "http://127.0.0.1:9/none" }).replace(
    /&record=[^&]*/,
    "",
  );
  if (lane === "ble") url += "&ble=emu";

  console.log(`\nTHE WI‑FI WALK WITH NO BOARD (${lane})`);
  console.log(`  emulated board   ${BOARD} (packaged fw-esp32c6), door http://${door.addr}/boards`);
  console.log(`  the page         ${url}\n`);

  const driver = await StudioDriver.launch({ width: 1100, height: 900 });
  // Yona reads screenshots at 2× (memory: screenshots-for-yona-2x-zoom).
  await driver.cdp.send(
    "Emulation.setDeviceMetricsOverride",
    { width: 1100, height: 900, deviceScaleFactor: 2, mobile: false },
    driver.sessionId,
  );
  const report = { lane, board: BOARD, url, steps: [] };
  const step = async (name, describe, body) => {
    console.log(`— ${name}: ${describe}`);
    let error = null;
    let note = null;
    try {
      note = await body();
    } catch (failure) {
      error = failure;
    }
    const file = path.join(shots, `wifi-${lane}-${report.steps.length + 1}-${name}.png`);
    try {
      await driver.screenshot(file);
    } catch {
      /* the page may be gone */
    }
    console.log(`  ${error ? "✗ " + error.message.split("\n")[0] : "✓"}${note ? `  ${note}` : ""}`);
    report.steps.push({ name, describe, ok: !error, error: error?.message ?? null, note, shot: file });
    if (error) throw error;
  };
  const load = async () => {
    await driver.navigate(url);
    await driver.awaitShim();
    await driver.waitFor(`${MAIN_TEXT}.length > 0`, { timeoutMs: STUDIO_LOAD_MS, what: "Studio to finish loading" });
    if (lane === "ble") {
      await driver.waitFor("Boolean(window.__lpEmuBluetooth)", { what: "the Bluetooth polyfill" });
    }
  };
  const connect = async () => {
    // After a reload a granted board may come back on its own; else add it.
    const back = await driver
      .waitFor(`Boolean(${WIFI_ROW})`, { timeoutMs: 20_000, what: "the board to come back on its own" })
      .then(() => true)
      .catch(() => false);
    if (back) return "came back on its own";
    await driver.clickWhenReady(lane === "usb" ? "via USB" : "via Bluetooth", { timeoutMs: STEP_MS });
    await driver.pickBoard(BOARD, { timeoutMs: STEP_MS });
    await driver.waitFor(`${MAIN_TEXT}.includes('Ready') && Boolean(${WIFI_ROW})`, {
      timeoutMs: STEP_MS,
      what: "the card, Ready, with its Wi‑Fi row",
    });
    return "added and identified";
  };
  const typeNetwork = (ssid, password) => typeNetworkWith(driver, ssid, password);
  const save = () => saveWith(driver);
  const openWifi = async () => {
    await driver.waitFor(`Boolean(${WIFI_ROW})`, { timeoutMs: STEP_MS, what: "the Wi‑Fi row" });
    await driver.evaluate(`${WIFI_ROW}.click()`);
    // Every root page ends on the Cloud relay switch.
    await driver.waitFor(`${PANEL_TEXT}.includes('Cloud relay')`, {
      timeoutMs: STEP_MS,
      what: "the Wi‑Fi panel",
    });
    // Settled, not mid-fade, so the step's screenshot reads.
    await driver.waitFor(`getComputedStyle(${PANEL}).opacity === '1'`, {
      timeoutMs: 10_000,
      what: "the panel to finish fading in",
    }).catch(() => null);
  };
  /// The page never shows the password once pressed: not in text, not in a
  /// field, not in a title.
  const noPasswordOnThePage = () =>
    driver.evaluate(`(() => {
      const pw = ${JSON.stringify(PASSWORD)};
      if ((document.body.innerText || '').includes(pw)) return 'in the page text';
      for (const el of document.querySelectorAll('input, textarea')) if ((el.value || '').includes(pw)) return 'in a field';
      for (const el of document.querySelectorAll('[title]')) if (el.title.includes(pw)) return 'in a title';
      return null;
    })()`);

  let fatal = null;
  try {
    await load();

    await step("connect", `connect the emulated board over ${lane === "usb" ? "the USB shim" : "?ble=emu"}`, connect);

    await step("open", "open the Wi‑Fi row: nothing saved, so it opens on adding a network by name", async () => {
      await openWifi();
      await driver.waitFor(`${PANEL_TEXT}.includes('Not set up.') && ${PANEL_TEXT}.includes("can't list networks")`, {
        timeoutMs: STEP_MS,
        what: "the connect page (Not set up., type the name)",
      });
      return "Not set up. — Add a network by name";
    });

    await step("typed", "type the network and its password (drawn as dots)", async () => {
      await typeNetwork(SSID, PASSWORD);
      return "the password field is a password input";
    });

    await step("saved", "Save: back on the list, the new row says it is saved and this firmware can't connect yet", async () => {
      await save();
      await driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(SSID)}) && ${PANEL_TEXT}.includes("this firmware can't connect to Wi‑Fi yet. It will after an update.")`, {
        timeoutMs: STEP_MS,
        what: "the new row's test (Saved · this firmware can't connect…)",
      });
      const leak = await noPasswordOnThePage();
      if (leak) throw new Error(`the password is on the page: ${leak}`);
      await driver.click("Done", { scope: PANEL });
      return `saved; the password is nowhere on the page`;
    });

    await step("second", "+ Connect to a network → a second network → Save: both are listed", async () => {
      await driver.click("Connect to a network", { scope: PANEL });
      await driver.waitFor(`${PANEL_TEXT}.includes('Add a network by name')`, {
        timeoutMs: STEP_MS,
        what: "the connect page",
      });
      await typeNetwork(SECOND_SSID, SECOND_PASSWORD);
      await save();
      await driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(SSID)}) && ${PANEL_TEXT}.includes(${JSON.stringify(SECOND_SSID)})`, {
        timeoutMs: STEP_MS,
        what: "both networks on the list",
      });
      await driver.click("Done", { scope: PANEL });
      return `${SSID}, ${SECOND_SSID}`;
    });

    await step("chip", "the board's own flash holds the network file (the door writes the chip back)", async () => {
      const chip = path.join(stateDir, `${BOARD}.flash.bin`);
      const deadline = Date.now() + 30_000;
      for (;;) {
        if (
          existsSync(chip) &&
          readFileSync(chip).includes(Buffer.from(`"ssid":"${SSID}"`)) &&
          readFileSync(chip).includes(Buffer.from(`"ssid":"${SECOND_SSID}"`))
        ) {
          return `${path.basename(chip)} holds both networks`;
        }
        if (Date.now() > deadline) throw new Error(`${chip} never held both networks`);
        await new Promise((resolve) => setTimeout(resolve, 1_000));
      }
    });

    await step("reloaded", "reload the page and connect again: both networks read back from the board", async () => {
      await load();
      const how = await connect();
      await driver.waitFor(`(${WIFI_ROW}?.innerText || '').includes('2 saved')`, {
        timeoutMs: STEP_MS,
        what: "the Wi‑Fi row to say 2 saved",
      });
      await openWifi();
      await driver.waitFor(`${PANEL_TEXT}.includes("this firmware can't connect to Wi‑Fi yet") && ${PANEL_TEXT}.includes(${JSON.stringify(SECOND_SSID)})`, {
        timeoutMs: STEP_MS,
        what: "the list after the reload",
      });
      return `${how}; the row says 2 saved; both listed`;
    });

    await step("relay-off", "turn the cloud relay off (it is on by default): the board's answer says so", async () => {
      const relay = `${PANEL}.querySelector('button[role="switch"][aria-label="Cloud relay"]')`;
      const before = await driver.evaluate(`${relay}?.getAttribute('aria-checked')`);
      if (before !== "true") throw new Error(`the Cloud relay switch starts ${before}, not on`);
      // Drawn locked while a read is in flight (opening the panel asks the
      // board again); a click on a locked switch does nothing.
      await driver.waitFor(`${relay} && !${relay}.disabled`, {
        timeoutMs: STEP_MS,
        what: "the Cloud relay switch to be pressable",
      });
      await driver.evaluate(`${relay}.click()`);
      await driver.waitFor(`${relay}?.getAttribute('aria-checked') === 'false' && !${relay}.disabled`, {
        timeoutMs: STEP_MS,
        what: "the board's status (relay off)",
      });
      return "Cloud relay on → off";
    });

    await step("forget", "the first network's page → Forget (Lasting: arm, then confirm): only the second is left", async () => {
      await driver.click(SSID, { scope: PANEL });
      await driver.waitFor(`${PANEL_TEXT}.includes("Password: saved on the board. It can't be shown.")`, {
        timeoutMs: STEP_MS,
        what: "the network's page",
      });
      await driver.click("Forget", { scope: PANEL });
      // Armed (red, the 4 s window): the second click inside it acts.
      await driver.waitFor(`Boolean(${PANEL}.querySelector('.ux-armed'))`, {
        timeoutMs: 3_000,
        what: "Forget to arm",
      });
      await driver.click("Forget", { scope: PANEL });
      await driver.waitFor(`!${PANEL_TEXT}.includes(${JSON.stringify(SSID)}) && ${PANEL_TEXT}.includes(${JSON.stringify(SECOND_SSID)})`, {
        timeoutMs: STEP_MS,
        what: "the list without the first network",
      });
      const relayOff = await driver.evaluate(
        `${PANEL}.querySelector('button[role="switch"][aria-label="Cloud relay"]')?.getAttribute('aria-checked') === 'false'`,
      );
      if (!relayOff) throw new Error("the cloud relay came back on");
      return `${SECOND_SSID} left; the relay stays off`;
    });
  } catch (error) {
    fatal = error;
  }

  const consoleErrors = driver.consoleLines().filter((l) => l.startsWith("[error]") || l.startsWith("[exception]"));
  const leaked = driver.consoleLines().filter((l) => l.includes(PASSWORD) || l.includes(SECOND_PASSWORD));
  report.consoleErrors = consoleErrors;
  report.passwordInConsole = leaked.length;
  writeFileSync(path.join(out, "walk-wifi-emu.json"), JSON.stringify(report, null, 2));

  console.log(`\n=== the Wi‑Fi walk (${lane}), step by step`);
  for (const s of report.steps) console.log(`  ${s.ok ? "✓" : "✗"} ${s.name.padEnd(9)} ${path.basename(s.shot)}`);
  if (consoleErrors.length) {
    console.log("\n  page console errors:");
    for (const line of consoleErrors.slice(-8)) console.log(`    ${line.slice(0, 300)}`);
  }
  console.log(`  the password in the page console: ${leaked.length} line(s)`);
  console.log(`\n  report → ${path.join(out, "walk-wifi-emu.json")}`);

  await driver.close();
  await stopDoor(door);
  server.close();

  if (fatal) {
    console.error(`\nThe Wi‑Fi walk did not finish: ${fatal.message}`);
    process.exit(1);
  }
  if (leaked.length) {
    console.error(`\nThe walk's steps passed, but the page console printed the password ${leaked.length} time(s).`);
    process.exit(1);
  }
  console.log(`\n✓ the Wi‑Fi walk (${lane}) finished: two networks added → saved on the chip → read back after a reload → cloud relay off → one forgotten, with no board.`);
}

// `lan` is its own walk; it reads `process.argv` itself (and skips the lane).
if (process.argv[2] === "lan") await import("./walk-wifi-emu-lan.mjs");
else await main();
