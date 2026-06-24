const sessions = new Map();
let nextSessionId = 1;
let controllerModulePromise = null;

export function isSupported() {
  return Boolean(globalThis.navigator?.serial);
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
  sessions.delete(id);
}

export async function releasePort(id) {
  const session = sessions.get(id);
  if (!session) {
    return;
  }
  await session.releaseProtocol();
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
