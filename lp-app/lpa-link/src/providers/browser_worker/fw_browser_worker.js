let runtimeId = null;
let booted = false;
let fwBrowser = null;

self.onmessage = async (event) => {
  try {
    const message = event.data || {};
    switch (message.kind) {
      case "boot":
        await boot(
          message.label || "browser-worker",
          message.fw_browser_module_path,
          message.fw_browser_wasm_path,
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
    self.postMessage({
      kind: "status",
      status: "error",
      message: String(error?.stack || error),
    });
  }
};

async function boot(label, modulePath, wasmPath) {
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
    postMany(fwBrowser.drain_output_json(runtimeId));
    self.postMessage({ kind: "status", status: "ready" });
  }
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
