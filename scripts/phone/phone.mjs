#!/usr/bin/env node
// The desk's Android test phone (a Pixel 8a), driven with no hands.
//
// Everything goes through `adb` (`brew install --cask android-platform-tools`):
//
//   - **Wireless debugging, not the cable.** The phone has one USB-C port,
//     and a Web Serial walk needs it for the board. Pair once
//     (`phone pair <code>`, the six digits under Developer options ›
//     Wireless debugging › Pair device with pairing code); after that
//     `phone connect` finds it by mDNS.
//   - **Chrome's DevTools socket** (`localabstract:chrome_devtools_remote`)
//     forwarded to a Mac port is the same protocol the headless walks
//     speak, so `StudioDriver.attach` drives the phone's Chrome like the
//     desk's — including the REAL Web Bluetooth chooser
//     (`StudioDriver.connectBluetooth`).
//   - **`adb reverse`** puts a Mac server on the phone's own `localhost`,
//     which is a secure context: Web Bluetooth and Web Serial work there
//     with no HTTPS and no tailnet.
//   - **The phone's Bluetooth packet log** (Developer options › Enable
//     Bluetooth HCI snoop log) comes off in a bug report: `phone snoop`.
//
// The phone stays awake and unlocked while it is driven (`phone awake`): a
// hidden or locked page is throttled, and Studio's frame feed only runs on
// a visible one. A test phone has no screen lock; this never types a PIN.
//
// Commands:
//   status                    the phone, Android, Chrome, battery, screen
//   setup                     grant Chrome its Bluetooth permissions (once)
//   pair <code>               pair wireless debugging (the code on the phone)
//   connect                   find the paired phone by mDNS and connect
//   awake                     screen on, stays on, keyguard dismissed
//   cdp                       forward Chrome's DevTools; print its browser ws URL
//   serve <port>              the Mac's 127.0.0.1:<port> on the phone's localhost:<port>
//   open <url>                open a URL in the phone's Chrome, in front
//   shot <file.png>           the phone's whole screen (system UI included)
//   ui                        what is on screen, as text and bounds (uiautomator)
//   usb                       the USB device on the phone's port (its MAC on a C6)
//   snoop <dir>               a bug report, and the Bluetooth packet log out of it
//
// `ANDROID_SERIAL` picks a phone when adb sees several.

import { execFileSync } from "node:child_process";
import { mkdirSync, readdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import process from "node:process";

const CHROME = "com.android.chrome";
const DEVTOOLS_SOCKET = "localabstract:chrome_devtools_remote";

/// Run adb; its stdout as text. `serial` pins the phone (all but the
/// bus-wide commands need one).
export function adb(args, { serial = null, encoding = "utf8", timeoutMs = 60_000 } = {}) {
  const full = serial ? ["-s", serial, ...args] : args;
  return execFileSync("adb", full, { encoding, timeout: timeoutMs, stdio: ["ignore", "pipe", "pipe"], maxBuffer: 64 * 1024 * 1024 });
}

/// The attached phones adb can talk to: `[{ serial, wireless, model }]`.
export function phones() {
  return adb(["devices", "-l"])
    .split("\n")
    .slice(1)
    .map((line) => line.trim().split(/\s+/))
    .filter((cols) => cols[1] === "device")
    .map(([serial, , ...rest]) => ({
      serial,
      wireless: serial.includes(":") || serial.includes("._adb-tls-connect."),
      model: rest.find((col) => col.startsWith("model:"))?.slice(6) ?? null,
    }));
}

/// The one phone to drive: `ANDROID_SERIAL`, else the only one attached,
/// else (none attached) the paired one mDNS finds. Several and no
/// `ANDROID_SERIAL` is an error naming them — never the first one.
export function pickPhone() {
  if (process.env.ANDROID_SERIAL) return process.env.ANDROID_SERIAL;
  let found = phones();
  if (found.length === 0) {
    connect();
    found = phones();
  }
  if (found.length === 1) return found[0].serial;
  if (found.length === 0) {
    throw new Error(
      "no phone: adb sees none and mDNS found no paired one. On the phone: Developer options › Wireless debugging on " +
        "(and paired once: `phone pair <code>`), or plug it in and allow USB debugging.",
    );
  }
  // One phone on both USB and Wi-Fi shows twice; prefer the wireless leg so
  // the USB port stays the board's.
  const models = new Set(found.map((phone) => phone.model));
  const wireless = found.filter((phone) => phone.wireless);
  if (models.size === 1 && wireless.length === 1) return wireless[0].serial;
  throw new Error(`several phones (${found.map((phone) => `${phone.serial} ${phone.model ?? ""}`).join(", ")}); set ANDROID_SERIAL`);
}

/// `adb mdns services` rows of one service type: `[{ name, address }]`.
function mdns(type) {
  return adb(["mdns", "services"])
    .split("\n")
    .map((line) => line.trim().split(/\s+/))
    .filter((cols) => cols[1] === type && cols[2])
    .map(([name, , address]) => ({ name, address }));
}

/// Pair wireless debugging with the code the phone shows.
export function pair(code) {
  const services = mdns("_adb-tls-pairing._tcp");
  if (services.length === 0) {
    throw new Error("no phone is offering pairing: on the phone, Wireless debugging › Pair device with pairing code, and keep that screen open");
  }
  if (services.length > 1) throw new Error(`several phones offer pairing: ${services.map((s) => s.address).join(", ")}`);
  return adb(["pair", services[0].address, String(code)]).trim();
}

/// Connect every paired phone mDNS finds. adb's own auto-connect usually
/// beats this to it; it is here for when it has not.
export function connect() {
  const lines = [];
  for (const { address } of mdns("_adb-tls-connect._tcp")) {
    try {
      lines.push(adb(["connect", address], { timeoutMs: 15_000 }).trim());
    } catch (error) {
      lines.push(`${address}: ${error.message.split("\n")[0]}`);
    }
  }
  return lines;
}

function shell(serial, command, options = {}) {
  return adb(["shell", command], { serial, ...options }).trim();
}

export function status(serial = pickPhone()) {
  const prop = (name) => shell(serial, `getprop ${name}`);
  const chrome = shell(serial, `dumpsys package ${CHROME} | grep -m1 versionName`).replace(/.*versionName=/, "");
  const battery = shell(serial, "dumpsys battery");
  const level = battery.match(/level: (\d+)/)?.[1];
  const powered = /(AC|USB|Wireless) powered: true/.test(battery);
  const screen = shell(serial, "dumpsys power | grep -m1 'mWakefulness='").replace(/.*=/, "");
  const locked = /mDreamingLockscreen=true|isStatusBarKeyguard=true|mShowing=true/.test(
    shell(serial, "dumpsys window | grep -E 'mDreamingLockscreen|isStatusBarKeyguard|mShowing' || true"),
  );
  return {
    serial,
    model: prop("ro.product.model"),
    android: prop("ro.build.version.release"),
    chrome,
    battery: `${level}%${powered ? " (charging)" : ""}`,
    screen: screen.toLowerCase(),
    locked,
  };
}

/// Screen on, kept on while powered, keyguard dismissed. A test phone has no
/// screen lock; with one, this leaves it locked and says so.
export function awake(serial = pickPhone()) {
  shell(serial, "input keyevent KEYCODE_WAKEUP");
  shell(serial, "svc power stayon true");
  shell(serial, "wm dismiss-keyguard");
  const now = status(serial);
  if (now.locked) throw new Error("the phone is still locked: a test phone runs with Screen lock › None");
  return now;
}

/// The runtime permissions Chrome's Web Bluetooth needs on Android 12+
/// ("Nearby devices", and location for older scans). Without them the
/// chooser opens empty and says "Chrome needs permission to scan". A test
/// phone grants them once, by adb, so no walk ever meets that sheet.
const CHROME_PERMISSIONS = ["BLUETOOTH_SCAN", "BLUETOOTH_CONNECT", "ACCESS_FINE_LOCATION", "ACCESS_COARSE_LOCATION"];

/// One-time phone setup an agent can do by itself: Chrome's Bluetooth
/// permissions. (Developer options, the debugging prompt and the screen lock
/// are the person's; see the README.)
export function setup(serial = pickPhone()) {
  for (const permission of CHROME_PERMISSIONS) shell(serial, `pm grant ${CHROME} android.permission.${permission}`);
  return { granted: CHROME_PERMISSIONS.map((p) => `${CHROME} ${p}`) };
}

async function devtoolsVersion(port, attempts) {
  for (let attempt = 0; attempt < attempts; attempt++) {
    try {
      return await (await fetch(`http://127.0.0.1:${port}/json/version`, { signal: AbortSignal.timeout(2_000) })).json();
    } catch {
      // Chrome's socket appears a while after its process does: over 10 s on
      // the Pixel's cold start (2026-10-17).
      await new Promise((resolve) => setTimeout(resolve, 500));
    }
  }
  return null;
}

/// Bring Chrome to the front (starting it if it is not running), forward its
/// DevTools socket to a Mac port, and return `{ port, browserWsUrl,
/// browser }`. A forward left from an earlier Chrome process answers
/// nothing, so one that does not answer is replaced.
///
/// The front matters: a phone draws one app, and Chrome in the background
/// renders nothing — a CDP screenshot then hangs, and Studio's frame feed
/// stops.
export async function cdp(serial = pickPhone()) {
  shell(serial, `am start -n ${CHROME}/com.google.android.apps.chrome.Main`);
  const existing = adb(["forward", "--list"])
    .split("\n")
    .map((line) => line.trim().split(/\s+/))
    .find(([s, , remote]) => s === serial && remote === DEVTOOLS_SOCKET)?.[1];
  let port = existing?.replace("tcp:", "");
  let version = port ? await devtoolsVersion(port, 4) : null;
  if (!version) {
    if (existing) adb(["forward", "--remove", existing], { serial });
    port = adb(["forward", "tcp:0", DEVTOOLS_SOCKET], { serial }).trim();
    version = await devtoolsVersion(port, 60);
  }
  if (!version) throw new Error(`Chrome's DevTools did not answer on 127.0.0.1:${port} (is Chrome past its first-run screens?)`);
  // Android Chrome names its browser socket without a host the Mac can use.
  const browserWsUrl = version.webSocketDebuggerUrl.replace(/^ws:\/\/[^/]+/, `ws://127.0.0.1:${port}`);
  return { port: Number(port), browserWsUrl, browser: version.Browser };
}

/// The Mac's 127.0.0.1:`port` on the phone's localhost:`port`.
export function serve(port, serial = pickPhone()) {
  adb(["reverse", `tcp:${port}`, `tcp:${port}`], { serial });
  return { phone: `http://localhost:${port}/` };
}

export function open(url, serial = pickPhone()) {
  shell(serial, `am start -a android.intent.action.VIEW -d '${url.replace(/'/g, "")}' ${CHROME}`);
  return { url };
}

export function shot(file, serial = pickPhone()) {
  writeFileSync(file, adb(["exec-out", "screencap", "-p"], { serial, encoding: "buffer" }));
  return { file };
}

/// A bug report into `dir`, and the Bluetooth packet log (btsnoop, opens in
/// Wireshark) unpacked beside it. Slow — a minute or two — and the log is
/// only there when the snoop log was enabled BEFORE the Bluetooth session
/// (Developer options; toggling it restarts Bluetooth).
export function snoop(dir, serial = pickPhone()) {
  mkdirSync(dir, { recursive: true });
  const zip = path.join(dir, "bugreport.zip");
  adb(["bugreport", zip], { serial, timeoutMs: 600_000 });
  try {
    execFileSync("unzip", ["-o", "-j", zip, "*btsnoop_hci.log*", "-d", dir], { stdio: "ignore" });
  } catch {
    // unzip exits non-zero when the pattern matched nothing.
  }
  const logs = readdirSync(dir).filter((name) => name.includes("btsnoop_hci.log"));
  return {
    bugreport: zip,
    btsnoop: logs.map((name) => path.join(dir, name)),
    note: logs.length ? null : "no btsnoop in the report: Developer options › Enable Bluetooth HCI snoop log is off",
  };
}

/// What is on the phone's screen as uiautomator sees it, system dialogs and
/// Chrome's own choosers included: `[{ text, desc, id, cls, clickable,
/// bounds: [x1, y1, x2, y2] }]`. The way to answer a native sheet CDP cannot
/// reach (the WebUSB chooser, Android's "Allow Chrome to access …?") by what
/// it says rather than by pixel.
export function ui(serial = pickPhone()) {
  const xml = shell(serial, "uiautomator dump /dev/tty", { timeoutMs: 30_000 });
  const nodes = [];
  for (const match of xml.matchAll(/<node ([^>]*?)\/?>/g)) {
    const attr = (name) => match[1].match(new RegExp(`\\b${name}="([^"]*)"`))?.[1] ?? "";
    const bounds = attr("bounds").match(/\[(\d+),(\d+)\]\[(\d+),(\d+)\]/);
    if (!bounds) continue;
    nodes.push({
      text: unescapeXml(attr("text")),
      desc: unescapeXml(attr("content-desc")),
      id: attr("resource-id"),
      cls: attr("class"),
      clickable: attr("clickable") === "true",
      bounds: bounds.slice(1).map(Number),
    });
  }
  return nodes;
}

/// Tap the middle of the first on-screen node whose text or description
/// matches `pattern` (a RegExp or a substring), waiting up to `timeoutMs` for
/// it to appear. Returns the node; throws naming what WAS on screen.
export async function tapText(pattern, { serial = pickPhone(), timeoutMs = 20_000 } = {}) {
  const matches = (value) => (pattern instanceof RegExp ? pattern.test(value) : value.includes(pattern));
  const deadline = Date.now() + timeoutMs;
  let seen = [];
  for (;;) {
    seen = ui(serial);
    const node = seen.find((candidate) => matches(candidate.text) || matches(candidate.desc));
    if (node) {
      const [x1, y1, x2, y2] = node.bounds;
      shell(serial, `input tap ${Math.round((x1 + x2) / 2)} ${Math.round((y1 + y2) / 2)}`);
      return node;
    }
    if (Date.now() > deadline) break;
    await new Promise((resolve) => setTimeout(resolve, 500));
  }
  const words = [...new Set(seen.flatMap((node) => [node.text, node.desc]).filter(Boolean))];
  throw new Error(`nothing on the phone's screen matches ${pattern}; it shows: ${words.slice(0, 30).join(" | ")}`);
}

function unescapeXml(text) {
  return text
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'")
    .replace(/&#(\d+);/g, (_, code) => String.fromCharCode(Number(code)))
    .replace(/&amp;/g, "&");
}

/// The USB device on the phone's port, as Android lists it (`dumpsys usb`):
/// `{ vendorId, productId, product, serialNumber }`, or null. On a C6 the
/// serial number is its MAC, which names the desk board.
export function usbDevice(serial = pickPhone()) {
  const text = shell(serial, "dumpsys usb");
  const field = (name) => text.match(new RegExp(`\\b${name}=([^\\n]+)`))?.[1]?.trim() ?? null;
  const vendor = field("vendor_id");
  if (!vendor || vendor === "-1") return null;
  return {
    vendorId: Number(vendor),
    productId: Number(field("product_id")),
    product: field("product_name"),
    serialNumber: field("serial_number"),
  };
}

async function main() {
  const [command, ...rest] = process.argv.slice(2);
  const run = {
    status: () => status(),
    setup: () => setup(),
    pair: () => pair(rest[0] ?? usage()),
    connect: () => connect(),
    awake: () => awake(),
    cdp: () => cdp(),
    serve: () => serve(Number(rest[0] ?? usage())),
    open: () => open(rest[0] ?? usage()),
    shot: () => shot(rest[0] ?? "phone.png"),
    ui: () => ui().filter((node) => node.text || node.desc),
    usb: () => usbDevice(),
    snoop: () => snoop(rest[0] ?? path.join("target", "phone-snoop")),
  }[command];
  if (!run) usage();
  console.log(JSON.stringify(await run(), null, 1));
}

function usage() {
  console.error("usage: phone status | setup | pair <code> | connect | awake | cdp | serve <port> | open <url> | shot <file.png> | ui | usb | snoop <dir>");
  process.exit(2);
}

if (import.meta.url === `file://${process.argv[1]}`) {
  main().catch((error) => {
    console.error(`phone: ${error.message}`);
    process.exit(1);
  });
}
