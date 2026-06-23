export class BrowserEsp32DeviceController {
  constructor({ port, label = null } = {}) {
    if (!port) {
      throw new Error("BrowserEsp32DeviceController requires a SerialPort.");
    }
    this.port = port;
    this.label = label ?? labelForPort(port);
    this.reader = null;
    this.writer = null;
    this.readLoopPromise = null;
    this.readStopRequested = false;
    this.releasing = false;
    this.closed = true;
    this.decoder = new TextDecoder();
    this.encoder = new TextEncoder();
    this.buffer = "";
    this.lines = [];
    this.errors = [];
    this.listeners = new Set();
  }

  static isSupported() {
    return Boolean(globalThis.navigator?.serial);
  }

  static async requestPort() {
    if (!this.isSupported()) {
      throw new Error("Web Serial is not supported in this browser.");
    }
    const port = await navigator.serial.requestPort();
    return { port, label: labelForPort(port) };
  }

  subscribe(listener) {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  async openProtocol({ baudRate = 115200, reset = true } = {}) {
    const logs = [];
    const progress = [];
    const log = (source, text) => {
      const message = String(text ?? "");
      logs.push(message);
      this.emit({ type: "log", source, text: message });
    };
    const pushProgress = (entry) => {
      const normalized = normalizeProgress(entry);
      const previous = progress.at(-1);
      if (
        previous &&
        previous.label === normalized.label &&
        previous.completedSteps === normalized.completedSteps &&
        previous.totalSteps === normalized.totalSteps &&
        previous.percent === normalized.percent
      ) {
        return;
      }
      progress.push(normalized);
      this.emit({ type: "progress", ...normalized });
    };

    this.clearBufferedInput();
    await this.releaseProtocol({ collectErrors: false });
    pushProgress({
      label: "Opening serial port",
      completedSteps: 1,
      totalSteps: reset ? 3 : 2,
      percent: reset ? 20 : 50,
    });
    await this.openForRead({ baudRate, clear: false, log });
    pushProgress({
      label: "Reading serial output",
      completedSteps: 2,
      totalSteps: reset ? 3 : 2,
      percent: reset ? 40 : 100,
    });

    let resetOutcome = { ok: false, skipped: true, message: "reset skipped" };
    if (reset) {
      pushProgress({
        label: "Resetting device",
        completedSteps: 2,
        totalSteps: 3,
        percent: 60,
      });
      resetOutcome = await this.tryNormalReset({ log });
      pushProgress({
        label: "Waiting for device boot",
        completedSteps: 3,
        totalSteps: 3,
        percent: 100,
      });
    }

    return { logs, progress, resetOutcome };
  }

  async openForRead({ baudRate = 115200, clear = true, log = null } = {}) {
    if (clear) {
      this.clearBufferedInput();
    }
    await this.openPortWithRetry(baudRate, log);
    this.writer = this.port.writable.getWriter();
    this.closed = false;
    this.releasing = false;
    this.startReadPump();
    return { label: this.label };
  }

  async resetAndRead({ baudRate = 115200, readWindowMs = 5000, resetKind = "normal" } = {}) {
    const logs = [];
    const log = (source, text) => {
      const message = String(text ?? "");
      logs.push(message);
      this.emit({ type: "log", source, text: message });
    };
    this.clearBufferedInput();
    await this.releaseProtocol({ collectErrors: false });
    await this.openForRead({ baudRate, clear: false, log });
    const resetOutcome = await this.runReset(resetKind, { log });
    if (readWindowMs > 0) {
      await sleep(readWindowMs);
      await this.stopReadPump();
      log("lpa-link", `read window complete (${readWindowMs}ms)`);
    }
    return { logs, resetOutcome };
  }

  async readFor({ baudRate = 115200, readWindowMs = 5000 } = {}) {
    const logs = [];
    const log = (source, text) => {
      const message = String(text ?? "");
      logs.push(message);
      this.emit({ type: "log", source, text: message });
    };
    this.clearBufferedInput();
    await this.releaseProtocol({ collectErrors: false });
    await this.openForRead({ baudRate, clear: false, log });
    if (readWindowMs > 0) {
      await sleep(readWindowMs);
      await this.stopReadPump();
      log("lpa-link", `read window complete (${readWindowMs}ms)`);
    }
    return { logs };
  }

  async writeLine(line) {
    if (!this.writer) {
      throw new Error("Serial port is not open.");
    }
    await this.writer.write(this.encoder.encode(line));
  }

  takeLines() {
    return this.lines.splice(0, this.lines.length);
  }

  takeErrors() {
    return this.errors.splice(0, this.errors.length);
  }

  async releaseProtocol({ collectErrors = true } = {}) {
    this.releasing = true;
    this.closed = true;
    await this.stopReadPump({ collectErrors });
    await safeCloseWriter(this, collectErrors);
    await safeClosePort(this, collectErrors);
    this.reader = null;
    this.writer = null;
    this.releasing = false;
  }

  async close() {
    await this.releaseProtocol();
  }

  async stopReadPump({ collectErrors = true } = {}) {
    this.readStopRequested = true;
    const activeReader = this.reader;
    if (!activeReader) {
      return;
    }
    try {
      await activeReader.cancel();
    } catch (error) {
      if (collectErrors) {
        this.pushError(errorMessage(error));
      }
    }
    try {
      await this.readLoopPromise;
    } finally {
      this.readLoopPromise = null;
      this.readStopRequested = false;
    }
  }

  async setDTR(value) {
    await this.setSignals({ dataTerminalReady: Boolean(value) });
  }

  async setRTS(value) {
    await this.setSignals({ requestToSend: Boolean(value) });
  }

  async snapshotSignals() {
    if (typeof this.port.getSignals !== "function") {
      throw new Error("getSignals() is not available for this port.");
    }
    return this.port.getSignals();
  }

  async runSignalSequence(sequence, { log = null } = {}) {
    const commands = String(sequence ?? "")
      .split(/[\s|,]+/)
      .map((command) => command.trim())
      .filter(Boolean);
    for (const command of commands) {
      const op = command[0]?.toUpperCase();
      const arg = command.slice(1);
      if (op === "D") {
        await this.setDTR(parseBinaryArg(command, arg));
        log?.("lpa-link", `DTR=${arg}`);
      } else if (op === "R") {
        await this.setRTS(parseBinaryArg(command, arg));
        log?.("lpa-link", `RTS=${arg}`);
      } else if (op === "W") {
        const ms = Number(arg);
        if (!Number.isFinite(ms) || ms < 0) {
          throw new Error(`invalid wait command: ${command}`);
        }
        log?.("lpa-link", `wait ${ms}ms`);
        await sleep(ms);
      } else {
        throw new Error(`unknown sequence command: ${command}`);
      }
    }
  }

  async tryNormalReset({ log = null } = {}) {
    return this.runReset("normal", { log });
  }

  async runReset(resetKind, { log = null } = {}) {
    try {
      if (resetKind === "usb-jtag-download") {
        log?.("lpa-link", "USB-JTAG download reset: R0 D0 W100 D1 R0 W100 R1 D0 R1 W100 R0 D0");
        await this.runSignalSequence("R0 D0 W100 D1 R0 W100 R1 D0 R1 W100 R0 D0", { log });
      } else if (resetKind === "rts-only") {
        log?.("lpa-link", "RTS-only reset: R1 W100 R0");
        await this.runSignalSequence("R1 W100 R0", { log });
      } else {
        log?.("lpa-link", "Hard resetting via RTS pin...");
        await this.runSignalSequence("D0 W100 R1 W100 R0", { log });
      }
      return { ok: true, skipped: false, message: "reset complete" };
    } catch (error) {
      const message = `Reset signal control failed: ${errorMessage(error)}`;
      log?.("lpa-link", message);
      log?.("lpa-link", "Continuing without a hardware reset.");
      return { ok: false, skipped: false, message };
    }
  }

  isOpen() {
    return Boolean(this.port?.readable || this.port?.writable);
  }

  clearBufferedInput() {
    this.buffer = "";
    this.lines = [];
    this.errors = [];
  }

  emit(event) {
    for (const listener of this.listeners) {
      try {
        listener(event);
      } catch (error) {
        console.warn("[browser-esp32-device] listener failed", error);
      }
    }
  }

  pushError(message) {
    this.errors.push(message);
    this.emit({ type: "error", error: message });
  }

  async openPortWithRetry(baudRate, log) {
    if (this.isOpen()) {
      return;
    }
    try {
      await this.port.open({ baudRate });
      log?.("lpa-link", `Serial port ${this.label}`);
      return;
    } catch (firstError) {
      const firstMessage = errorMessage(firstError);
      log?.("lpa-link", `Serial open failed: ${firstMessage}`);
      await safeClosePort(this, false);
      await sleep(250);
      try {
        await this.port.open({ baudRate });
        log?.("lpa-link", `Serial port ${this.label}`);
      } catch (secondError) {
        throw new Error(`Failed to open serial port: ${errorMessage(secondError)}`);
      }
    }
  }

  startReadPump() {
    if (this.reader) {
      return;
    }
    if (!this.port.readable) {
      throw new Error("Serial port readable stream is unavailable.");
    }
    this.readStopRequested = false;
    this.reader = this.port.readable.getReader();
    this.readLoopPromise = this.readPump(this.reader);
  }

  async readPump(activeReader) {
    try {
      for (;;) {
        const { value, done } = await activeReader.read();
        if (done) {
          break;
        }
        if (!value) {
          continue;
        }
        const text = this.decoder.decode(value, { stream: true });
        this.buffer += text;
        this.emit({ type: "raw", source: "serial", text });
        this.drainCompleteLines();
      }
    } catch (error) {
      if (!this.closed && !this.readStopRequested) {
        this.pushError(errorMessage(error));
      }
    } finally {
      try {
        activeReader.releaseLock();
      } catch {}
      if (this.reader === activeReader) {
        this.reader = null;
      }
      if (!this.closed && !this.releasing && !this.readStopRequested) {
        this.pushError("Serial port disconnected.");
      }
    }
  }

  drainCompleteLines() {
    for (;;) {
      const newline = this.buffer.indexOf("\n");
      if (newline < 0) {
        return;
      }
      const line = this.buffer.slice(0, newline).replace(/\r$/, "");
      this.buffer = this.buffer.slice(newline + 1);
      this.lines.push(line);
      this.emit({ type: "line", source: "serial", text: line });
    }
  }

  async setSignals(signals) {
    if (typeof this.port.setSignals !== "function") {
      throw new Error("Web Serial port does not support DTR/RTS reset signals.");
    }
    await this.port.setSignals(signals);
  }
}

export function labelForPort(port) {
  const info = port.getInfo?.() ?? {};
  const vendor = numberToHex(info.usbVendorId);
  const product = numberToHex(info.usbProductId);
  if (vendor && product) {
    return `ESP32 Serial (${vendor}:${product})`;
  }
  return "Browser serial device";
}

export function errorMessage(error) {
  if (error instanceof Error) {
    return error.message;
  }
  return String(error);
}

export function sleep(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function normalizeProgress(entry) {
  return {
    label: String(entry.label ?? ""),
    completedSteps: Number(entry.completedSteps ?? 0),
    totalSteps: entry.totalSteps == null ? null : Number(entry.totalSteps),
    percent: entry.percent == null ? null : Number(entry.percent),
  };
}

async function safeCloseWriter(controller, collectErrors) {
  const writer = controller.writer;
  if (!writer) {
    return;
  }
  try {
    await writer.close();
  } catch (error) {
    if (collectErrors && !isAlreadyClosedError(error)) {
      controller.pushError(errorMessage(error));
    }
  }
  try {
    writer.releaseLock();
  } catch (error) {
    if (collectErrors && !isAlreadyClosedError(error)) {
      controller.pushError(errorMessage(error));
    }
  }
}

async function safeClosePort(controller, collectErrors) {
  try {
    await controller.port.close();
  } catch (error) {
    if (collectErrors && !isAlreadyClosedError(error)) {
      controller.pushError(errorMessage(error));
    }
  }
}

function isAlreadyClosedError(error) {
  const message = errorMessage(error).toLowerCase();
  return message.includes("already closed") || message.includes("port is already closed");
}

function parseBinaryArg(command, arg) {
  if (arg !== "0" && arg !== "1") {
    throw new Error(`invalid binary command: ${command}`);
  }
  return arg === "1";
}

function numberToHex(value) {
  if (typeof value !== "number") {
    return null;
  }
  return value.toString(16).padStart(4, "0");
}
