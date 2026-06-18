const sessions = new Map();
let nextSessionId = 1;

export function installLightPlayerBrowserSerial() {
  globalThis.lpBrowserSerialIsSupported = isSupported;
  globalThis.lpBrowserSerialRequestPort = requestPort;
  globalThis.lpBrowserSerialOpen = openPort;
  globalThis.lpBrowserSerialWriteLine = writeLine;
  globalThis.lpBrowserSerialTakeLines = takeLines;
  globalThis.lpBrowserSerialTakeErrors = takeErrors;
  globalThis.lpBrowserSerialClose = closePort;
}

function isSupported() {
  return Boolean(globalThis.navigator?.serial);
}

async function requestPort() {
  if (!isSupported()) {
    throw new Error("Web Serial is not supported in this browser.");
  }
  const port = await navigator.serial.requestPort();
  const id = nextSessionId++;
  sessions.set(id, {
    port,
    reader: null,
    writer: null,
    decoder: new TextDecoder(),
    encoder: new TextEncoder(),
    buffer: "",
    lines: [],
    errors: [],
    closed: false,
  });
  return { id, label: labelForPort(port) };
}

async function openPort(id, baudRate) {
  const session = requireSession(id);
  await session.port.open({ baudRate });
  session.reader = session.port.readable.getReader();
  session.writer = session.port.writable.getWriter();
  session.closed = false;
  readPump(id, session);
}

async function writeLine(id, line) {
  const session = requireSession(id);
  if (!session.writer) {
    throw new Error("Serial port is not open.");
  }
  await session.writer.write(session.encoder.encode(line));
}

function takeLines(id) {
  const session = requireSession(id);
  return session.lines.splice(0, session.lines.length);
}

function takeErrors(id) {
  const session = requireSession(id);
  return session.errors.splice(0, session.errors.length);
}

async function closePort(id) {
  const session = sessions.get(id);
  if (!session) {
    return;
  }
  session.closed = true;
  try {
    await session.reader?.cancel();
  } catch (error) {
    session.errors.push(errorMessage(error));
  }
  try {
    session.reader?.releaseLock();
  } catch (error) {
    session.errors.push(errorMessage(error));
  }
  try {
    await session.writer?.close();
  } catch (error) {
    session.errors.push(errorMessage(error));
  }
  try {
    session.writer?.releaseLock();
  } catch (error) {
    session.errors.push(errorMessage(error));
  }
  try {
    await session.port.close();
  } catch (error) {
    session.errors.push(errorMessage(error));
  }
  sessions.delete(id);
}

async function readPump(id, session) {
  try {
    while (!session.closed) {
      const { value, done } = await session.reader.read();
      if (done) {
        break;
      }
      if (!value) {
        continue;
      }
      session.buffer += session.decoder.decode(value, { stream: true });
      drainCompleteLines(session);
    }
  } catch (error) {
    if (!session.closed) {
      session.errors.push(errorMessage(error));
    }
  } finally {
    const wasClosed = session.closed;
    session.closed = true;
    if (!wasClosed && sessions.get(id) === session) {
      session.errors.push("Serial port disconnected.");
    }
  }
}

function drainCompleteLines(session) {
  for (;;) {
    const newline = session.buffer.indexOf("\n");
    if (newline < 0) {
      return;
    }
    const line = session.buffer.slice(0, newline).replace(/\r$/, "");
    session.buffer = session.buffer.slice(newline + 1);
    session.lines.push(line);
  }
}

function labelForPort(port) {
  const info = port.getInfo?.() ?? {};
  const vendor = numberToHex(info.usbVendorId);
  const product = numberToHex(info.usbProductId);
  if (vendor && product) {
    return `ESP32 Serial (${vendor}:${product})`;
  }
  return "Browser serial device";
}

function numberToHex(value) {
  if (typeof value !== "number") {
    return null;
  }
  return value.toString(16).padStart(4, "0");
}

function requireSession(id) {
  const session = sessions.get(id);
  if (!session) {
    throw new Error(`Unknown browser serial session: ${id}`);
  }
  return session;
}

function errorMessage(error) {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}
