// Studio's Web Bluetooth link: lp-link over a Nordic UART (NUS) GATT
// service, owned here the way `browser_serial.js` owns a Web Serial port.
//
// Since `WIRE_PROTO_VERSION` 36 a board's Bluetooth links run lp-link, as
// its USB link does (plan `lp2025/2026-09-28-1445-ble-on-lp-link`): the
// DATAGRAM framing, where one GATT value is one whole frame (a 4-byte
// header, up to one payload, a 4-byte CRC-32C). This file does not read
// frames, but it keeps their boundaries: every TX notification is handed to
// Rust as it came (`takeFrames`), and every `write` is one frame from Rust,
// written as one GATT value — never cut, never joined. The link itself (the
// handshake, the checksum, resends, the session) is the Rust side's
// (`ble_link_port.rs`, one `LinkPortService` per session), and so is the
// payload size: the board's SYN carries it (`min(180, ATT MTU − 11)`), and
// lp-link cuts the host's frames to it, so this file never needs the MTU
// Web Bluetooth will not tell it.
//
// THE GATT SUBSET, in full — `?ble=emu`'s polyfill
// (`lpa-studio-web/public/lpa-link/virtual_bluetooth.js`) implements exactly
// this and nothing more, and the conformance suite
// (`lpa-link/tests/browser_ble_conformance.rs`) pins it:
//
//   navigator.bluetooth.{requestDevice, getDevices, getAvailability}
//   device.{id, name, gatt, forget, addEventListener("gattserverdisconnected")}
//   gatt.{connected, connect, disconnect}
//   server.getPrimaryService(NUS) → service.getCharacteristic(RX | TX)
//   rx.writeValueWithResponse(bytes)
//   tx.{startNotifications, addEventListener("characteristicvaluechanged"), value}
//
// FIVE RULES, each with a measurement behind it (M2 desk sitting, spike
// Runs B and F):
//
//  1. **Every write is awaited, and every write asks for a response.** One
//     at a time, in order (`session.writeChain`): Web Bluetooth refuses a
//     second GATT operation while one is in flight, and unpaced
//     write-WITHOUT-response lost about two thirds of the bytes in Run B, so
//     this file never issues one. `writesPending` tells the Rust side how
//     many frames are queued here, so it hands over a frame only when there
//     is room and keeps the rest in the link, where a resend is still a
//     choice rather than a duplicate queued behind the original.
//  2. **One frame is one write, and no write is an ATT long write.** A frame
//     is at most the board's payload + 8 B, and the board sizes its payload
//     so that fits one ATT value at the connection's MTU (174 B payload on
//     iOS's 185, 180 B on the 247 macOS and the board agree). The 180 B
//     chunker this rule used to be is gone with the `M!` lines it cut, and so
//     is every Prepare … Execute write: the board now refuses one
//     (docs/defects/2026-09-25-a-long-bluetooth-write-is-acknowledged-and-lost.md
//     is the bug that class of write was).
//  3. **Every `gatt.connect()` is bounded (10 s).** Chrome's hung forever
//     once in Run B and wedged the page with it.
//  4. **A hidden page does not hear its link drop.** iOS suspends a hidden
//     web view: Bluefy queues `gattserverdisconnected` until the page runs
//     again (docs/defects/2026-09-23-bluefy-hidden-page-does-not-see-ble-drops.md).
//     So becoming visible is "state unknown": every session re-reads
//     `gatt.connected`, a dead one is handled as a drop right then, and a
//     session that was reconnecting starts again at once.
//  5. **A drop is torn down, whatever reported it.** Bluefy can tell the
//     page its link is gone while iOS keeps the radio connection up (G4,
//     2026-09-25: the board held the link for 20 minutes, until the tab
//     closed). A reconnect then rides the OLD link, so the board never sees
//     a new one and never starts the new session it owes a new link. Every
//     drop therefore calls `gatt.disconnect()`, and so does a failed write:
//     lp-link would resend a frame a write lost, but a write that fails on a
//     link the page still calls up is how that phantom link shows itself.
//     The reconnect is always a fresh connection, so a fresh lp-link session
//     (each connection's `generation` is a new link on the Rust side).
//
// THE BOARD'S SESSION STARTS AT THE SUBSCRIBE. The board makes its end of
// the link when the central enables TX notifications, and drops RX writes
// that arrive before that; so `startNotifications()` comes before the
// session counts as connected, and Rust writes nothing until it does.

// RECONNECT NEEDS NO GESTURE (G1, Run F): Bluefy reconnected a held
// `BluetoothDevice` after a page-caused drop in 954 ms and after a board
// reboot in 803 ms, and `getDevices()` returns the granted board. So a drop
// starts a bounded reconnect loop on the held device, and a page load
// re-connects what `getDevices()` returns. Both announce the result the way
// Web Serial's hotplug does — a `connect`/`disconnect` edge with no payload —
// and Studio's device layer re-derives from `presentDevices()`.

const NUS_SERVICE = "6e400001-b5a3-f393-e0a9-e50e24dcca9e";
const NUS_RX = "6e400002-b5a3-f393-e0a9-e50e24dcca9e"; // central → board (write)
const NUS_TX = "6e400003-b5a3-f393-e0a9-e50e24dcca9e"; // board → central (notify)

/// Rule 3.
export const CONNECT_TIMEOUT_MS = 10_000;
/// Bytes of notified frames a session holds for a Rust side that is not
/// draining. Its link drains on every notification (`onActivity`) and on a
/// timer, so this only bounds a page that stopped running Rust at all.
const MAX_BUFFERED_BYTES = 256 * 1024;
/// The reconnect loop's delays, in order. A drop costs one fast retry
/// (Bluefy's measured reconnects were under a second), then backs off; after
/// the last one the session PARKS until the page is shown again, the user
/// reconnects, or the board is picked again.
const RECONNECT_DELAYS_MS = [250, 1_000, 2_000, 4_000, 8_000, 15_000, 30_000, 30_000];
/// A page load's re-connect of `getDevices()` boards is one quiet attempt:
/// a board that is not in range at load is not worth a background loop.
const RESTORE_ATTEMPTS = 1;

const sessions = new Map();
let nextSessionId = 1;
let restored = false;
let visibilityInstalled = false;
const presence = { onConnect: null, onDisconnect: null };

// --- availability (M5 S4) --------------------------------------------------

export function isSupported() {
  return Boolean(globalThis.navigator?.bluetooth?.requestDevice);
}

/// What the Add verb says when Bluetooth is not usable here. `supported` is
/// whether `navigator.bluetooth` exists at all; `available` is
/// `getAvailability()` (null when it cannot be asked); `browser` names the
/// one case the copy has specific words for.
export async function availability() {
  const bluetooth = globalThis.navigator?.bluetooth;
  const browser = browserFamily();
  if (!bluetooth?.requestDevice) {
    return { supported: false, available: null, browser };
  }
  let available = null;
  if (typeof bluetooth.getAvailability === "function") {
    try {
      available = Boolean(await bluetooth.getAvailability());
    } catch {
      available = null;
    }
  }
  return { supported: true, available, browser };
}

function browserFamily() {
  const nav = globalThis.navigator ?? {};
  // Brave hides Web Bluetooth behind a flag and says who it is here.
  if (nav.brave && typeof nav.brave.isBrave === "function") {
    return "brave";
  }
  const ua = String(nav.userAgent ?? "");
  // Every browser on iOS is WebKit, and WebKit ships no Web Bluetooth;
  // Bluefy is the exception because it brings its own. iPadOS reports a Mac.
  const ios =
    /iPhone|iPad|iPod/.test(ua) ||
    (nav.platform === "MacIntel" && Number(nav.maxTouchPoints ?? 0) > 1);
  if (ios) {
    return "ios";
  }
  if (/Firefox\//.test(ua)) {
    return "firefox";
  }
  if (/Safari\//.test(ua) && !/Chrome\/|Chromium\//.test(ua)) {
    return "safari";
  }
  return "other";
}

// --- presence: the hotplug edges ------------------------------------------

/// Install the `connect`/`disconnect` edge callbacks, once. Each is called
/// with no argument, exactly like Web Serial's, and means "re-derive": a
/// session became present (connected), or stopped being present (dropped).
export function installBleEvents(onConnect, onDisconnect) {
  if (presence.onConnect) {
    return false;
  }
  presence.onConnect = onConnect;
  presence.onDisconnect = onDisconnect;
  installVisibility();
  return true;
}

function announce(edge) {
  const callback = edge === "connect" ? presence.onConnect : presence.onDisconnect;
  try {
    callback?.();
  } catch (error) {
    console.warn(`[ble] ${edge} edge handler threw:`, error);
  }
}

// Rule 4. Installed with the edges (and on first use), once per page.
function installVisibility() {
  if (visibilityInstalled || typeof globalThis.document?.addEventListener !== "function") {
    return;
  }
  visibilityInstalled = true;
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") {
      recheckAll("the page was shown again");
    }
  });
}

/// Re-read every session's link: the page may have slept through a drop.
/// Exported so the conformance suite can drive it without a real hide/show.
export function recheckAll(why = "rechecked") {
  for (const session of sessions.values()) {
    if (session.state === "connected" && !session.device.gatt?.connected) {
      handleDrop(session, `dropped while ${why === "the page was shown again" ? "the page was hidden" : why}`);
      continue;
    }
    if ((session.state === "lost" || session.state === "parked") && session.wanted) {
      startReconnect(session, RECONNECT_DELAYS_MS.length);
    }
  }
}

// --- sessions --------------------------------------------------------------

class BleSession {
  constructor(id, device) {
    this.id = id;
    this.device = device;
    this.rx = null;
    this.tx = null;
    // idle → connecting → connected; a drop is `lost` (reconnecting) and
    // then `parked`; the model's own close is `closed`.
    this.state = "idle";
    // Listed by `presentDevices()`: connected, or closed by request (still
    // ours, still reachable — the way a closed serial port is still granted).
    this.present = false;
    // Whether the link is wanted up: a drop reconnects only then.
    this.wanted = false;
    // Notified frames not yet taken, each as it arrived (rule 2).
    this.frames = [];
    this.buffered = 0;
    this.errors = [];
    this.writeChain = Promise.resolve();
    // Frames on the write chain: queued or being written (rule 1).
    this.writesPending = 0;
    // `onActivity` callbacks: something the Rust side should look at now.
    this.listeners = new Set();
    this.generation = 0;
    this.connecting = null;
    this.reconnectTimer = null;
    this.attemptsLeft = 0;
    this.onValue = (event) => this.receive(event);
    this.adopt(device);
  }

  /// Hold `device` and listen for its drops. Events from an object this
  /// session no longer holds are ignored.
  adopt(device) {
    this.device = device;
    device.addEventListener?.("gattserverdisconnected", () => {
      if (this.device === device && this.state === "connected") {
        handleDrop(this, "the board or the radio ended the connection");
      }
    });
  }

  describe() {
    return {
      id: this.id,
      deviceId: String(this.device.id ?? ""),
      name: this.device.name || "Bluetooth device",
      connected: this.state === "connected",
    };
  }

  /// One TX notification: one lp-link frame, kept whole (rule 2).
  receive(event) {
    const view = event?.target?.value ?? event?.value;
    if (!view) {
      return;
    }
    // Copied: the DataView's buffer is the browser's and may be reused.
    const frame = new Uint8Array(view.buffer, view.byteOffset, view.byteLength).slice();
    this.frames.push(frame);
    this.buffered += frame.length;
    while (this.buffered > MAX_BUFFERED_BYTES && this.frames.length > 1) {
      this.buffered -= this.frames.shift().length;
      if (!this.overflowNoted) {
        this.overflowNoted = true;
        this.errors.push("bluetooth receive buffer overflowed; the oldest frames were dropped");
      }
    }
    this.activity();
  }

  /// Forget notified frames: a new connection is a new lp-link session, and
  /// the old one's frames mean nothing to it.
  clearFrames() {
    this.frames = [];
    this.buffered = 0;
  }

  /// Tell the Rust side to look now (a frame came, a write finished, the
  /// link came up or went away). A throwing listener is its own problem.
  activity() {
    for (const listener of [...this.listeners]) {
      try {
        listener();
      } catch (error) {
        console.warn("[ble] activity listener threw:", error);
      }
    }
  }
}

function sessionFor(device) {
  for (const session of sessions.values()) {
    if (session.device === device) {
      return session;
    }
    if (device.id && session.device.id === device.id) {
      // The same device as a NEW object (a re-pick in some browsers, or a
      // polyfill re-installed under the page). A live connection keeps the
      // object it is listening to; an idle session adopts the new one,
      // because the old one may belong to nothing any more.
      if (session.state !== "connected" && session.state !== "connecting") {
        session.adopt(device);
      }
      return session;
    }
  }
  const session = new BleSession(nextSessionId++, device);
  sessions.set(session.id, session);
  installVisibility();
  return session;
}

function requireSession(id) {
  const session = sessions.get(id);
  if (!session) {
    throw new Error(`Unknown bluetooth session: ${id}`);
  }
  return session;
}

// --- the chooser and the grants -------------------------------------------

/// The Bluetooth chooser, then a connect. Resolves the session descriptor;
/// rejects with `NotFoundError` when the user closed the chooser (the Rust
/// side reads that as "cancelled", the way the serial chooser's is).
export async function requestDevice() {
  const bluetooth = globalThis.navigator?.bluetooth;
  if (!bluetooth?.requestDevice) {
    throw new Error("this browser has no Web Bluetooth");
  }
  const device = await bluetooth.requestDevice({
    filters: [{ services: [NUS_SERVICE] }],
    optionalServices: [NUS_SERVICE],
  });
  const session = sessionFor(device);
  await connectSession(session);
  return session.describe();
}

/// Re-connect, once and quietly, every board this origin was ALREADY
/// granted (`getDevices()`), the Bluetooth twin of the serial startup
/// sweep. Idempotent per page. Successes arrive as `connect` edges.
export async function restoreGrantedDevices() {
  if (restored) {
    return 0;
  }
  restored = true;
  const bluetooth = globalThis.navigator?.bluetooth;
  if (typeof bluetooth?.getDevices !== "function") {
    return 0;
  }
  let devices;
  try {
    devices = await bluetooth.getDevices();
  } catch {
    return 0;
  }
  for (const device of devices) {
    const session = sessionFor(device);
    if (session.state === "idle") {
      session.wanted = true;
      startReconnect(session, RESTORE_ATTEMPTS, 0);
    }
  }
  return devices.length;
}

/// The sessions Studio should hold a link for right now: connected, or
/// closed by request. A dropped or parked session is NOT listed — that is
/// how the departure sweep learns the board went away.
export function presentDevices() {
  return [...sessions.values()].filter((session) => session.present).map((s) => s.describe());
}

export function isConnected(id) {
  return sessions.get(id)?.state === "connected";
}

/// Connect (or confirm) a session's link. Bounded (rule 3); concurrent
/// callers share the attempt in flight.
export async function connect(id) {
  const session = requireSession(id);
  session.wanted = true;
  cancelReconnect(session);
  await connectSession(session);
  return true;
}

/// Close the link by request: no reconnect follows, and the session stays
/// listed so the link can be opened again without the chooser.
export async function disconnect(id) {
  const session = sessions.get(id);
  if (!session) {
    return;
  }
  session.wanted = false;
  cancelReconnect(session);
  session.generation += 1;
  session.connecting = null;
  const wasUp = session.state === "connected";
  session.state = "closed";
  session.rx = null;
  detachNotifications(session);
  session.clearFrames();
  session.activity();
  if (wasUp || session.device.gatt?.connected) {
    try {
      session.device.gatt?.disconnect();
    } catch {
      // Already gone; the state above is the answer either way.
    }
  }
}

/// Revoke the browser's permission for a session's device
/// (`BluetoothDevice.forget()`, where it exists). The session is deleted:
/// no flow may reach this device again without the chooser. Answers
/// whether the permission was actually revoked.
export async function forget(id) {
  const session = sessions.get(id);
  if (!session) {
    return false;
  }
  await disconnect(id);
  sessions.delete(id);
  if (session.present) {
    session.present = false;
  }
  if (typeof session.device.forget !== "function") {
    return false;
  }
  try {
    await session.device.forget();
    return true;
  } catch {
    return false;
  }
}

// --- frames ----------------------------------------------------------------

/// Queue ONE lp-link frame for the board: one GATT write, awaited in turn
/// behind whatever is already queued (rules 1 and 2). Returns immediately;
/// `false` when the link is not connected (the frame never left, which the
/// link on the Rust side treats like any lost frame). A failure is reported
/// through `takeErrors`.
export function write(id, frame) {
  const session = requireSession(id);
  if (session.state !== "connected" || !session.rx) {
    session.errors.push("write on a bluetooth link that is not connected");
    return false;
  }
  // A copy, never a view: Bluefy writes a view's whole underlying buffer
  // (2026-10-02), and a frame from Rust is a view onto wasm memory. The
  // copy owns a buffer exactly one frame long.
  const data = frame instanceof Uint8Array ? frame.slice() : new Uint8Array(frame);
  const generation = session.generation;
  const rx = session.rx;
  session.writesPending += 1;
  session.writeChain = session.writeChain.then(async () => {
    try {
      if (session.generation !== generation) {
        return;
      }
      if (typeof rx.writeValueWithResponse === "function") {
        await rx.writeValueWithResponse(data);
      } else {
        await rx.writeValue(data);
      }
    } catch (error) {
      if (session.generation !== generation) {
        return;
      }
      session.errors.push(`bluetooth write failed: ${messageOf(error)}`);
      // Rule 5.
      handleDrop(
        session,
        session.device.gatt?.connected ? "a write failed" : "a write found the connection gone",
      );
    } finally {
      session.writesPending -= 1;
      session.activity();
    }
  });
  return true;
}

/// `{ generation, connected, frames, writesPending }`: every frame the board
/// notified since the last call, each as the one notification it came in
/// (rule 2); which connection they belong to (`generation` moves with every
/// connect, drop and close — a new one is a new lp-link session); whether
/// the link can be written now (connected AND subscribed); and how many
/// frames are still queued on the write chain (rule 1).
export function takeFrames(id) {
  const session = requireSession(id);
  const frames = session.frames;
  session.clearFrames();
  return {
    generation: session.generation,
    connected: session.state === "connected" && session.rx !== null,
    frames,
    writesPending: session.writesPending,
  };
}

/// Call `callback` (no argument) whenever the Rust side should look at the
/// session now: a notification came, a write finished, the link came up or
/// went away. A hidden page throttles timers to a second or worse, but not
/// these events, so the board's frames are still acknowledged promptly.
/// Returns the unsubscribe function.
export function onActivity(id, callback) {
  const session = requireSession(id);
  session.listeners.add(callback);
  return () => session.listeners.delete(callback);
}

export function takeErrors(id) {
  const session = requireSession(id);
  const errors = session.errors;
  session.errors = [];
  return errors;
}

// --- the connection itself -------------------------------------------------

async function connectSession(session) {
  if (session.state === "connected" && session.device.gatt?.connected) {
    return;
  }
  if (session.connecting) {
    return session.connecting;
  }
  session.state = "connecting";
  const generation = ++session.generation;
  const attempt = boundedConnect(session, generation);
  session.connecting = attempt;
  try {
    await attempt;
  } finally {
    if (session.connecting === attempt) {
      session.connecting = null;
    }
  }
}

async function boundedConnect(session, generation) {
  let timer = null;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(
      () => reject(new Error(`bluetooth connect timed out after ${CONNECT_TIMEOUT_MS / 1000} s`)),
      CONNECT_TIMEOUT_MS,
    );
  });
  try {
    await Promise.race([openGatt(session, generation), timeout]);
  } catch (error) {
    if (session.generation === generation) {
      session.state = session.wanted ? "lost" : "idle";
      try {
        // Abandon a connect that may still be pending underneath (rule 3).
        session.device.gatt?.disconnect();
      } catch {
        // nothing to abandon
      }
    }
    throw error;
  } finally {
    clearTimeout(timer);
  }
}

async function openGatt(session, generation) {
  const server = await session.device.gatt.connect();
  const service = await server.getPrimaryService(NUS_SERVICE);
  const rx = await service.getCharacteristic(NUS_RX);
  const tx = await service.getCharacteristic(NUS_TX);
  if (session.generation !== generation) {
    // Superseded while it was in flight (a timeout, a close): not ours.
    return;
  }
  detachNotifications(session);
  // Before the listener: the board starts the new session's SYNs the moment
  // notifications are on, and none of the old connection's frames may
  // precede them.
  session.clearFrames();
  tx.addEventListener("characteristicvaluechanged", session.onValue);
  await tx.startNotifications();
  if (session.generation !== generation) {
    tx.removeEventListener?.("characteristicvaluechanged", session.onValue);
    return;
  }
  session.rx = rx;
  session.tx = tx;
  session.writeChain = Promise.resolve();
  session.overflowNoted = false;
  session.state = "connected";
  session.wanted = true;
  const wasPresent = session.present;
  session.present = true;
  session.activity();
  if (!wasPresent) {
    announce("connect");
  }
}

function detachNotifications(session) {
  session.tx?.removeEventListener?.("characteristicvaluechanged", session.onValue);
  session.tx = null;
}

/// The link died underneath us: say so ONCE (the Rust side reads this
/// exact phrase as a departure), stop being present, and reconnect if the
/// link is wanted.
function handleDrop(session, why) {
  session.generation += 1;
  session.connecting = null;
  session.rx = null;
  detachNotifications(session);
  session.state = "lost";
  // Rule 5: make the radio link as gone as the page thinks it is. The
  // `gattserverdisconnected` this may fire is ignored (not "connected").
  try {
    session.device.gatt?.disconnect();
  } catch {
    // Already gone.
  }
  session.errors.push(`bluetooth link lost: ${why}`);
  session.clearFrames();
  session.activity();
  if (session.present) {
    session.present = false;
    announce("disconnect");
  }
  if (session.wanted) {
    startReconnect(session, RECONNECT_DELAYS_MS.length);
  } else {
    session.state = "parked";
  }
}

function startReconnect(session, attempts, firstDelayMs = null) {
  cancelReconnect(session);
  session.attemptsLeft = attempts;
  schedule(session, firstDelayMs ?? RECONNECT_DELAYS_MS[0]);
}

function schedule(session, delayMs) {
  session.reconnectTimer = setTimeout(async () => {
    session.reconnectTimer = null;
    if (!session.wanted || !sessions.has(session.id)) {
      return;
    }
    session.attemptsLeft -= 1;
    try {
      await connectSession(session);
    } catch (error) {
      if (!session.wanted) {
        return;
      }
      if (session.attemptsLeft <= 0) {
        session.state = "parked";
        return;
      }
      const used = RECONNECT_DELAYS_MS.length - session.attemptsLeft;
      schedule(session, RECONNECT_DELAYS_MS[Math.min(used, RECONNECT_DELAYS_MS.length - 1)]);
    }
  }, delayMs);
}

function cancelReconnect(session) {
  if (session.reconnectTimer !== null) {
    clearTimeout(session.reconnectTimer);
    session.reconnectTimer = null;
  }
}

function messageOf(error) {
  return error?.message ?? String(error);
}
