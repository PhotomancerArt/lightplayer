#!/usr/bin/env node
// THE WI‑FI SETTINGS WALK WITH NO BOARD (Wi‑Fi roadmap M5;
// `just walk-wifi-emu <usb|ble>`).
//
// `lan` is a different walk in its own script, `walk-wifi-emu-lan.mjs`
// (Wi‑Fi plan P13: two emulated boards on one virtual LAN, Studio over
// `?lan=`); this file hands `lan` and its arguments straight to it. `relay`
// (Wi‑Fi relay plan P9, `walk-wifi-emu-relay.mjs`) likewise: lp-cli-driven,
// one board reaching an in-process relay through the LAN's uplink.
//
// Real Studio, headless, against an emulated ESP32-C6 running the shipped
// firmware image — over the `?emu=` USB shim (`usb`) or the `?ble=emu`
// Bluetooth polyfill (`ble`). The board is on a virtual LAN of its own
// (`lan=home`, a fixture this walk writes: two made-up access points), so
// its station scans and joins for real (the network seam, `net=lan`):
//
//     connect → open the card's Wi‑Fi row (nothing saved: it opens on
//       "Pick the board's network", listing what the board's scan heard) →
//       pick lp-walk-net, type its password → Connect → the board joins:
//       the new row says "Connected · <its lease>" → Done → "+ Connect to a
//       network" → "Other network…" → a name no access point has → Connect
//       → "Not in range", and the board stays on lp-walk-net
//       → the board's own flash holds the network file
//       → reload the page, connect again → both networks read back from the
//         board, still joined (Studio never stores a password)
//       → Cloud relay off (on by default) → open the second network's page
//         → Forget (two clicks) → only lp-walk-net is left, still joined
//
// ⚠️ WHAT THIS PROVES: the transport, the UI, the board's store and its
// station on the emulated LAN. NOT access: the emulated firmware sees its
// trusted link on both lanes (the Bluetooth polyfill's header says why), so
// every request is answered at the edit tier. Tiers are proven by
// `lpa-server`'s host tests. And NOT the radio: the network seam stands in
// for it (no signal, airtime or coexistence; ADR
// `2026-10-05-emulator-seams.md` §11).
//
// Every claim keys off the BOARD, never a Studio string something else could
// satisfy: its status answer over its own LAN address (`lp-cli wifi status`
// through the door's forward, a path the page never touches), the networks
// its scan heard (Studio holds no list of networks), and the bytes the door
// writes back to the chip file. Studio's words say when to click and when to
// look.
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
import { boardRegistry, serveStudioBundle, startDoor, stopDoor, studioUrlFor, walkPort } from "./emulated-lane.mjs";
// The `lan` lane's readers of the board's own status (imported for its
// helpers, that file runs nothing).
import { awaitStatus, forwardOf, lpEmuCommit, stationOf } from "./walk-wifi-emu-lan.mjs";

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), "../..");
const BOARD = "c6-a";
const LAN = "home";
/// TEST VALUES ONLY: committed, printed, and named in the emulator's output.
/// An access point on the board's LAN: picked from the scan and joined.
const SSID = "lp-walk-net";
const PASSWORD = "correct-horse-42";
/// Another access point, heard and never saved: the scan lists more than
/// the network the walk picks.
const NEIGHBOUR_SSID = "lp-walk-guest";
/// No access point has it: typed by name, saved, never joined.
const SECOND_SSID = "lp-back-office";
const SECOND_PASSWORD = "staple-battery-7";
const FIXTURE = `# walk-wifi-emu.mjs (usb, ble): made-up test values only, never a real network.
[[access_point]]
name = "${SSID}"
password = "${PASSWORD}"
signal_dbm = -50

[[access_point]]
name = "${NEIGHBOUR_SSID}"
password = "paper-lantern-3"
signal_dbm = -68
`;
const WIFI = "Wi‑Fi";
const STUDIO_LOAD_MS = 420_000;
const STEP_MS = 180_000;
/// Joins, scans and the board's status reads run at emulated speed on a
/// shared box: a wedged-run deadline, never a measurement.
const JOIN_MS = 420_000;

/// Studio's words (`lpa-studio-core/src/app/network/wifi_words.rs`). They
/// say when to look; the board's status says what happened.
const WORDS = {
  pickANetwork: "Pick the board's network",
  /// The connect page on a firmware that cannot scan (no station).
  cannotList: "This firmware can't list networks.",
  otherNetwork: "Other network",
  connectToANetwork: "Connect to a network",
  notInRange: "Not in range",
  connected: (ip) => `Connected · ${ip}`,
  passwordSaved: "Password: saved on the board. It can't be shown.",
};

const MAIN_TEXT = `(document.querySelector('#main')?.innerText || '')`;
const PANEL = `document.querySelector('[id^="ux-popover-panel"]')`;
const PANEL_TEXT = `(${PANEL}?.innerText || '')`;
/// The card's Wi‑Fi row: the button whose text starts "Wi‑Fi".
const WIFI_ROW = `[...document.querySelectorAll('button')].find((b) => (b.innerText || '').trim().startsWith(${JSON.stringify(WIFI)}))`;

function args() {
  const lane = process.argv[2];
  if (lane !== "usb" && lane !== "ble") {
    console.error("usage: node scripts/emu/walk-wifi-emu.mjs <usb|ble|lan|relay|studio-lan|studio-lan-reset|studio-relay> (lan, studio-lan, studio-lan-reset, studio-relay: [--out <dir>] [--keep-open] [--dry-run])");
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

/// Press the panel's button labelled exactly `label` once it is pressable
/// (Connect, drawn disabled while a read is in flight; opening the popover
/// asks the board again).
async function pressWith(driver, label) {
  await driver.waitFor(
    `[...(${PANEL}?.querySelectorAll('button') ?? [])].some((b) => !b.disabled && (b.innerText || '').trim() === ${JSON.stringify(label)})`,
    { timeoutMs: STEP_MS, what: `${label} to be pressable` },
  );
  await driver.click(label, { scope: PANEL, exact: true });
}

/// A saved network's `last` attempt, as the board reports it.
function lastAttempt(status, ssid) {
  return (status?.networks ?? []).find((network) => network.ssid === ssid)?.last ?? null;
}

/// The saved networks' names, in the board's order.
function savedOf(status) {
  return (status?.networks ?? []).map((network) => network.ssid);
}

/// Joined to `ssid`, by the board's own status.
function joinedTo(status, ssid) {
  const station = stationOf(status);
  return station.kind === "connected" && station.ssid === ssid;
}

async function main() {
  const { lane, out } = args();
  const shots = path.join(out, "shots");
  mkdirSync(shots, { recursive: true });
  const stateDir = path.join(out, "state");

  const fixture = path.join(out, "lan.toml");
  writeFileSync(fixture, FIXTURE);

  const port = walkPort(ROOT, "walk-wifi-emu");
  const server = await serveStudioBundle({ root: ROOT, port });
  const door = await startDoor({
    root: ROOT,
    id: `walk-wifi-${lane}`,
    boards: [`${BOARD}={fw},lan=${LAN}`],
    stateDir,
    consoleDir: path.join(out, "console"),
    logFile: path.join(out, "serve.log"),
    fresh: true,
    extraArgs: ["--lan", `${LAN}=${fixture}`],
  });
  let url = studioUrlFor({ studioPort: port, doorAddr: door.addr, sinkUrl: "http://127.0.0.1:9/none" }).replace(
    /&record=[^&]*/,
    "",
  );
  if (lane === "ble") url += "&ble=emu";
  // The board's LAN address through the door: where the walk asks the
  // board itself. The page never dials it (it holds the USB door).
  const entry = (await boardRegistry(door.addr)).find((b) => b.id === BOARD);
  const lan = `lan:${forwardOf(entry)}`;
  const configuration = entry?.configuration ?? "lp-emu:esp32c6";
  const boardSays = (test, what) => awaitStatus(lan, test, { timeoutMs: JOIN_MS, what });

  console.log(`\nTHE WI‑FI WALK WITH NO BOARD (${lane})`);
  console.log(`  emulated board   ${BOARD} (packaged fw-esp32c6, ${configuration}, lp-emu ${lpEmuCommit()}), door http://${door.addr}/boards`);
  console.log(`  its LAN          ${LAN} (${SSID}, ${NEIGHBOUR_SSID}); the board's status over ${lan}`);
  console.log(`  the page         ${url}\n`);

  const driver = await StudioDriver.launch({ width: 1100, height: 900 });
  // Yona reads screenshots at 2× (memory: screenshots-for-yona-2x-zoom).
  await driver.cdp.send(
    "Emulation.setDeviceMetricsOverride",
    { width: 1100, height: 900, deviceScaleFactor: 2, mobile: false },
    driver.sessionId,
  );
  const report = { lane, board: BOARD, configuration, lpEmu: lpEmuCommit(), lan, url, steps: [] };
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
    // The shim's banner covers the Wi‑Fi panel's lower rows in a shot.
    await driver.click("hide", { exact: true }).catch(() => null);
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
  const press = (label) => pressWith(driver, label);
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
  /// The page never shows a password once pressed: not in text, not in a
  /// field, not in a title.
  const noPasswordOnThePage = () =>
    driver.evaluate(`(() => {
      for (const pw of ${JSON.stringify([PASSWORD, SECOND_PASSWORD])}) {
        if ((document.body.innerText || '').includes(pw)) return 'in the page text';
        for (const el of document.querySelectorAll('input, textarea')) if ((el.value || '').includes(pw)) return 'in a field';
        for (const el of document.querySelectorAll('[title]')) if (el.title.includes(pw)) return 'in a title';
      }
      return null;
    })()`);

  let fatal = null;
  try {
    await load();

    await step("connect", `connect the emulated board over ${lane === "usb" ? "the USB shim" : "?ble=emu"}`, connect);

    await step("open", "open the Wi‑Fi row: nothing saved, so it opens on the connect page, listing what the board heard", async () => {
      await openWifi();
      await driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.pickANetwork)}) || ${PANEL_TEXT}.includes(${JSON.stringify(WORDS.cannotList)})`, {
        timeoutMs: STEP_MS,
        what: "the connect page (Pick the board's network)",
      });
      // This lane needs a station. Said by name, so a firmware without one
      // is not read as a slow page (defect
      // 2026-10-09-the-wifi-settings-walk-still-expected-firmware-without-wifi).
      if (await driver.evaluate(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.cannotList)})`)) {
        throw new Error(`the board's firmware cannot connect to Wi‑Fi (Studio: "${WORDS.cannotList}"); this lane needs a firmware with a station and the network seam`);
      }
      // The board heard them: its scan answer is what lists them (Studio
      // holds no list of networks). The `lan` lane once found the list
      // empty for good — Studio's scan dropped while the panel's own status
      // read was in flight — so a person's "refresh" is pressed once if the
      // list stays empty, and the report says so.
      const listed = `${PANEL_TEXT}.includes(${JSON.stringify(SSID)}) && ${PANEL_TEXT}.includes(${JSON.stringify(NEIGHBOUR_SSID)})`;
      let refreshed = false;
      await driver.waitFor(listed, { timeoutMs: 30_000, what: "the board's scan" }).catch(async () => {
        refreshed = true;
        await driver.click("refresh", { scope: PANEL });
        await driver.waitFor(listed, { timeoutMs: STEP_MS, what: `${SSID} and ${NEIGHBOUR_SSID} in the board's scan after refresh` });
      });
      return `the board heard ${SSID} and ${NEIGHBOUR_SSID}${refreshed ? " (after one refresh)" : ""}`;
    });

    await step("typed", `pick ${SSID} and type its password (drawn as dots)`, async () => {
      await driver.click(SSID, { scope: PANEL });
      await driver.waitFor(`Boolean(${PANEL}?.querySelector('input[type="password"]'))`, {
        timeoutMs: STEP_MS,
        what: `${SSID}'s page (a password input)`,
      });
      if (!(await driver.evaluate(typeInto('input[type="password"]', PASSWORD)))) {
        throw new Error("no password field (type=password)");
      }
      return "the password field is a password input";
    });

    await step("joined", `Connect: the board joins ${SSID} on its LAN, and the new row says so with the board's own lease`, async () => {
      await press("Connect");
      await driver.waitFor(`${PANEL_TEXT}.includes('Connected · ')`, {
        timeoutMs: JOIN_MS,
        what: "the new row's test (Connected · …)",
      });
      // The board's words, over its own LAN address: joined, and with the
      // lease the row shows.
      const status = await boardSays((s) => joinedTo(s, SSID), `connected to ${SSID}`);
      const { ip } = stationOf(status);
      await driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.connected(ip))})`, {
        timeoutMs: STEP_MS,
        what: `the row to say ${WORDS.connected(ip)} (the board's lease)`,
      });
      const leak = await noPasswordOnThePage();
      if (leak) throw new Error(`the password is on the page: ${leak}`);
      await driver.click("Done", { scope: PANEL });
      return `the board says connected to ${SSID} at ${ip}; the password is nowhere on the page`;
    });

    await step("second", "+ Connect to a network → Other network… → a name no access point has → Connect: saved, Not in range, still joined", async () => {
      await driver.click(WORDS.connectToANetwork, { scope: PANEL });
      await driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.otherNetwork)})`, {
        timeoutMs: STEP_MS,
        what: "the connect page",
      });
      await driver.click(WORDS.otherNetwork, { scope: PANEL });
      await typeNetwork(SECOND_SSID, SECOND_PASSWORD);
      await press("Connect");
      await driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(SECOND_SSID)}) && ${PANEL_TEXT}.includes(${JSON.stringify(WORDS.notInRange)})`, {
        timeoutMs: JOIN_MS,
        what: `${SECOND_SSID}'s test (Not in range)`,
      });
      // The board's words: it looked for the second network, did not find
      // it, and went back to the first.
      const status = await boardSays(
        (s) => lastAttempt(s, SECOND_SSID) === "notFound" && joinedTo(s, SSID),
        `${SECOND_SSID} last=notFound and connected to ${SSID}`,
      );
      await driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(SSID)}) && ${PANEL_TEXT}.includes(${JSON.stringify(SECOND_SSID)})`, {
        timeoutMs: STEP_MS,
        what: "both networks on the list",
      });
      await driver.click("Done", { scope: PANEL });
      return `the board saved ${savedOf(status).join(", ")}; ${SECOND_SSID} last=notFound; still on ${SSID}`;
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

    await step("reloaded", "reload the page and connect again: the board is still joined, and both networks read back from it", async () => {
      await load();
      const how = await connect();
      // The board's words first: still (or again, if opening its port reset
      // it) on the first network, both saved.
      const status = await boardSays(
        (s) => joinedTo(s, SSID) && savedOf(s).includes(SSID) && savedOf(s).includes(SECOND_SSID),
        `connected to ${SSID} with ${SSID} and ${SECOND_SSID} saved`,
      );
      const { ip } = stationOf(status);
      // The card's row names the network the board is on.
      await driver.waitFor(`(${WIFI_ROW}?.innerText || '').includes(${JSON.stringify(SSID)})`, {
        timeoutMs: STEP_MS,
        what: `the Wi‑Fi row to say ${SSID}`,
      });
      await openWifi();
      await driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.connected(ip))}) && ${PANEL_TEXT}.includes(${JSON.stringify(SECOND_SSID)})`, {
        timeoutMs: STEP_MS,
        what: "the list after the reload",
      });
      return `${how}; the board says connected to ${SSID} at ${ip}, ${savedOf(status).length} saved; both listed`;
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
        what: "the switch to draw the board's answer (relay off)",
      });
      await boardSays((s) => s.cloudRelay === false, "Cloud relay off");
      return "Cloud relay on → off (the board says off)";
    });

    await step("forget", `${SECOND_SSID}'s page → Forget (Lasting: arm, then confirm): only ${SSID} is left, still joined`, async () => {
      await driver.click(SECOND_SSID, { scope: PANEL });
      await driver.waitFor(`${PANEL_TEXT}.includes(${JSON.stringify(WORDS.passwordSaved)})`, {
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
      await driver.waitFor(`!${PANEL_TEXT}.includes(${JSON.stringify(SECOND_SSID)}) && ${PANEL_TEXT}.includes(${JSON.stringify(SSID)})`, {
        timeoutMs: STEP_MS,
        what: `the list without ${SECOND_SSID}`,
      });
      // The board's words: one network left, still on it, the relay off.
      const status = await boardSays(
        (s) => savedOf(s).join(",") === SSID && joinedTo(s, SSID) && s.cloudRelay === false,
        `only ${SSID} saved, connected to it, Cloud relay off`,
      );
      const relayOff = await driver.evaluate(
        `${PANEL}.querySelector('button[role="switch"][aria-label="Cloud relay"]')?.getAttribute('aria-checked') === 'false'`,
      );
      if (!relayOff) throw new Error("the cloud relay switch came back on");
      return `the board says ${savedOf(status).join(", ")} saved, connected to ${stationOf(status).ssid}, relay off`;
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
  console.log(
    `\n✓ the Wi‑Fi walk (${lane}) finished: a heard network joined → a second saved by name, not in range → both on the chip → read back after a reload → cloud relay off → one forgotten, with no board (${configuration}, lp-emu ${report.lpEmu}).`,
  );
}

// `lan` and `relay` are their own walks; each reads `process.argv` itself
// (and skips the lane).
if (process.argv[2] === "lan") await import("./walk-wifi-emu-lan.mjs");
else if (process.argv[2] === "relay") await import("./walk-wifi-emu-relay.mjs");
// `studio-lan` (network-transport plan P04): Studio with no flag reaching
// boards on the virtual LAN — its own walk too.
else if (process.argv[2] === "studio-lan") await import("./walk-wifi-emu-studio-lan.mjs");
// `studio-lan-reset`: the card's Reset on a board reached over Wi‑Fi.
else if (process.argv[2] === "studio-lan-reset") await import("./walk-wifi-emu-studio-lan-reset.mjs");
// `studio-relay` (network-transport plan, PR C): Studio with no flag
// reaching a board through a local relay, by "Connect through lightplayer.app".
else if (process.argv[2] === "studio-relay") await import("./walk-wifi-emu-studio-relay.mjs");
else await main();
