import { getPort, releasePort } from "./browser_serial.js";

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

  const port = getPort(portId);
  await releasePort(portId);

  const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
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

export async function flashFirmware(portId, manifestPath, esptoolModulePath) {
  if (!isSupported()) {
    throw new Error("Web Serial firmware flashing is not supported in this browser.");
  }

  const port = getPort(portId);
  await releasePort(portId);

  const manifest = await loadFullManifest(manifestPath);
  const imageFiles = await loadImageFiles(manifest, manifestPath);
  const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
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
}

export async function eraseDeviceFlash(portId, esptoolModulePath) {
  if (!isSupported()) {
    throw new Error("Web Serial device erase is not supported in this browser.");
  }

  const port = getPort(portId);
  await releasePort(portId);

  const { ESPLoader, Transport } = await loadEsptoolModule(esptoolModulePath);
  const logs = [];
  const progress = [];
  const terminal = terminalFor(logs, "esp32-erase");
  const transport = new Transport(port, true);
  const loader = new ESPLoader({
    transport,
    baudrate: 115200,
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
    progress.push({
      label: "Erasing device flash",
      completedSteps: 2,
      totalSteps: 3,
      percent: 50,
    });
    await loader.eraseFlash();
    progress.push({
      label: "Resetting blank device",
      completedSteps: 3,
      totalSteps: 3,
      percent: 100,
    });
    await loader.after("hard_reset");
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

async function loadFullManifest(manifestPath) {
  const response = await fetch(manifestPath, { cache: "no-store" });
  if (!response.ok) {
    throw new Error(`Firmware manifest is unavailable (${response.status} ${response.statusText}).`);
  }
  const manifest = await response.json();
  validateManifest(manifest);
  return manifest;
}

async function loadImageFiles(manifest, manifestPath) {
  const basePath = new URL(manifestPath, globalThis.location?.href ?? "http://localhost/");
  return Promise.all(
    manifest.images.map(async (image) => {
      const response = await fetch(new URL(image.path, basePath), { cache: "no-store" });
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

async function loadEsptoolModule(esptoolModulePath) {
  if (!esptoolModulePath) {
    throw new Error("Missing esptool_module_path.");
  }
  return import(esptoolModulePath);
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
