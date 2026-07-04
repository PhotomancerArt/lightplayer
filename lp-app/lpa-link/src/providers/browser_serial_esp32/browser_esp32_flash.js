import { getPort, releasePort } from "./browser_serial.js";

const ESPTOOL_TRANSPORT_TRACING = false;

export function isSupported() {
  return Boolean(globalThis.navigator?.serial && globalThis.fetch);
}

export async function loadManifest(manifestPath) {
  const manifest = await loadFullManifest(manifestPath);
  return summarizeManifest(manifest, manifestPath);
}

export async function probeTarget(portId, esptoolModulePath) {
  if (!isSupported()) {
    throw new Error("Web Serial ESP32 probing is not supported in this browser.");
  }

  try {
    const port = getPort(portId);
    await releasePort(portId);

    const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
    const logs = [];
    const terminal = terminalFor(logs, "esp32-probe");
    const transport = new Transport(port, ESPTOOL_TRANSPORT_TRACING);
    const loader = new ESPLoader({
      transport,
      baudrate: 115200,
      terminal,
      debugLogging: false,
    });

    try {
      const chipName = await loader.main();
      await loader.after("hard_reset");
      return {
        chipName: chipName ? String(chipName) : null,
        logs,
      };
    } finally {
      try {
        await transport.disconnect();
      } catch (error) {
        console.warn("[esp32-probe] transport disconnect failed", error);
      }
    }
  } catch (error) {
    reportFailure("esp32-probe", error);
    throw error;
  }
}

export async function flashFirmware(portId, manifestPath, esptoolModulePath, onEvent) {
  if (!isSupported()) {
    throw new Error("Web Serial firmware flashing is not supported in this browser.");
  }

  const logs = [];
  const progress = [];
  const terminal = terminalFor(logs, "esp32-flash", onEvent);
  try {
    const manifest = await loadFullManifest(manifestPath);
    const imageFiles = await loadImageFiles(manifest, manifestPath);
    const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
    const port = getPort(portId);
    await releasePort(portId);

    const transport = new Transport(port, ESPTOOL_TRANSPORT_TRACING);
    const loader = new ESPLoader({
      transport,
      baudrate: manifest.flash?.baudRate ?? 115200,
      terminal,
      debugLogging: false,
    });

    try {
      const chipName = await loader.main();
      pushProgress(progress, onEvent, {
        label: "Connected to ESP32 bootloader",
        completedSteps: 1,
        totalSteps: 3,
        percent: 10,
      });
      await loader.writeFlash({
        fileArray: imageFiles.map((image) => ({
          data: image.data,
          address: image.address,
        })),
        flashSize: "keep",
        flashMode: "keep",
        flashFreq: "keep",
        eraseAll: false,
        compress: true,
        reportProgress: (fileIndex, written, total) => {
          const percent = total > 0 ? Math.round((written / total) * 100) : 0;
          pushProgress(progress, onEvent, {
            label: `Writing firmware image ${fileIndex + 1}/${imageFiles.length}`,
            completedSteps: 2,
            totalSteps: 3,
            percent,
          });
        },
      });
      pushProgress(progress, onEvent, {
        label: "Resetting flashed device",
        completedSteps: 3,
        totalSteps: 3,
        percent: 100,
      });
      await loader.after("hard_reset");
      return {
        manifest: summarizeManifest(manifest, manifestPath),
        chipName: chipName ? String(chipName) : null,
        logs,
        progress: compactProgress(progress),
      };
    } finally {
      try {
        await transport.disconnect();
      } catch (error) {
        console.warn("[esp32-flash] transport disconnect failed", error);
      }
    }
  } catch (error) {
    reportFailure("esp32-flash", error, onEvent);
    throw error;
  }
}

export async function eraseDeviceFlash(portId, esptoolModulePath, onEvent) {
  if (!isSupported()) {
    throw new Error("Web Serial device erase is not supported in this browser.");
  }

  const logs = [];
  const progress = [];
  const terminal = terminalFor(logs, "esp32-erase", onEvent);
  try {
    const port = getPort(portId);
    await releasePort(portId);

    const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
    const transport = new Transport(port, ESPTOOL_TRANSPORT_TRACING);
    const loader = new ESPLoader({
      transport,
      baudrate: 115200,
      terminal,
      debugLogging: false,
    });

    try {
      const chipName = await loader.main();
      pushProgress(progress, onEvent, {
        label: "Connected to ESP32 bootloader",
        completedSteps: 1,
        totalSteps: 3,
        percent: 10,
      });
      pushProgress(progress, onEvent, {
        label: "Erasing device flash",
        completedSteps: 2,
        totalSteps: 3,
        percent: 50,
      });
      await loader.eraseFlash();
      assertNoFlashCommunicationWarning(logs, "Device erase");
      pushProgress(progress, onEvent, {
        label: "Device flash erased",
        completedSteps: 3,
        totalSteps: 3,
        percent: 100,
      });
      return {
        chipName: chipName ? String(chipName) : null,
        logs,
        progress: compactProgress(progress),
      };
    } finally {
      try {
        await transport.disconnect();
      } catch (error) {
        console.warn("[esp32-erase] transport disconnect failed", error);
      }
    }
  } catch (error) {
    reportFailure("esp32-erase", error, onEvent);
    throw error;
  }
}

function assertNoFlashCommunicationWarning(logs, context) {
  const warning = logs.find((line) =>
    line.includes("Failed to communicate with the flash chip") ||
    line.includes("Flash ID: 0")
  );
  if (warning) {
    throw new Error(`${context} failed: ${warning}`);
  }
}

function terminalFor(logs, target, onEvent) {
  return {
    clean() {},
    writeLine(line) {
      const message = String(line ?? "");
      logs.push(message);
      emitEvent(onEvent, { kind: "log", message });
      console.info(`[${target}] ${message}`);
    },
    write(text) {
      const message = String(text ?? "").trimEnd();
      if (message.length > 0) {
        logs.push(message);
        emitEvent(onEvent, { kind: "log", message });
        console.info(`[${target}] ${message}`);
      }
    },
  };
}

function pushProgress(progress, onEvent, entry) {
  const normalized = {
    label: String(entry.label ?? ""),
    completedSteps: Number(entry.completedSteps ?? 0),
    totalSteps: entry.totalSteps == null ? null : Number(entry.totalSteps),
    percent: entry.percent == null ? null : Number(entry.percent),
  };
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
  emitEvent(onEvent, { kind: "progress", ...normalized });
}

function emitEvent(onEvent, event) {
  if (typeof onEvent === "function") {
    onEvent(event);
  }
}

async function loadFullManifest(manifestPath) {
  const url = new URL(manifestPath, globalThis.location?.href ?? "http://localhost/");
  const response = await fetch(url, { cache: "no-store" });
  const contentType = response.headers.get("content-type") ?? "";
  const text = await response.text();
  if (!response.ok) {
    throw new Error(
      `Firmware manifest is unavailable at ${url.href} (${response.status} ${response.statusText}, content-type: ${contentType || "unknown"}): ${snippet(text)}`,
    );
  }
  if (looksLikeHtml(contentType, text)) {
    throw new Error(
      `Firmware manifest URL returned HTML instead of JSON: ${url.href} (content-type: ${contentType || "unknown"}): ${snippet(text)}`,
    );
  }
  let manifest;
  try {
    manifest = JSON.parse(text);
  } catch (error) {
    throw new Error(
      `Firmware manifest is not valid JSON at ${url.href} (content-type: ${contentType || "unknown"}): ${errorMessage(error)}; body: ${snippet(text)}`,
    );
  }
  validateManifest(manifest);
  return manifest;
}

async function loadImageFiles(manifest, manifestPath) {
  const basePath = new URL(manifestPath, globalThis.location?.href ?? "http://localhost/");
  return Promise.all(
    manifest.images.map(async (image) => {
      const url = new URL(image.path, basePath);
      const response = await fetch(url, { cache: "no-store" });
      const contentType = response.headers.get("content-type") ?? "";
      if (!response.ok) {
        throw new Error(
          `Firmware image is unavailable at ${url.href} (${response.status} ${response.statusText}, content-type: ${contentType || "unknown"}).`,
        );
      }
      if (contentType.includes("text/html")) {
        const text = await response.text();
        throw new Error(
          `Firmware image URL returned HTML instead of binary data: ${url.href}: ${snippet(text)}`,
        );
      }
      return {
        address: parseAddress(image.address),
        data: new Uint8Array(await response.arrayBuffer()),
      };
    }),
  );
}

async function loadEsptoolModule(esptoolModulePath) {
  if (!esptoolModulePath) {
    throw new Error("Missing esptool_module_path.");
  }
  try {
    return await import(esptoolModulePath);
  } catch (error) {
    throw new Error(`Failed to import esptool module ${esptoolModulePath}: ${errorMessage(error)}`);
  }
}

function summarizeManifest(manifest, manifestPath) {
  return {
    firmwareId: String(manifest.firmwareId),
    displayName: String(manifest.displayName ?? manifest.firmwareId),
    targetChip: String(manifest.target?.chip ?? "esp32c6"),
    imageCount: manifest.images.length,
    totalBytes: manifest.images.reduce((total, image) => total + Number(image.sizeBytes ?? 0), 0),
    manifestPath,
  };
}

function compactProgress(progress) {
  const compacted = [];
  let previousKey = null;
  for (const entry of progress) {
    const key = `${entry.label}:${entry.percent}`;
    if (key === previousKey) {
      continue;
    }
    previousKey = key;
    compacted.push(entry);
  }
  return compacted;
}

function validateManifest(manifest) {
  if (!manifest || typeof manifest !== "object") {
    throw new Error("Firmware manifest is not a JSON object.");
  }
  if (typeof manifest.firmwareId !== "string") {
    throw new Error("Firmware manifest is missing firmwareId.");
  }
  if (!Array.isArray(manifest.images) || manifest.images.length === 0) {
    throw new Error("Firmware manifest does not list any flash images.");
  }
  for (const image of manifest.images) {
    if (typeof image.path !== "string" || typeof image.address !== "string") {
      throw new Error("Firmware manifest image entries must include path and address.");
    }
  }
}

function parseAddress(address) {
  const value = Number(address);
  if (!Number.isInteger(value)) {
    throw new Error(`Firmware image address is invalid: ${address}`);
  }
  return value;
}

function reportFailure(target, error, onEvent = null) {
  const message = `${errorMessage(error)}${error?.stack ? `\n${error.stack}` : ""}`;
  emitEvent(onEvent, { kind: "log", message });
  console.error(`[${target}] ${message}`, error);
}

function errorMessage(error) {
  return error instanceof Error ? error.message : String(error ?? "unknown error");
}

function looksLikeHtml(contentType, text) {
  return contentType.includes("text/html") || text.trimStart().startsWith("<!DOCTYPE") || text.trimStart().startsWith("<html");
}

function snippet(text, limit = 240) {
  return String(text ?? "")
    .replace(/\s+/g, " ")
    .trim()
    .slice(0, limit);
}
