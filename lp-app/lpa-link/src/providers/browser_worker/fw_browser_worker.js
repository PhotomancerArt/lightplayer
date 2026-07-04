let runtimeId = null;
let booted = false;
let fwBrowser = null;

// Clock ownership. In "self_ticking" mode the worker drives the firmware clock
// from its own timer using real measured deltas so previews animate at roughly
// real time even when no protocol request is in flight. In "explicit" mode the
// clock only advances when the host sends a `tick` envelope (deterministic mode
// used by tests, stories, and emulator-style harnesses).
const SELF_TICK_INTERVAL_MS = 33; // ~30 fps sim frame cadence.
let tickMode = "self_ticking";
let selfTickTimer = null;
let lastTickAtMs = null;

self.onmessage = async (event) => {
  try {
    const message = event.data || {};
    switch (message.kind) {
      case "boot":
        await boot(
          message.label || "browser-worker",
          message.fw_browser_module_path,
          message.fw_browser_wasm_path,
          message.tick_mode || "self_ticking",
        );
        break;
      case "protocol_in":
        requireBooted();
        postMany(fwBrowser.handle_envelope_json(runtimeId, JSON.stringify(message)));
        break;
      case "tick":
        requireBooted();
        postMany(fwBrowser.tick_runtime(runtimeId, message.delta_ms || 16));
        break;
      case "drain":
        requireBooted();
        postMany(fwBrowser.drain_output_json(runtimeId));
        break;
      case "start":
      case "stop":
        requireBooted();
        postMany(fwBrowser.handle_envelope_json(runtimeId, JSON.stringify(message)));
        break;
      default:
        throw new Error(`unknown worker message kind: ${message.kind}`);
    }
  } catch (error) {
    console.error("[fw-browser-worker]", error);
    self.postMessage({
      kind: "status",
      status: "error",
      message: String(error?.stack || error),
    });
  }
};

async function boot(label, modulePath, wasmPath, mode) {
  if (!booted) {
    if (!modulePath) {
      throw new Error("missing fw_browser_module_path");
    }
    self.postMessage({ kind: "status", status: "booting" });
    fwBrowser = await import(modulePath);
    const exports = await fwBrowser.default(wasmPath || undefined);
    fwBrowser.fw_browser_init_exports(exports);
    runtimeId = fwBrowser.create_runtime(label);
    booted = true;
    tickMode = mode;
    postMany(fwBrowser.drain_output_json(runtimeId));
    self.postMessage({ kind: "status", status: "ready" });
    if (tickMode === "self_ticking") {
      startSelfTick();
    }
  }
}

function startSelfTick() {
  if (selfTickTimer != null) {
    return;
  }
  lastTickAtMs = performance.now();
  selfTickTimer = setInterval(() => {
    if (!booted || runtimeId == null || fwBrowser == null) {
      return;
    }
    const now = performance.now();
    const deltaMs = Math.max(1, Math.round(now - lastTickAtMs));
    lastTickAtMs = now;
    try {
      postMany(fwBrowser.tick_runtime(runtimeId, deltaMs));
    } catch (error) {
      console.error("[fw-browser-worker] self-tick failed", error);
      self.postMessage({
        kind: "status",
        status: "error",
        message: String(error?.stack || error),
      });
    }
  }, SELF_TICK_INTERVAL_MS);
}

function requireBooted() {
  if (!booted || runtimeId == null || fwBrowser == null) {
    throw new Error("worker runtime has not booted");
  }
}

function postMany(envelopesJson) {
  for (const envelope of JSON.parse(envelopesJson)) {
    self.postMessage(envelope);
  }
}
