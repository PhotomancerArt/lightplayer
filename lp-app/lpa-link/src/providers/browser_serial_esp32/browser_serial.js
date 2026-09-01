const sessions = new Map();
let nextSessionId = 1;
let controllerModulePromise = null;

export function isSupported() {
  return Boolean(globalThis.navigator?.serial);
}

// M6 (D32): hotplug plumbing. `navigator.serial` fires `connect` when a
// granted port (re)appears and `disconnect` when one leaves; Studio's
// auto-connect sweep and Gone handling ride these. Installed at most
// once; returns whether listeners are live.
let serialEventsInstalled = false;
export function installSerialEvents(onConnect, onDisconnect) {
  const serial = globalThis.navigator?.serial;
  if (!serial?.addEventListener || serialEventsInstalled) {
    return serialEventsInstalled;
  }
  serialEventsInstalled = true;
  serial.addEventListener("connect", () => onConnect());
  serial.addEventListener("disconnect", () => onDisconnect());
  return true;
}

// Enumerate the ports this origin was ALREADY granted (no permission
// prompt), registering each one as a session so the returned {id, label}
// descriptors are openable without a chooser. Repeat calls return the same
// ids: navigator.serial.getPorts() yields stable SerialPort object
// identities, and existing sessions are matched by port identity.
export async function getGrantedPorts() {
  const serial = globalThis.navigator?.serial;
  if (!serial?.getPorts) {
    return [];
  }
  let ports;
  try {
    ports = await serial.getPorts();
  } catch {
    return [];
  }
  if (ports.length === 0) {
    return [];
  }
  const { BrowserEsp32DeviceController } = await loadControllerModule();
  await adoptReenumeratedPorts(ports);
  return ports.map((port) => sessionForPort(BrowserEsp32DeviceController, port, undefined));
}

// Chrome mints a NEW SerialPort object when a granted device re-enumerates
// (a physical replug, or a USB-Serial-JTAG chip resetting), so matching by
// object identity alone would mint a second session — and downstream a
// second endpoint and another "new device found" card — for the same grant
// (G1 finding, 2026-08-31: a replugged C6 wallpapered the gallery). A
// session whose port is no longer enumerated is a dead generation: adopt
// the new port into it when the USB identity (vid:pid) matches, pairing in
// order — the round-1 single-board case; multi-board disambiguation stays
// with the hello-identity merge.
async function adoptReenumeratedPorts(ports) {
  const live = new Set(ports);
  const dead = [...sessions.values()].filter((session) => !live.has(session.port));
  if (dead.length === 0) {
    return;
  }
  const known = new Set([...sessions.values()].map((session) => session.port));
  for (const port of ports) {
    if (known.has(port)) {
      continue;
    }
    const info = port.getInfo?.() ?? {};
    const index = dead.findIndex((session) => {
      const old = session.port?.getInfo?.() ?? {};
      return (
        old.usbVendorId === info.usbVendorId && old.usbProductId === info.usbProductId
      );
    });
    if (index === -1) {
      continue;
    }
    const [session] = dead.splice(index, 1);
    await session.adoptPort(port);
  }
}

// The ONE session-per-SerialPort rule, shared by the chooser and the
// granted-ports paths so they cannot drift (they did: requestPort used to
// mint a second controller over an already-registered port, and two
// controllers contending for one port's reader/writer is a concrete
// disconnect mechanism — the multi-board L1 defect).
function sessionForPort(BrowserEsp32DeviceController, port, label) {
  for (const [id, session] of sessions) {
    if (session.port === port) {
      return describeSession(id, session);
    }
  }
  const id = nextSessionId++;
  const session = new BrowserEsp32DeviceController({ port, label });
  sessions.set(id, session);
  return describeSession(id, session);
}

// The descriptor handed to Rust. VID:PID travel as their own fields (D7,
// grant-aware port picking): the label bakes them into prose, and prose is
// not something a board's usb_bridge can be matched against.
function describeSession(id, session) {
  const info = session.port?.getInfo?.() ?? {};
  return {
    id,
    label: session.label,
    usbVendorId: info.usbVendorId ?? null,
    usbProductId: info.usbProductId ?? null,
  };
}

export async function requestPort() {
  const { BrowserEsp32DeviceController } = await loadControllerModule();
  const { port, label } = await BrowserEsp32DeviceController.requestPort();
  return sessionForPort(BrowserEsp32DeviceController, port, label);
}

// `resetKind` names one of the controller's `runReset` sequences
// ("normal" | "rts-only" | "usb-jtag-download" | "both-then-drop"); it is
// ignored when `reset` is false. The Rust side owns the naming
// (`browser_serial.rs`'s reset_kind_js_name).
export async function openPort(id, baudRate, reset = true, resetKind = "normal") {
  return requireSession(id).openProtocol({ baudRate, reset, resetKind });
}

export async function writeLine(id, line) {
  await requireSession(id).writeLine(line);
}

export function takeLines(id) {
  return requireSession(id).takeLines();
}

export function takeErrors(id) {
  return requireSession(id).takeErrors();
}

export async function closePort(id) {
  const session = sessions.get(id);
  if (!session) {
    return;
  }
  await session.close();
  // The entry STAYS: the SerialPort is a persistent grant handle, and the
  // management flow closes the link session then flashes through the same
  // id (`getPort`) — deleting here orphaned that port ("Unknown browser
  // serial session"). Keeping entries also keeps ids stable per port
  // identity, which `getGrantedPorts` dedupe relies on. `close()` above
  // released the reader/writer, so no stream stays held.
}

// Revoke the persistent grant behind a session's port
// (`SerialPort.forget()`, Chrome 103+). Unlike `closePort`, the session
// entry is deleted on purpose: the grant no longer exists, so no flow may
// reopen through this id. Returns whether the grant was actually revoked —
// `false` means it survives (unknown id, or a browser without `forget()`).
export async function forgetPort(id) {
  const session = sessions.get(id);
  if (!session) {
    return false;
  }
  sessions.delete(id);
  try {
    // Best-effort stream release: a wedged reader/writer must not keep
    // the grant alive — revoking is the whole point of this call.
    await session.close();
  } catch {
    // fall through to forget()
  }
  if (typeof session.port?.forget !== "function") {
    return false;
  }
  await session.port.forget();
  return true;
}

export async function releasePort(id) {
  const session = sessions.get(id);
  if (!session) {
    return;
  }
  await session.releaseProtocol();
}

export async function resetAndRead(id, baudRate, readWindowMs, resetKind = "normal") {
  return requireSession(id).resetAndRead({
    baudRate,
    readWindowMs,
    resetKind,
  });
}

export async function getPort(id) {
  // Resolve to a LIVE SerialPort, adopting first: a boot-looping native-USB
  // chip re-enumerates every few seconds, so the object this session holds
  // may be a dead generation by the time a consumer (the esptool bridge)
  // asks — its open() then fails instantly with NetworkError (bench, G1
  // 2026-08-31: flashing the blank C6 lost the race on the first try).
  // Adoption is what pairs the dead generation to the replacement, and it
  // previously ran only on hotplug sweeps, never at resolution time.
  const serial = navigator.serial;
  if (serial?.getPorts) {
    try {
      await adoptReenumeratedPorts(await serial.getPorts());
    } catch (error) {
      // Resolution still answers with what the session holds; a failed
      // adoption pass must not mask the real open error downstream.
    }
  }
  return requireSession(id).port;
}

function requireSession(id) {
  const session = sessions.get(id);
  if (!session) {
    throw new Error(`Unknown browser serial session: ${id}`);
  }
  return session;
}

function loadControllerModule() {
  controllerModulePromise ??= import(controllerModulePath());
  return controllerModulePromise;
}

function controllerModulePath() {
  return "/lpa-link/browser_esp32_device_controller.js";
}
