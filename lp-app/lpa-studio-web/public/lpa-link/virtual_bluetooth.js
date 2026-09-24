// The `navigator.bluetooth` polyfill for `?ble=emu`: Web Bluetooth's NUS
// service over the emulated boards `?emu=` already holds.
//
// Installed, Studio's Bluetooth stack — `browser_ble.js`, the Rust link and
// transport above it, the device card, Play mode — runs against it
// UNCHANGED. That is the claim, and `lpa-link/tests/browser_ble_conformance.rs`
// pins it (CI runs it in Firefox against this file alone).
//
// THE GATT SUBSET, and nothing more — exactly what `browser_ble.js` calls:
//
//   navigator.bluetooth.{requestDevice, getDevices, getAvailability}
//   device.{id, name, gatt, forget, addEventListener("gattserverdisconnected")}
//   gatt.{connected, connect, disconnect}
//   server.getPrimaryService(NUS) → service.getCharacteristic(RX | TX)
//   rx.writeValueWithResponse(bytes)
//   tx.{startNotifications, addEventListener("characteristicvaluechanged"), value}
//
// ONE PLUMBING, NOT TWO. The NUS characteristics pipe bytes to the SAME
// `EmulatorPort` the `navigator.serial` bus holds for each board (`emu serve`
// over a socket, or the tab's Worker): RX writes are the port's `write`, TX
// notifications are its `onBytes`, re-cut into ≤244-byte notifications the
// way a 247-MTU link delivers them. A board is therefore reachable over
// "Bluetooth" OR over serial in one page, not both at once — its one byte
// channel can only be opened once, which is the door's rule, not ours.
//
// THE CABLE IS THE RADIO. The dev banner's `detach` takes a board out of
// range: an open GATT connection drops (`gattserverdisconnected`), and
// `connect()` fails until `attach` brings it back. That is what exercises
// Studio's reconnect path with no board and no phone.
//
// ⚠️ TRUST: THIS PROVES THE TRANSPORT, THE UI AND PLAY MODE — NOT ACCESS
// ENFORCEMENT. The emulated firmware sees its USB-Serial-JTAG link, which the
// board trusts (USB is `Edit`; physical possession is the recovery path), so
// every request is answered at the edit tier here. What a real Bluetooth
// link may do before login is proven by M3's host tests and M4's desk check,
// never by this file. The banner says so too.

export const NUS_SERVICE = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";
export const NUS_RX = "6e400002-b5a3-f393-e0a9-e50e24dcca9e";
export const NUS_TX = "6e400003-b5a3-f393-e0a9-e50e24dcca9e";

/// A notification's payload on a 247-byte ATT MTU — what iOS and macOS
/// both negotiated in the M2 runs.
const NOTIFY_CHUNK_BYTES = 244;
/// The largest write a central may make with response (a long write).
const MAX_WRITE_BYTES = 512;

let installed = null;
let previous = null;

/// Build the polyfill over a `virtual_serial.js` bus WITHOUT touching
/// `navigator.bluetooth` (the dev seam defines a synchronous façade and hands
/// it this when it arrives, the `?emu=` pattern).
///
/// `picker` — `(candidates) => Promise<boardId | null>`, the in-page chooser.
/// Without one, `requestDevice()` resolves to the first board in range.
export function createBluetooth(bus, { picker = null } = {}) {
  return new VirtualBluetooth(bus, picker);
}

/// Replace `navigator.bluetooth` with a polyfill over `bus`'s boards.
export function install(bus, options = {}) {
  if (installed) {
    uninstall();
  }
  const bluetooth = createBluetooth(bus, options);
  previous = Object.getOwnPropertyDescriptor(globalThis.navigator, "bluetooth") ?? null;
  Object.defineProperty(globalThis.navigator, "bluetooth", {
    value: bluetooth,
    configurable: true,
    enumerable: true,
    writable: false,
  });
  installed = bluetooth;
  return bluetooth;
}

/// Put `navigator.bluetooth` back the way it was found.
export function uninstall() {
  const bluetooth = installed;
  installed = null;
  bluetooth?.dispose();
  if (previous) {
    Object.defineProperty(globalThis.navigator, "bluetooth", previous);
  } else {
    delete globalThis.navigator.bluetooth;
  }
  previous = null;
  return bluetooth !== null;
}

/// The installed polyfill, or null.
export function bluetooth() {
  return installed;
}

class VirtualBluetooth extends EventTarget {
  constructor(bus, picker) {
    super();
    this.bus = bus;
    this.picker = picker;
    this.devices = new Map();
    this.granted = new Set();
    // Board ids out of range: the cable is out.
    this.away = new Set();
    this.onCableOut = (event) => {
      const boardId = event?.detail?.port?.boardId;
      if (!boardId) return;
      this.away.add(boardId);
      this.devices.get(boardId)?.gatt.drop("the board went out of range");
    };
    this.onCableIn = (event) => {
      const boardId = event?.detail?.port?.boardId;
      if (boardId) this.away.delete(boardId);
    };
    bus.addEventListener("disconnect", this.onCableOut);
    bus.addEventListener("connect", this.onCableIn);
  }

  // --- the three `navigator.bluetooth` calls -------------------------------

  async requestDevice(options = {}) {
    if (!wantsNus(options)) {
      throw domError("NotFoundError", "No device offers the requested services.");
    }
    const candidates = this.bus.boardIds.filter((boardId) => this.inRange(boardId));
    let chosen = null;
    if (this.picker && candidates.length > 0) {
      chosen = await this.picker(candidates.map((boardId) => this.describe(boardId)));
    } else {
      chosen = candidates[0] ?? null;
    }
    if (chosen === null || chosen === undefined || !candidates.includes(chosen)) {
      // What Chrome throws when its chooser closes with nothing.
      throw domError("NotFoundError", "User cancelled the requestDevice() chooser.");
    }
    this.granted.add(chosen);
    return this.deviceFor(chosen);
  }

  async getDevices() {
    return [...this.granted].map((boardId) => this.deviceFor(boardId));
  }

  async getAvailability() {
    return true;
  }

  // --- page-internal (the banner, the walk, the suite; never Studio) -------

  inRange(boardId) {
    const port = this.bus.newestPortFor(boardId);
    return Boolean(port) && !this.away.has(boardId);
  }

  deviceFor(boardId) {
    let device = this.devices.get(boardId);
    if (!device) {
      device = new VirtualBluetoothDevice(this, boardId);
      this.devices.set(boardId, device);
    }
    return device;
  }

  describe(boardId) {
    const port = this.bus.newestPortFor(boardId);
    const mac = port?.board?.mac ?? null;
    return {
      boardId,
      mac,
      chip: port?.board?.chip ?? null,
      name: advertisedName(boardId, mac),
      granted: this.granted.has(boardId),
      connected: this.devices.get(boardId)?.gatt.connected ?? false,
    };
  }

  /// Bytes on the air per board, both ways — what the walk reads to state
  /// the idle cost of a connected Studio (M5 director notes).
  stats(boardId) {
    const gatt = this.devices.get(boardId)?.gatt;
    return gatt ? { ...gatt.stats } : null;
  }

  /// Drop a connection WITHOUT telling the page — the shape of a drop iOS
  /// holds back while the page is hidden. `gatt.connected` goes false and no
  /// event fires; only a re-check can find it.
  silentDrop(boardId) {
    this.devices.get(boardId)?.gatt.drop("dropped while nobody was listening", { quiet: true });
  }

  /// The next `connect()` for this board never settles (Chrome, Run B).
  hangNextConnect(boardId) {
    this.deviceFor(boardId).gatt.hangNext = true;
  }

  dispose() {
    this.bus.removeEventListener("disconnect", this.onCableOut);
    this.bus.removeEventListener("connect", this.onCableIn);
    for (const device of this.devices.values()) {
      device.gatt.drop("the polyfill was removed", { quiet: true });
    }
    this.devices.clear();
    this.granted.clear();
  }
}

class VirtualBluetoothDevice extends EventTarget {
  constructor(bluetooth, boardId) {
    super();
    this.bluetooth = bluetooth;
    this.boardId = boardId;
    const mac = bluetooth.bus.newestPortFor(boardId)?.board?.mac ?? null;
    // Opaque and per-origin in a real browser; stable per board here.
    this.id = `vble-${(mac ?? boardId).replace(/[^0-9a-zA-Z]/g, "")}`;
    this.name = advertisedName(boardId, mac);
    this.gatt = new VirtualGattServer(this);
  }

  async forget() {
    this.gatt.disconnect();
    this.bluetooth.granted.delete(this.boardId);
  }
}

class VirtualGattServer {
  constructor(device) {
    this.device = device;
    this.connected = false;
    this.emulator = null;
    this.service = new VirtualNusService(this);
    this.hangNext = false;
    this.offBytes = null;
    this.offError = null;
    this.stats = { written: 0, writes: 0, notified: 0, notifications: 0, connects: 0 };
  }

  async connect() {
    const bluetooth = this.device.bluetooth;
    if (this.hangNext) {
      this.hangNext = false;
      return new Promise(() => {});
    }
    if (this.connected) {
      return this;
    }
    if (!bluetooth.inRange(this.device.boardId)) {
      throw domError("NetworkError", "Bluetooth Device is no longer in range.");
    }
    const port = bluetooth.bus.newestPortFor(this.device.boardId);
    const emulator = port.emulator;
    // Opening the byte channel IS the board's USB port opening (the door's
    // coupling rule). If the serial side of this page holds it, this says so.
    await emulator.open();
    this.emulator = emulator;
    this.connected = true;
    this.stats.connects += 1;
    this.offBytes = emulator.onBytes((bytes) => this.service.tx.deliver(bytes));
    this.offError = emulator.on("byteserror", () => this.drop("the board's link failed"));
    return this;
  }

  disconnect() {
    // A page-initiated disconnect fires the event too, as Chrome's does.
    this.drop("disconnected by the page");
  }

  drop(_why, { quiet = false } = {}) {
    if (!this.connected) {
      return;
    }
    this.connected = false;
    this.offBytes?.();
    this.offError?.();
    this.offBytes = null;
    this.offError = null;
    const emulator = this.emulator;
    this.emulator = null;
    emulator?.close().catch(() => {});
    if (!quiet) {
      this.device.dispatchEvent(new Event("gattserverdisconnected"));
    }
  }

  async getPrimaryService(uuid) {
    if (!this.connected) {
      throw domError("NetworkError", "GATT Server is disconnected.");
    }
    if (String(uuid).toLowerCase() !== NUS_SERVICE) {
      throw domError("NotFoundError", `No Services matching UUID ${uuid} found in Device.`);
    }
    return this.service;
  }
}

class VirtualNusService {
  constructor(gatt) {
    this.gatt = gatt;
    this.rx = new VirtualRxCharacteristic(gatt);
    this.tx = new VirtualTxCharacteristic(gatt);
  }

  async getCharacteristic(uuid) {
    switch (String(uuid).toLowerCase()) {
      case NUS_RX:
        return this.rx;
      case NUS_TX:
        return this.tx;
      default:
        throw domError("NotFoundError", `No Characteristics matching UUID ${uuid} found in Service.`);
    }
  }
}

class VirtualRxCharacteristic {
  constructor(gatt) {
    this.gatt = gatt;
  }

  async writeValueWithResponse(value) {
    const gatt = this.gatt;
    if (!gatt.connected || !gatt.emulator) {
      throw domError("NetworkError", "GATT Server is disconnected.");
    }
    const bytes = toBytes(value);
    if (bytes.length > MAX_WRITE_BYTES) {
      throw domError("InvalidModificationError", "Value can't exceed 512 bytes.");
    }
    gatt.emulator.write(bytes);
    gatt.stats.writes += 1;
    gatt.stats.written += bytes.length;
  }
}

class VirtualTxCharacteristic extends EventTarget {
  constructor(gatt) {
    super();
    this.gatt = gatt;
    this.value = null;
    this.notifying = false;
  }

  async startNotifications() {
    if (!this.gatt.connected) {
      throw domError("NetworkError", "GATT Server is disconnected.");
    }
    this.notifying = true;
    return this;
  }

  deliver(bytes) {
    if (!this.notifying) {
      return;
    }
    const data = toBytes(bytes);
    for (let offset = 0; offset < data.length; offset += NOTIFY_CHUNK_BYTES) {
      const chunk = data.slice(offset, offset + NOTIFY_CHUNK_BYTES);
      this.value = new DataView(chunk.buffer, chunk.byteOffset, chunk.byteLength);
      this.gatt.stats.notifications += 1;
      this.gatt.stats.notified += chunk.length;
      this.dispatchEvent(new Event("characteristicvaluechanged"));
    }
  }
}

function wantsNus(options) {
  const services = [
    ...(options?.filters ?? []).flatMap((filter) => filter?.services ?? []),
    ...(options?.optionalServices ?? []),
  ].map((uuid) => String(uuid).toLowerCase());
  return services.includes(NUS_SERVICE);
}

/// The name the firmware advertises (`LP-<last two MAC bytes>`), so the
/// chooser and the card read like a real board's.
function advertisedName(boardId, mac) {
  const tail = String(mac ?? "")
    .replace(/[^0-9a-fA-F]/g, "")
    .slice(-4)
    .toLowerCase();
  return tail ? `LP-${tail}` : `LP-${boardId}`;
}

function toBytes(value) {
  if (value instanceof Uint8Array) return value;
  if (ArrayBuffer.isView(value)) {
    return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  }
  return new Uint8Array(value);
}

function domError(name, message) {
  if (typeof DOMException === "function") {
    return new DOMException(message, name);
  }
  const error = new Error(message);
  error.name = name;
  return error;
}
