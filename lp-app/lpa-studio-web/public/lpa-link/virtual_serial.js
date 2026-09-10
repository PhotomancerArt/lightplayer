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
//  1. **A REPLUG mints a NEW `SerialPort` object.** After one, `getPorts()`
//     returns a new object for that board and the old one stops being
//     enumerated — while still answering `getInfo()`, because that is how
//     `adoptReenumeratedPorts` pairs the dead generation to its replacement
//     (G1 2026-08-31: a replugged C6 wallpapered the gallery).
//  2. **A closed port is still a granted port.** `close()` releases the
//     streams and nothing else; only `forget()` revokes the grant and takes
//     the port out of `getPorts()` (2026-07-22: deleting the grant handle on
//     close broke flashing).
//
// A CHIP RESET IS NOT A REPLUG, and that is a ruling, not an omission (plan
// two M5). On this part the USB-Serial-JTAG controller is in the same silicon
// block as the CPU it resets, so the USB device survives every reset the
// serial channel can ask for — which is exactly why Studio can flash a real
// C6 over Web Serial today without the port vanishing under esptool-js. This
// file claims `303a:1001`, native USB, so it must model native USB: a bus
// that re-enumerated on reset would be modelling a USB-CDC BRIDGE (an S3 over
// OTG, a CH340 whose DTR/RTS dance really does drop the port) while calling
// itself the other thing.
//
// MEASURED, on the real esptool-js/Chrome path (2026-09-09, `emu serve`
// holding a `kind=rom-up` board, Studio's own Flash firmware verb):
//
//   [m5] +9.02s EmulatorPort._reenumerated c6-c (lastCycle=2965920001)
//   [m5] +9.02s bus.reenumerate c6-c -> mints a NEW SerialPort
//   [m5] +9.02s port.markDead c6-c (opened was true)
//   [esp32-flash] esptool.js
//   [esp32-flash] Connecting...
//   [esp32-flash] emulated board c6-c is already open in this page
//
// `browser_esp32_flash.js` holds ONE `port` object for the whole call and is
// frozen, so "it re-enumerates and the flash flow tolerates it" is not
// available: the flash died between "Connecting…" and the chip guard, every
// time. The reboot is still OBSERVED — `EmulatorPort` fires `reboot` when a
// control reply's guest cycle goes backwards, and the banner re-renders — it
// simply does not mint a port. The cable (`detach`/`attach`) remains the one
// thing that does.
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

/// Build the bus over `baseUrl`'s emulated boards WITHOUT touching
/// `navigator.serial`.
///
/// `install()` below is this plus the `defineProperty`, and it is what the
/// conformance suite uses. M3's dev-mode seam (`index.html`) wants the halves
/// apart: it defines `navigator.serial` SYNCHRONOUSLY, as a façade, in an
/// inline script — because a dynamic `import()` cannot promise to resolve
/// before the wasm bundle boots — and hands the façade this bus when it
/// arrives. A second `defineProperty` from in here would take the façade's
/// event listeners off the page with it.
///
/// `boards` — an array of board ids to admit, when the page wants a subset.
/// `backing` — an `{ describe, listBoards, connect }` triple; the default is
/// `emu serve` over `baseUrl`, and the conformance suite passes a scripted
/// double instead so CI needs no server, no sockets and no firmware.
/// `picker` — `(candidates) => Promise<boardId | null>`, the in-page chooser
/// (M3). With one, `requestPort()` asks it and the page starts with NO grants,
/// which is what a fresh Chrome profile looks like. Without one, every board
/// is granted at load and `requestPort()` resolves to the first match.
export async function createBus(baseUrl, { boards = null, backing = null, picker = null } = {}) {
  const source = backing ?? nativeBacking(baseUrl);
  const bus = new VirtualSerial(source, picker);
  await bus.load(boards);
  return bus;
}

/// Replace `navigator.serial` with a bus over `baseUrl`'s emulated boards.
export async function install(baseUrl, options = {}) {
  if (installed) {
    await uninstall();
  }
  const bus = await createBus(baseUrl, options);

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
  constructor(backing, picker = null) {
    super();
    this.backing = backing;
    // The in-page chooser, or null. See `createBus`.
    this.picker = picker;
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
    const boards = only?.length ? only.map((id) => ({ id })) : await this.list();
    for (const board of boards) {
      const id = board?.id ?? board;
      const emulator = await this.backing.connect(id, board);
      const port = this.mint(id, emulator, board);
      this.boardIds.push(id);
      // A page with a chooser starts with no grants: Chrome hands a fresh
      // profile an empty `getPorts()` until the chooser resolves one, and a
      // shim that pre-granted every board would auto-connect Studio to all of
      // them and leave the picker unreachable. A page WITHOUT a chooser has no
      // way to grant at all, so there every board is granted at load — which
      // is the shape M2's suite pins.
      if (!this.picker) {
        this.granted.add(port);
      }
      // The board went back to power-on. The port SURVIVES (see the header):
      // all this does is let the page's own chrome re-read it.
      emulator.on("reboot", () => this.noteState(this.newestPortFor(id)));
    }
  }

  // `GET /boards` is a CROSS-ORIGIN fetch whenever the page and the server
  // are not the same origin — and they never are, because `emu serve` binds
  // its own port. M2 measured that against the door as merged and it failed:
  // the reply carried no `Access-Control-Allow-Origin`, so the browser refused
  // to let the page read it and this rejected with a bare `TypeError: Failed
  // to fetch`. M1.1 (PR #646, merged 2026-09-09) puts the header on every
  // plain HTTP reply, so the registry path is the live one now and M3's dev
  // seam uses it — the board ids, the MACs and the flash state on the picker
  // all come from here. `install(url, { boards: [...] })` remains for a page
  // that wants a named subset; the WebSockets never needed either, a handshake
  // not being subject to CORS.
  async list() {
    try {
      return await this.backing.listBoards();
    } catch (error) {
      throw new Error(
        `${this.backing.describe()}: could not read the board registry ` +
          `(${error?.message ?? error}). A cross-origin GET /boards needs an ` +
          `Access-Control-Allow-Origin header — check the server is an ` +
          `\`lp-cli emu serve\` new enough to send it (PR #646) — or name the ` +
          `boards with install(url, { boards: ["c6-a"] }) instead.`,
      );
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

  /// The newest generation for a board, live or dead. A detached board has no
  /// live port — `getPorts()` and the picker must not offer it — but the page
  /// still has to name it in the banner, and `attach` still has to find the
  /// `EmulatorPort` underneath to plug back in.
  newestPortFor(boardId) {
    for (let i = this.generations.length - 1; i >= 0; i -= 1) {
      if (this.generations[i].boardId === boardId) {
        return this.generations[i];
      }
    }
    return null;
  }

  /// The board enumerated again — which on this bus means the CABLE went out
  /// and back in, and nothing else (a chip reset does not; see the header).
  /// Chrome's answer to a replug is a NEW `SerialPort` object with the same
  /// grant, and so is ours: the old one goes dead (enumerated no longer,
  /// `getInfo()` still answering) and the new one takes its place in the
  /// granted set, with `disconnect` then `connect` on the bus so Studio's
  /// hotplug sweep re-derives.
  reenumerate(boardId) {
    const old = this.livePortFor(boardId);
    if (!old) {
      return;
    }
    old.markDead();
    const fresh = this.adopt(old);
    this.dispatchEvent(new CustomEvent("disconnect", { detail: { port: old } }));
    this.dispatchEvent(new CustomEvent("connect", { detail: { port: fresh } }));
    return fresh;
  }

  /// Mint the generation that replaces `old` and move its grant onto it.
  adopt(old) {
    const wasGranted = this.granted.delete(old);
    const fresh = this.mint(old.boardId, old.emulator, old.board);
    if (wasGranted) {
      this.granted.add(fresh);
    }
    return fresh;
  }

  forget(port) {
    this.granted.delete(port);
    this.noteState(port);
  }

  /// A port opened, closed or lost its grant. This is NOT a Web Serial event —
  /// Studio listens for `connect`/`disconnect` and nothing else — it exists so
  /// the page's own chrome can stop saying "closed" about a port an
  /// application is holding open. Measured 2026-09-09: the dev banner rendered
  /// only on hotplug edges and said "closed" under two boards Studio had open.
  noteState(port) {
    this.dispatchEvent(new CustomEvent("boardstate", { detail: { port } }));
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

  /// There is no BROWSER chooser under the shim, so the page draws one: with
  /// a `picker` the request resolves through it, and without one it resolves
  /// to the first admitted board (M2's shape, which the suite pins).
  ///
  /// Either way this is the same call `browser_esp32_device_controller.js:41`
  /// makes, with the same filters, and it answers with the same two outcomes:
  /// a port, or `NotFoundError` for "the user closed it with nothing".
  async requestPort(options = {}) {
    const filters = options?.filters ?? null;
    const port = this.picker
      ? await this.pickPort(filters)
      : this.resolveRequestedPort(filters);
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

  /// Offer the filtered boards to the page's picker and take its answer.
  /// A picker that answers null — the user closed it — is a cancelled
  /// chooser, and the caller above turns that into `NotFoundError`.
  async pickPort(filters) {
    const candidates = [];
    for (const boardId of this.boardIds) {
      const port = this.livePortFor(boardId);
      if (port && matchesFilters(port.getInfo(), filters)) {
        candidates.push(port);
      }
    }
    if (candidates.length === 0) {
      return null;
    }
    const chosen = await this.picker(candidates.map((port) => this.describe(port)));
    if (chosen === null || chosen === undefined) {
      return null;
    }
    return candidates.find((port) => port.boardId === chosen) ?? null;
  }

  /// What the page's chrome is allowed to know about a board: the registry
  /// record `GET /boards` gave, plus this page's own grant/open state. Studio
  /// never sees any of it — it holds a `SerialPort`, and that is the point.
  describe(port) {
    const info = port.getInfo();
    return {
      boardId: port.boardId,
      mac: port.board?.mac ?? null,
      chip: port.board?.chip ?? null,
      flash: port.board?.flash ?? null,
      link: port.board?.link ?? null,
      usbVendorId: info.usbVendorId,
      usbProductId: info.usbProductId,
      granted: this.granted.has(port),
      open: port.opened,
      attached: !port.dead,
    };
  }

  /// Every board the bus holds, described, newest generation first — a
  /// DETACHED board included, because the banner has to name it and offer the
  /// cable back. `getPorts()` and the picker are the ones that must not see it.
  describeBoards() {
    return this.boardIds
      .map((boardId) => this.newestPortFor(boardId))
      .filter((port) => port !== null)
      .map((port) => this.describe(port));
  }

  // --- the cable, as a page-level control ---------------------------------
  //
  // `attach` and `detach` are control verbs — a cable going in and coming out
  // — and they are never implied by a socket (`lp-emu/esp/README.md`). Chrome
  // answers a replug with `connect`/`disconnect` on `navigator.serial`, so the
  // bus answers these with exactly one of each: `installSerialEvents` wires
  // them to `StudioCommand::DeviceHotplug`, and both edges are re-derivation
  // triggers carrying no port (`web_app.rs:1908-1913`).

  /// The cable comes out. The emulator hears the verb, and the PORT dies the
  /// way Chrome's does when a board is unplugged: an open `readable` errors,
  /// the port stops being enumerated, and `disconnect` fires.
  ///
  /// MEASURED 2026-09-09, and the reason this is not just the verb: with the
  /// port left open, Studio's hotplug sweep saw a link that was still open,
  /// re-derived nothing, and the card stayed Ready with the cable out. The
  /// edge is only half of what a replug is — the other half is that the port
  /// object goes away, and the sweep is written against exactly that
  /// ("disconnect" means detach the links that stopped being open,
  /// `web_app.rs:1908-1913`).
  async detach(boardId) {
    const port = this.requireLivePort(boardId);
    await port.emulator.detach();
    port.unplug("The emulated board was detached.");
    this.dispatchEvent(new CustomEvent("disconnect", { detail: { port } }));
    return port;
  }

  /// The cable goes back in. A replug is an enumeration, so the grant survives
  /// onto a NEW `SerialPort` object — the same thing `reenumerate` does after a
  /// reset, and the reason `adoptReenumeratedPorts` exists upstream.
  async attach(boardId) {
    const previous = this.newestPortFor(boardId);
    if (!previous) {
      throw new Error(`no emulated board \`${boardId}\` on this bus`);
    }
    await previous.emulator.attach();
    if (!previous.dead) {
      // Already plugged in: the verb is idempotent and the edge still fires,
      // because a page that pressed the button asked for a re-derivation.
      this.dispatchEvent(new CustomEvent("connect", { detail: { port: previous } }));
      return previous;
    }
    const fresh = this.adopt(previous);
    this.dispatchEvent(new CustomEvent("connect", { detail: { port: fresh } }));
    return fresh;
  }

  requireLivePort(boardId) {
    const port = this.livePortFor(boardId);
    if (!port) {
      throw new Error(`no live port for emulated board \`${boardId}\``);
    }
    return port;
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
    this.bus.noteState(this);
  }

  async close() {
    if (!this.opened) {
      throw domError("InvalidStateError", "The port is already closed.");
    }
    this.opened = false;
    this._detachStreams();
    await this.emulator.close();
    this.bus.noteState(this);
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

  /// The board was unplugged under this port. Same as `markDead`, except an
  /// open `readable` is ERRORED rather than cancelled — which is what Chrome
  /// does, and what the controller's read pump is written to catch.
  unplug(reason) {
    this.dead = true;
    if (this.opened) {
      this._errorStream(reason);
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
