// The `navigator.serial` polyfill and its `SerialPort` double.
//
// Installed, `navigator.serial` in this page is a virtual USB bus whose ports
// are emulated C6 boards (`emulator_port.js`). Studio's device layer —
// `browser_serial.js`, `browser_esp32_device_controller.js`,
// `browser_esp32_flash.js` and the Rust provider above them — runs against it
// UNCHANGED. That claim is the whole point, and it is what the real-Chrome
// conformance suite in `lp-app/lpa-link/tests/browser_serial_conformance.rs`
// pins.
//
// ELEVEN CALLS. That is the entire contract the Studio layer has with Web
// Serial: `navigator.serial.{requestPort, getPorts, addEventListener}` and
// `port.{open, close, readable, writable, setSignals, getSignals, getInfo,
// forget}`. Nothing here implements more of the spec than that, because
// anything more would be untested surface pretending to be a browser.
//
// TWO BEHAVIOURS THE REAL API HAS, AND SO DOES THIS ONE. Both have a defect
// behind them, and a polyfill that "improved" on either would make the
// emulator LESS faithful than a board:
//
//  1. **Re-enumeration mints a NEW `SerialPort` object.** A USB-Serial-JTAG
//     chip resetting looks exactly like a replug, and it is exactly what the
//     emulator does on `reset` / `download-mode`. So after one, `getPorts()`
//     returns a new object for that board and the old one stops being
//     enumerated — while still answering `getInfo()`, because that is how
//     `adoptReenumeratedPorts` pairs the dead generation to its replacement
//     (G1 2026-08-31: a replugged C6 wallpapered the gallery).
//  2. **A closed port is still a granted port.** `close()` releases the
//     streams and nothing else; only `forget()` revokes the grant and takes
//     the port out of `getPorts()` (2026-07-22: deleting the grant handle on
//     close broke flashing).
//
// `getInfo()` reports **303a:1001** — Espressif native USB, honestly
// indistinguishable (PD7). An emulated C6 *is* native USB, so the existing
// vid:pid paths (grant-aware picking, a board's `usb_bridge`, re-enumeration
// pairing, `labelForPort`'s strong "ESP32 Serial" label) run unchanged.
//
// THE SHIM TRANSLATES NOTHING. `setSignals` writes DTR/RTS onto the control
// channel and stops there. The reset dances are decoded by the emulator from
// the RTS falling edge and whether DTR was ever high; this file never
// pattern-matches a dance and never sends a verb of its own.

import { nativeBacking } from "./emulator_port.js";

const VENDOR_ID = 0x303a;
const PRODUCT_ID = 0x1001;

let installed = null;
let previous = null;
let hadOwnProperty = false;

/// Replace `navigator.serial` with a bus over `baseUrl`'s emulated boards.
///
/// `boards` — an array of board ids to admit, when the page wants a subset.
/// `backing` — an `{ describe, listBoards, connect }` triple; the default is
/// `emu serve` over `baseUrl`, and the conformance suite passes a scripted
/// double instead so CI needs no server, no sockets and no firmware.
export async function install(baseUrl, { boards = null, backing = null } = {}) {
  if (installed) {
    await uninstall();
  }
  const source = backing ?? nativeBacking(baseUrl);
  const bus = new VirtualSerial(source);
  await bus.load(boards);

  const navigator = globalThis.navigator;
  // DD9: `navigator.serial` is a getter on `Navigator.prototype` in Chromium,
  // so an own property on the instance shadows it, and `configurable: true`
  // is what lets `uninstall()` take it back off again. The conformance suite
  // asserts both directions in real Chrome — M3's install seam rests on it.
  const own = Object.getOwnPropertyDescriptor(navigator, "serial");
  hadOwnProperty = own !== undefined;
  previous = own;
  Object.defineProperty(navigator, "serial", {
    value: bus,
    configurable: true,
    enumerable: true,
    writable: false,
  });
  installed = bus;
  return bus;
}

/// Put `navigator.serial` back the way it was found and drop every board.
export async function uninstall() {
  const bus = installed;
  installed = null;
  if (bus) {
    await bus.dispose();
  }
  const navigator = globalThis.navigator;
  if (hadOwnProperty && previous) {
    Object.defineProperty(navigator, "serial", previous);
  } else {
    delete navigator.serial;
  }
  previous = null;
  hadOwnProperty = false;
  return bus !== null;
}

/// The installed bus, or null. Tests and M3's dev controls reach the boards
/// through this; Studio only ever sees `navigator.serial`.
export function bus() {
  return installed;
}

class VirtualSerial extends EventTarget {
  constructor(backing) {
    super();
    this.backing = backing;
    // Every port ever minted for a board, live or dead, newest last. The
    // dead ones stay reachable so `getInfo()` still answers on them.
    this.generations = [];
    this.granted = new Set();
    // Board ids in the order `GET /boards` gave them. Enumeration order is
    // stable across a re-enumeration because of this: `adoptReenumeratedPorts`
    // pairs dead generations to replacements IN ORDER, so a bus that shuffled
    // its ports when a board rebooted would cross two boards' sessions.
    this.boardIds = [];
  }

  async load(only) {
    const boards = await this.backing.listBoards();
    for (const board of boards) {
      const id = board?.id ?? board;
      if (only && !only.includes(id)) {
        continue;
      }
      const emulator = await this.backing.connect(id, board);
      const port = this.mint(id, emulator, board);
      this.boardIds.push(id);
      this.granted.add(port);
      emulator.on("reenumerate", () => this.reenumerate(id));
    }
  }

  mint(boardId, emulator, board) {
    const port = new VirtualSerialPort(this, boardId, emulator, board);
    this.generations.push(port);
    return port;
  }

  livePortFor(boardId) {
    for (let i = this.generations.length - 1; i >= 0; i -= 1) {
      const port = this.generations[i];
      if (port.boardId === boardId && !port.dead) {
        return port;
      }
    }
    return null;
  }

  // The board went back to power-on. Chrome's answer to that is a NEW
  // `SerialPort` object with the same grant, and so is ours: the old one goes
  // dead (enumerated no longer, `getInfo()` still answering) and the new one
  // takes its place in the granted set, with `disconnect` then `connect` on
  // the bus so Studio's hotplug sweep re-derives.
  reenumerate(boardId) {
    const old = this.livePortFor(boardId);
    if (!old) {
      return;
    }
    old.markDead();
    const wasGranted = this.granted.delete(old);
    const fresh = this.mint(boardId, old.emulator, old.board);
    if (wasGranted) {
      this.granted.add(fresh);
    }
    this.dispatchEvent(new CustomEvent("disconnect", { detail: { port: old } }));
    this.dispatchEvent(new CustomEvent("connect", { detail: { port: fresh } }));
  }

  forget(port) {
    this.granted.delete(port);
  }

  // --- the three `navigator.serial` calls ---------------------------------

  /// Granted ports, newest generation per board, in board order, no prompt.
  async getPorts() {
    const ports = [];
    for (const boardId of this.boardIds) {
      const port = this.livePortFor(boardId);
      if (port && this.granted.has(port)) {
        ports.push(port);
      }
    }
    return ports;
  }

  /// There is no chooser under the shim. M2 resolves to the first admitted
  /// board and the suite says so; M3 replaces `resolveRequestedPort` with the
  /// in-page picker without touching the port double.
  async requestPort(options = {}) {
    const port = this.resolveRequestedPort(options?.filters ?? null);
    if (!port) {
      // What Chrome throws when the chooser closes with nothing — upstream
      // maps `NotFoundError` to "cancelled"
      // (`browser_esp32_device_controller.js:36-40`).
      const error = new Error("No port selected by the user.");
      error.name = "NotFoundError";
      throw error;
    }
    this.granted.add(port);
    return port;
  }

  resolveRequestedPort(filters) {
    for (const boardId of this.boardIds) {
      const port = this.livePortFor(boardId);
      if (port && matchesFilters(port.getInfo(), filters)) {
        return port;
      }
    }
    return null;
  }

  async dispose() {
    const emulators = new Set();
    for (const port of this.generations) {
      port.markDead();
      emulators.add(port.emulator);
    }
    this.generations = [];
    this.boardIds = [];
    this.granted.clear();
    for (const emulator of emulators) {
      await emulator.dispose();
    }
  }
}

function matchesFilters(info, filters) {
  if (!Array.isArray(filters) || filters.length === 0) {
    return true;
  }
  return filters.some((filter) => {
    if (filter?.usbVendorId !== undefined && filter.usbVendorId !== info.usbVendorId) {
      return false;
    }
    if (filter?.usbProductId !== undefined && filter.usbProductId !== info.usbProductId) {
      return false;
    }
    return true;
  });
}

class VirtualSerialPort {
  constructor(bus, boardId, emulator, board) {
    this.bus = bus;
    this.boardId = boardId;
    this.emulator = emulator;
    this.board = board;
    this.dead = false;
    this.opened = false;
    this._readable = null;
    this._writable = null;
    this._streamController = null;
    this._unsubscribe = null;
    this._offBytesError = null;
  }

  // `browser_esp32_device_controller.js:317` tests openness as
  // `Boolean(port?.readable || port?.writable)`, so these are NULL while
  // closed and non-null while open. That is behaviour, not a convenience.
  get readable() {
    return this._readable;
  }

  get writable() {
    return this._writable;
  }

  getInfo() {
    return { usbVendorId: VENDOR_ID, usbProductId: PRODUCT_ID };
  }

  async open({ baudRate } = {}) {
    if (this.dead) {
      // A dead generation's `open()` fails instantly, exactly as Chrome's
      // does — the bench failure `getPort`'s adoption pass exists for.
      throw domError("NetworkError", "Failed to open serial port.");
    }
    if (this.opened) {
      throw domError("InvalidStateError", "The port is already open.");
    }
    // The baud rate is real hardware's business: a USB-Serial-JTAG device
    // ignores it, and so does the emulator. Recorded, never sent.
    this.baudRate = baudRate ?? null;
    await this.emulator.open();
    this.opened = true;
    this._attachStreams();
  }

  async close() {
    if (!this.opened) {
      throw domError("InvalidStateError", "The port is already closed.");
    }
    this.opened = false;
    this._detachStreams();
    await this.emulator.close();
  }

  async setSignals(signals = {}) {
    if (!this.opened) {
      throw domError("InvalidStateError", "The port is not open.");
    }
    await this.emulator.signals({
      dtr: signals.dataTerminalReady,
      rts: signals.requestToSend,
    });
  }

  // The emulator models the HOST's side of the cable, not a modem's, so these
  // four names carry the four facts `state` reports: the cable (`host`), the
  // bus (`sof`) and whether an application is draining the IN endpoint. A
  // native-USB port has no ring indicator to report and says so.
  async getSignals() {
    const state = await this.emulator.state();
    return {
      clearToSend: state.draining === true,
      dataCarrierDetect: state.sof === "on",
      dataSetReady: state.host === "attached",
      ringIndicator: false,
    };
  }

  /// Revoke the grant. The port stops being enumerated; the object survives,
  /// as Chrome's does, and `requestPort` can grant it again.
  async forget() {
    if (this.opened) {
      await this.close();
    }
    this.bus.forget(this);
  }

  markDead() {
    this.dead = true;
    if (this.opened) {
      this.opened = false;
      this._detachStreams();
    }
  }

  _attachStreams() {
    const port = this;
    this._readable = new ReadableStream({
      start(controller) {
        port._streamController = controller;
        port._unsubscribe = port.emulator.onBytes((bytes) => {
          try {
            controller.enqueue(bytes);
          } catch {
            // the consumer cancelled between frames
          }
        });
        port._offBytesError = port.emulator.on("byteserror", (detail) => {
          port._errorStream(detail?.reason ?? "The device has been lost.");
        });
      },
      cancel() {
        port._releaseByteListeners();
      },
    });
    this._writable = new WritableStream({
      write(chunk) {
        port.emulator.write(chunk);
      },
    });
  }

  _detachStreams() {
    this._releaseByteListeners();
    const readable = this._readable;
    this._readable = null;
    this._writable = null;
    this._streamController = null;
    if (readable && !readable.locked) {
      readable.cancel().catch(() => {});
    }
  }

  // The device went away under an open port. Chrome errors the `readable`
  // stream and the reader's pending `read()` rejects; the controller's read
  // pump catches that and pushes it to `takeErrors`.
  _errorStream(reason) {
    const controller = this._streamController;
    this._releaseByteListeners();
    this.opened = false;
    this._readable = null;
    this._writable = null;
    this._streamController = null;
    if (controller) {
      try {
        controller.error(domError("NetworkError", reason));
      } catch {
        // already closed or errored
      }
    }
  }

  _releaseByteListeners() {
    this._unsubscribe?.();
    this._offBytesError?.();
    this._unsubscribe = null;
    this._offBytesError = null;
  }
}

function domError(name, message) {
  if (typeof DOMException === "function") {
    return new DOMException(message, name);
  }
  const error = new Error(message);
  error.name = name;
  return error;
}
