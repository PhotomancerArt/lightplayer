// The `navigator.bluetooth` polyfill for `?ble=emu`: Web Bluetooth's NUS
// service over the emulated boards `?emu=` already holds.
//
// Installed, Studio's Bluetooth code — `browser_ble.js`, the Rust link, its
// lp-link end and transport above it, the device model, the device card,
// Play mode — runs against it unchanged, and `lpa-link/tests/browser_ble_conformance.rs`
// pins that (CI runs it in Firefox against this file alone). What is NOT
// the same as a real board is the link underneath, and this file is where
// the difference is made up:
//
// THE FRAMING IS TRANSLATED HERE (plan `lp2025/2026-09-28-1445-ble-on-lp-link`,
// D9). A real board's Bluetooth link is lp-link on DATAGRAMS: one frame per
// GATT write and per notification (`LinkConfig::ble()`). The emulated board
// has no radio; what this polyfill reaches is its USB-Serial-JTAG link,
// lp-link on a STREAM (`LinkConfig::usb()`: `0x00 COBS-FF(frame) 0x00`,
// console text between frames). So each RX write — one frame — goes to the
// board wrapped for the stream, and the board's stream is cut back into
// frames, one notification each; console text between frames goes nowhere
// (a real board sends none over the air). The frame inside is the same bytes
// either way (header, payload, CRC-32C keyed by both nonces), so Studio's
// datagram link and the board's stream link complete a real handshake and a
// real session. What the two ends do NOT share is a config: the board runs
// USB's timers (1 s stall, 250 ms keepalive) and Studio runs Bluetooth's
// (3.5 s stall, 1 s keepalive), and the payload is the smaller of the two
// SYNs' (Studio's 180 B, not a real board's per-MTU size). This proves the
// Rust and JS above the framing; it proves nothing about a radio, an MTU,
// or the board's radio-link code (`fw-esp32-common/src/radio_link/`), which
// never runs here.
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
// ONE PLUMBING, NOT TWO. The NUS characteristics reach the SAME
// `EmulatorPort` the `navigator.serial` bus holds for each board (`emu serve`
// over a socket, or the tab's Worker): an RX write is the port's `write` (the
// frame, stream-wrapped), and TX notifications come from its `onBytes` (the
// stream, cut into frames). A board is therefore reachable over "Bluetooth"
// OR over serial in one page, not both at once — its one byte channel can
// only be opened once, which is the door's rule, not ours.
//
// THE LINK OPENS ON SUBSCRIBE, as the firmware's does: `gatt.connect()`
// reaches the board, and the board's side of the link (the byte channel)
// opens only when the central starts TX notifications; a write before that
// is refused, where a real board drops it. A GATT drop closes the byte
// channel, which is the emulated board's USB host going away — so the
// reconnect is a new lp-link session, as it is on a real board.
//
// M4's 10 s unauthenticated drop is modelled from `M!` hello LINES in the
// console text between frames. Since the USB cut-over (proto 30) a board's
// hello rides inside lp-link frames, which this file does not read, so the
// rule fires only for text a test injects (`deliverBytes`); the emulated
// board's own link is trusted USB and never asks anyway.
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

/// M4: an untrusted link that has not logged in is closed after this long.
const UNAUTHENTICATED_TIMEOUT_MS = 10_000;

/// One ATT value on a 247-byte ATT MTU (what macOS and the board agree):
/// the most one write or one notification carries. A longer write is an ATT
/// long write, which the firmware refuses (P3 of the BLE-on-lp-link plan),
/// so this refuses it too — one frame is always one plain write.
const ATT_VALUE_BYTES = 244;
/// A frame body longer than this between two `0x00`s is not a frame (the
/// board's USB link frames are ≤ 256 B of payload, COBS-FF at most doubles).
const MAX_STREAM_FRAME_BYTES = 1_024;

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
    // M4's unauthenticated-link timeout. Page-internal: the conformance
    // suite shortens it, because its runner allows the whole suite ~20 s.
    this.unauthTimeoutMs = UNAUTHENTICATED_TIMEOUT_MS;
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

  /// Tell the page its link is gone while the RADIO link stays up — Bluefy
  /// on iOS (G4, 2026-09-25): `gatt.connected` goes false and no event
  /// fires, but the board's side of the link stays open until the page calls
  /// `gatt.disconnect()`. A reconnect before that rides the old link.
  phantomDrop(boardId) {
    const gatt = this.devices.get(boardId)?.gatt;
    if (gatt) gatt.connected = false;
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
    // The board's side of the link is open only while TX is subscribed
    // (M4: "the link opens when the central SUBSCRIBES", not at connect).
    this.emulator = null;
    this.service = new VirtualNusService(this);
    this.hangNext = false;
    this.offBytes = null;
    this.offError = null;
    this.unauthTimer = null;
    this.lineTail = "";
    // The board's stream, cut back into frames (see the header).
    this.deframer = null;
    // `linkOpens`/`linkCloses`: the BOARD's side of the link (the port under
    // it opened or closed), which a phantom drop leaves open.
    // `textDropped`: console bytes between frames, which a real board never
    // sends over the air.
    this.stats = {
      written: 0,
      writes: 0,
      largestWrite: 0,
      notified: 0,
      notifications: 0,
      largestNotification: 0,
      connects: 0,
      linkOpens: 0,
      linkCloses: 0,
      textDropped: 0,
    };
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
    this.connected = true;
    this.stats.connects += 1;
    return this;
  }

  /// TX subscribed: the board opens this link. Opening the byte channel IS
  /// the board's port opening (the door's coupling rule); if the serial side
  /// of this page holds it, this says so.
  async openLink() {
    if (this.emulator) {
      return;
    }
    const port = this.device.bluetooth.bus.newestPortFor(this.device.boardId);
    const emulator = port.emulator;
    await emulator.open();
    this.emulator = emulator;
    this.stats.linkOpens += 1;
    this.lineTail = "";
    // A new byte channel is a new stream: nothing half-read carries over.
    this.deframer = new StreamDeframer(
      (frame) => this.service.tx.deliver(frame),
      (text) => {
        this.stats.textDropped += text.length;
        this.watchAuth(text);
      },
    );
    this.offBytes = emulator.onBytes((bytes) => this.deframer?.push(toBytes(bytes)));
    this.offError = emulator.on("byteserror", () => this.drop("the board's link failed"));
  }

  /// One lp-link frame from the page (one RX write), onto the board's
  /// stream (see the header).
  writeFrame(frame) {
    this.emulator.write(wrapStreamFrame(frame));
  }

  /// M4's rule, modelled from the board's OWN words: a link whose hello
  /// says `auth.required` with nothing granted must log in within 10 s or
  /// the board closes it. Read from `M!` lines in the console text between
  /// frames (see the header: a board's real hello is inside a frame, which
  /// this does not read). The emulated board's link is trusted USB, so its
  /// hello never asks and this never fires against it — it exists so the
  /// polyfill behaves like the firmware wherever a board DOES ask.
  watchAuth(bytes) {
    const text = this.lineTail + new TextDecoder().decode(bytes);
    const lines = text.split("\n");
    this.lineTail = lines.pop() ?? "";
    for (const line of lines) {
      if (!line.startsWith("M!")) continue;
      let frame;
      try {
        frame = JSON.parse(line.slice(2));
      } catch {
        continue;
      }
      const auth = findKey(frame, "auth");
      if (auth && auth.required === true && (auth.granted ?? null) === null && !this.unauthTimer) {
        this.unauthTimer = setTimeout(
          () => this.drop("an untrusted link did not log in within 10 s"),
          this.device.bluetooth.unauthTimeoutMs,
        );
      }
      const result = findKey(frame, "loginResult");
      if (result && result.granted) {
        clearTimeout(this.unauthTimer);
        this.unauthTimer = null;
      }
    }
  }

  disconnect() {
    // A page-initiated disconnect fires the event too, as Chrome's does.
    this.drop("disconnected by the page");
  }

  drop(_why, { quiet = false } = {}) {
    // A phantom-dropped link is not `connected` but still holds the board's
    // side; a drop (the page's disconnect) closes it all the same.
    if (!this.connected && !this.emulator) {
      return;
    }
    const wasConnected = this.connected;
    this.connected = false;
    clearTimeout(this.unauthTimer);
    this.unauthTimer = null;
    this.service.tx.notifying = false;
    this.offBytes?.();
    this.offError?.();
    this.offBytes = null;
    this.offError = null;
    this.deframer = null;
    const emulator = this.emulator;
    this.emulator = null;
    if (emulator) {
      this.stats.linkCloses += 1;
    }
    emulator?.close().catch(() => {});
    if (!quiet && wasConnected) {
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
    const frame = toBytes(value);
    if (frame.length > ATT_VALUE_BYTES) {
      // Past one ATT value a real central makes a long write (Prepare …
      // Execute), and the firmware refuses one: one frame, one plain write.
      throw domError(
        "NotSupportedError",
        `a ${frame.length}-byte write is an ATT long write, which the board refuses`,
      );
    }
    gatt.writeFrame(frame);
    gatt.stats.writes += 1;
    gatt.stats.written += frame.length;
    gatt.stats.largestWrite = Math.max(gatt.stats.largestWrite, frame.length);
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
    await this.gatt.openLink();
    return this;
  }

  /// One lp-link frame from the board: ONE notification, never cut or
  /// joined (a real board's rule). The emulated board's frames are at most
  /// 180 B of payload here (the smaller SYN wins), so every one fits an ATT
  /// value.
  deliver(frame) {
    if (!this.notifying) {
      return;
    }
    const data = toBytes(frame).slice();
    this.value = new DataView(data.buffer, data.byteOffset, data.byteLength);
    this.gatt.stats.notifications += 1;
    this.gatt.stats.notified += data.length;
    this.gatt.stats.largestNotification = Math.max(
      this.gatt.stats.largestNotification,
      data.length,
    );
    this.dispatchEvent(new Event("characteristicvaluechanged"));
  }
}

// --- lp-link's stream framing, just enough to translate --------------------
//
// The byte-level shape of `lp-base/lp-link`'s `Framing::Stream` with
// `escape_ff` (`frame.rs` `wrap_stream`, `cobs.rs` COBS-FF, `deframer.rs`):
// a frame is `0x00 COBS-FF(raw) 0x00`, where COBS-FF keeps both `0x00` and
// `0xFF` out of the body; bytes outside a frame are console text; a raw
// `0xFF` is the text mark (it abandons a frame in progress). The raw frame
// inside — header, payload, CRC — is exactly one datagram. Nothing here
// checks a CRC: the link on each end does.

/// COBS-FF's escape byte and what follows it.
const COBS_ESC = 0xfe;
const COBS_ESC_FF = 0x00;
const COBS_ESC_FE = 0x01;
/// The largest block code: 253 data bytes, no zero after.
const COBS_FF_FULL = 0xfe;
const COBS_FF_BLOCK = COBS_FF_FULL - 1;
/// The stream's text mark.
const TEXT_MARK = 0xff;

/// `0x00 COBS-FF(raw) 0x00`: one raw frame, wrapped for the stream.
export function wrapStreamFrame(raw) {
  const out = [0];
  let codeAt = out.length;
  let len = 0;
  out.push(0);
  const close = () => {
    out[codeAt] = len + 1;
    codeAt = out.length;
    len = 0;
    out.push(0);
  };
  const put = (b) => {
    out.push(b);
    len += 1;
    if (len === COBS_FF_BLOCK) close();
  };
  for (const b of toBytes(raw)) {
    if (b === 0) {
      close();
    } else if (b === 0xff) {
      put(COBS_ESC);
      close();
    } else if (b === COBS_ESC) {
      put(COBS_ESC);
      put(COBS_ESC_FE);
    } else {
      put(b);
    }
  }
  out[codeAt] = len + 1;
  out.push(0);
  return Uint8Array.from(out);
}

/// Undo COBS-FF on a frame body (the bytes between two `0x00`s); `null` when
/// it is not a valid encoding.
export function unwrapStreamFrame(body) {
  const joined = [];
  let i = 0;
  while (i < body.length) {
    const code = body[i];
    if (code === 0 || code === 0xff) return null;
    i += 1;
    const n = code - 1;
    if (i + n > body.length) return null;
    for (let k = i; k < i + n; k += 1) {
      if (body[k] === 0xff) return null;
      joined.push(body[k]);
    }
    i += n;
    if (code !== COBS_FF_FULL && i < body.length) joined.push(0);
  }
  const out = [];
  for (let r = 0; r < joined.length; r += 1) {
    if (joined[r] !== COBS_ESC) {
      out.push(joined[r]);
      continue;
    }
    const next = joined[r + 1];
    if (next === COBS_ESC_FF) out.push(0xff);
    else if (next === COBS_ESC_FE) out.push(COBS_ESC);
    else return null;
    r += 1;
  }
  return Uint8Array.from(out);
}

/// The board's stream → whole raw frames (`onFrame`) and console text
/// (`onText`, at a newline or before a frame), mirroring lp-link's own
/// deframer's resync rules: a body that does not decode takes its closing
/// `0x00` as the opening of the next frame.
export class StreamDeframer {
  constructor(onFrame, onText) {
    this.onFrame = onFrame;
    this.onText = onText;
    this.inFrame = false;
    this.discarding = false;
    this.body = [];
    this.text = [];
  }

  push(bytes) {
    for (const b of bytes) {
      if (b === TEXT_MARK) {
        this.inFrame = false;
        this.discarding = false;
        this.body = [];
        continue;
      }
      if (b === 0) {
        if (!this.inFrame) {
          this.inFrame = true;
          this.body = [];
          this.flushText();
          continue;
        }
        if (this.discarding || this.body.length === 0) {
          this.discarding = false;
          this.body = [];
          continue;
        }
        const raw = unwrapStreamFrame(this.body);
        this.body = [];
        if (raw) {
          this.inFrame = false;
          this.onFrame(raw);
        }
        continue;
      }
      if (this.inFrame) {
        if (this.discarding) continue;
        if (this.body.length >= MAX_STREAM_FRAME_BYTES) {
          this.discarding = true;
          this.body = [];
          continue;
        }
        this.body.push(b);
        continue;
      }
      this.text.push(b);
      if (b === 0x0a) this.flushText();
    }
  }

  flushText() {
    if (this.text.length === 0) return;
    const text = Uint8Array.from(this.text);
    this.text = [];
    this.onText(text);
  }
}

/// The first value under `key`, searching a parsed frame a few levels deep
/// (the hello's `auth` and a `loginResult` sit inside the message body).
function findKey(value, key, depth = 0) {
  if (!value || typeof value !== "object" || depth > 4) return null;
  if (Object.prototype.hasOwnProperty.call(value, key)) return value[key];
  for (const child of Object.values(value)) {
    const found = findKey(child, key, depth + 1);
    if (found) return found;
  }
  return null;
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
