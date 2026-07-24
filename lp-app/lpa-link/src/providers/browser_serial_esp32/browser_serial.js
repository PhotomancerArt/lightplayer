const sessions = new Map();
let nextSessionId = 1;
let controllerModulePromise = null;

export function isSupported() {
  return Boolean(globalThis.navigator?.serial);
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
  return ports.map((port) => {
    for (const [id, session] of sessions) {
      if (session.port === port) {
        return { id, label: session.label };
      }
    }
    const id = nextSessionId++;
    const session = new BrowserEsp32DeviceController({ port });
    sessions.set(id, session);
    return { id, label: session.label };
  });
}

export async function requestPort() {
  const { BrowserEsp32DeviceController } = await loadControllerModule();
  const { port, label } = await BrowserEsp32DeviceController.requestPort();
  const id = nextSessionId++;
  sessions.set(id, new BrowserEsp32DeviceController({ port, label }));
  return { id, label };
}

export async function openPort(id, baudRate) {
  return requireSession(id).openProtocol({ baudRate });
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

export async function releasePort(id) {
  const session = sessions.get(id);
  if (!session) {
    return;
  }
  await session.releaseProtocol();
}

export async function resetAndRead(id, baudRate, readWindowMs) {
  return requireSession(id).resetAndRead({
    baudRate,
    readWindowMs,
  });
}

export function getPort(id) {
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
