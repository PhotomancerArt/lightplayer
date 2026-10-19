// Every bootloader session that ends by booting the board ends with the
// board's own reset: a flash, an erase, a boot-control write, a flash plan
// and a probe each leave the stub with EN pulsed (RTS asserted, DTR not),
// before the port is let go.
//
// Until 2026-10-19 they ended with esptool-js 0.6.0's `after("hard_reset")`,
// which only releases RTS. The connect's reset sequence has already left both
// lines low, so on a C6 it changed no line: the board stayed in the stub, and
// a flashed board answered only when the Flash activity's reconnect ladder
// reset it, a rung (8 s) later.
//
// `node --test lp-app/lpa-link/tests/js/` — `just lpa-link-js-test`. The
// flows run whole, through `browser_serial.js`'s session map, over a fake
// `navigator.serial` port and the fake esptool in `fake_esptool.js`.
import assert from "node:assert/strict";
import { registerHooks } from "node:module";
import { test } from "node:test";
import { fileURLToPath, pathToFileURL } from "node:url";

import {
  eraseDeviceFlash,
  executePlan,
  flashFirmware,
  probeTarget,
  writeBootControl,
} from "../../src/providers/browser_serial_esp32/browser_esp32_flash.js";
import { getGrantedPorts } from "../../src/providers/browser_serial_esp32/browser_serial.js";
import { board } from "./fake_esptool.js";
import { patterned } from "./fake_esp_stub.js";

// What a served Studio puts at `/lpa-link/`: the page's controller module.
const PUBLIC_LPA_LINK = fileURLToPath(new URL("../../../lpa-studio-web/public/lpa-link/", import.meta.url));
registerHooks({
  resolve(specifier, context, next) {
    if (specifier.startsWith("/lpa-link/")) {
      return {
        url: pathToFileURL(PUBLIC_LPA_LINK + specifier.slice("/lpa-link/".length)).href,
        shortCircuit: true,
      };
    }
    return next(specifier, context);
  },
});

const ESPTOOL = new URL("./fake_esptool.js", import.meta.url).href;
const KNOWN_CHIP_IDS = ["esp32c6", "esp32s3", "esp32"];
const MANIFEST = "http://localhost/firmware/esp32c6/manifest.json";
const IMAGE = patterned(4096);

/// The pulse `rebootFromFlash` writes, whole-status both times.
const REBOOT = [
  { signals: { dataTerminalReady: false, requestToSend: true } },
  { signals: { dataTerminalReady: false, requestToSend: false } },
];

test("a flash reboots the board into what it wrote", async () => {
  const { port, id } = await boardOnThePage();
  await flashFirmware(id, MANIFEST, ESPTOOL, KNOWN_CHIP_IDS, null);
  assertRebootedThenReleased(port);
  assert.deepEqual(board.flash.slice(0x10000, 0x10000 + IMAGE.length), IMAGE);
});

test("a flash plan reboots the board when its last step is done", async () => {
  const { port, id } = await boardOnThePage();
  await executePlan(id, MANIFEST, ESPTOOL, KNOWN_CHIP_IDS, [{ kind: "firmware" }], () => null, () => -1, null);
  assertRebootedThenReleased(port);
});

test("a boot-control write reboots the board, so the record is read at once", async () => {
  const { port, id } = await boardOnThePage();
  const record = Uint8Array.of(0x4c, 0x50, 0x42, 0x43, 1, 0, 0, 0);
  await writeBootControl(id, ESPTOOL, 0x9000, record, null);
  assertRebootedThenReleased(port);
  assert.deepEqual(board.flash.slice(0x9000, 0x9000 + record.length), record);
});

test("a probe puts the board back to running its firmware", async () => {
  const { port, id } = await boardOnThePage();
  const { chipName } = await probeTarget(id, ESPTOOL);
  assert.match(chipName, /ESP32-C6/);
  assertRebootedThenReleased(port);
});

test("an erase reboots the board onto its blank flash", async () => {
  const { port, id } = await boardOnThePage();
  await eraseDeviceFlash(id, ESPTOOL, null);
  assertRebootedThenReleased(port);
});

test("a flash that fails does not reboot the board", async () => {
  const { port, id } = await boardOnThePage();
  board.chipName = "ESP32-S3 (QFN56) (revision v0.2)";
  try {
    await assert.rejects(flashFirmware(id, MANIFEST, ESPTOOL, KNOWN_CHIP_IDS, null), /Refusing to flash/);
  } finally {
    board.chipName = "ESP32-C6 (revision 0)";
  }
  assert.deepEqual(signalsOf(port), [], "a refused flash pulsed EN");
  assert.deepEqual(port.events.at(-1), { disconnect: true });
});

// --- helpers ---------------------------------------------------------------

/// The reset is the session's last word to the board: the pulse, then the
/// port let go; and not one esptool-js `after(...)` call.
function assertRebootedThenReleased(port) {
  assert.deepEqual(port.events.slice(-3), [...REBOOT, { disconnect: true }]);
  assert.deepEqual(signalsOf(port), REBOOT, "a line moved outside the reboot pulse");
  const loader = board.loaders.at(-1);
  assert.deepEqual(loader.afterCalls, [], "the session still asked esptool-js to reset");
}

function signalsOf(port) {
  return port.events.filter((event) => event.signals);
}

/// One granted C6 on a fresh page: a `navigator.serial` that holds it, the
/// firmware package served, and the session id Rust would have.
async function boardOnThePage() {
  const port = fakeSerialPort();
  Object.defineProperty(globalThis, "navigator", {
    value: { serial: { getPorts: async () => [port] }, userAgent: "node" },
    configurable: true,
  });
  globalThis.fetch = servePackage();
  board.flash = new Uint8Array(0x20000).fill(0xff);
  const [{ id }] = await getGrantedPorts();
  return { port, id };
}

/// The `SerialPort` members a session and esptool-js's `Transport` reach;
/// `events` is every line change and the transport's disconnect, in order.
/// The fake esptool connects without moving a line, so every line change
/// the flows make is theirs.
function fakeSerialPort() {
  return {
    events: [],
    readable: null,
    writable: null,
    getInfo: () => ({ usbVendorId: 0x303a, usbProductId: 0x1001 }),
    async open() {},
    async close() {},
    async setSignals(signals) {
      this.events.push({ signals: { ...signals } });
    },
  };
}

function servePackage() {
  const manifest = {
    schemaVersion: 2,
    firmwareId: "lightplayer-esp32c6",
    core: { platform: { chip: "esp32c6" } },
    flash: { baudRate: 115200 },
    images: [{ path: "app.bin", address: "0x10000", sizeBytes: IMAGE.length }],
  };
  return async (url) => {
    const href = String(url);
    if (href === MANIFEST) {
      return new Response(JSON.stringify(manifest), { headers: { "content-type": "application/json" } });
    }
    if (href === new URL("app.bin", MANIFEST).href) {
      return new Response(IMAGE, { headers: { "content-type": "application/octet-stream" } });
    }
    return new Response("not found", { status: 404, statusText: "Not Found" });
  };
}
