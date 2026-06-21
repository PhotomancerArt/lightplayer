const DEFAULT_MANIFEST_URL = "./firmware/esp32c6/manifest.json";
const DEFAULT_ESPTOOL_JS_MODULE_URL = "https://unpkg.com/esptool-js@0.6.0/lib/index.js";

export function installLightPlayerBrowserEsp32Flash() {
  globalThis.lpBrowserEsp32FlashIsSupported = isSupported;
  globalThis.lpBrowserEsp32FlashLoadManifest = loadManifest;
  globalThis.lpBrowserEsp32FlashProbeTarget = probeTarget;
  globalThis.lpBrowserEsp32FlashFirmware = flashFirmware;
}

function isSupported() {
  return Boolean(globalThis.navigator?.serial && globalThis.fetch);
}

async function loadManifest(manifestUrl = DEFAULT_MANIFEST_URL) {
  const manifest = await loadFullManifest(manifestUrl);
  return summarizeManifest(manifest, manifestUrl);
}

async function probeTarget(portId) {
  if (!isSupported()) {
    throw new Error("Web Serial ESP32 probing is not supported in this browser.");
  }

  const port = serialPortFor(portId);
  await globalThis.lpBrowserSerialRelease?.(portId);

  const { ESPLoader, Transport } = await loadEsptoolModule();
  const logs = [];
  const terminal = terminalFor(logs, "esp32-probe");
  const transport = new Transport(port, true);
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
}

async function flashFirmware(portId, manifestUrl = DEFAULT_MANIFEST_URL) {
  if (!isSupported()) {
    throw new Error("Web Serial firmware flashing is not supported in this browser.");
  }

  const port = serialPortFor(portId);
  await globalThis.lpBrowserSerialRelease?.(portId);

  const manifest = await loadFullManifest(manifestUrl);
  const imageFiles = await loadImageFiles(manifest, manifestUrl);
  const { ESPLoader, Transport } = await loadEsptoolModule();
  const logs = [];
  const progress = [];
  const terminal = terminalFor(logs, "esp32-flash");
  const transport = new Transport(port, true);
  const loader = new ESPLoader({
    transport,
    baudrate: manifest.flash?.baudRate ?? 115200,
    terminal,
    debugLogging: false,
  });

  try {
    const chipName = await loader.main();
    progress.push({
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
        progress.push({
          label: `Writing firmware image ${fileIndex + 1}/${imageFiles.length}`,
          completedSteps: 2,
          totalSteps: 3,
          percent,
        });
      },
    });
    progress.push({
      label: "Resetting flashed device",
      completedSteps: 3,
      totalSteps: 3,
      percent: 100,
    });
    await loader.after("hard_reset");
    return {
      manifest: summarizeManifest(manifest, manifestUrl),
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
}

function serialPortFor(portId) {
  const getPort = globalThis.lpBrowserSerialGetPort;
  if (typeof getPort !== "function") {
    throw new Error("Browser serial port access is not installed.");
  }
  const port = getPort(portId);
  if (!port) {
    throw new Error(`No browser serial port exists for session ${portId}.`);
  }
  return port;
}

function terminalFor(logs, target) {
  return {
    clean() {},
    writeLine(line) {
      const message = String(line ?? "");
      logs.push(message);
      console.info(`[${target}] ${message}`);
    },
    write(text) {
      const message = String(text ?? "").trimEnd();
      if (message.length > 0) {
        logs.push(message);
        console.info(`[${target}] ${message}`);
      }
    },
  };
}

async function loadFullManifest(manifestUrl) {
  const response = await fetch(manifestUrl, { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`Firmware manifest is unavailable (${response.status} ${response.statusText}).`);
  }
  const manifest = await response.json();
  validateManifest(manifest);
  return manifest;
}

async function loadImageFiles(manifest, manifestUrl) {
  const baseUrl = new URL(manifestUrl, globalThis.location?.href ?? "http://localhost/");
  return Promise.all(
    manifest.images.map(async (image) => {
      const response = await fetch(new URL(image.path, baseUrl), { cache: "no-store" });
      if (!response.ok) {
        throw new Error(`Firmware image ${image.path} is unavailable (${response.status} ${response.statusText}).`);
      }
      return {
        address: parseAddress(image.address),
        data: new Uint8Array(await response.arrayBuffer()),
      };
    }),
  );
}

async function loadEsptoolModule() {
  const moduleUrl = globalThis.lpEspToolJsModuleUrl ?? DEFAULT_ESPTOOL_JS_MODULE_URL;
  return import(moduleUrl);
}

function summarizeManifest(manifest, manifestUrl) {
  return {
    firmwareId: String(manifest.firmwareId),
    displayName: String(manifest.displayName ?? manifest.firmwareId),
    targetChip: String(manifest.target?.chip ?? "esp32c6"),
    imageCount: manifest.images.length,
    totalBytes: manifest.images.reduce((total, image) => total + Number(image.sizeBytes ?? 0), 0),
    manifestUrl,
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
