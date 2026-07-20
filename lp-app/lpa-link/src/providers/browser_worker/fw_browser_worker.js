let bootRuntimeId = null;
let booted = false;
let fwBrowser = null;
let wasmExports = null;

// Clock ownership. In "self_ticking" mode the worker drives the firmware clock
// from its own timer using real measured deltas so previews animate at roughly
// real time even when no protocol request is in flight. In "explicit" mode the
// clock only advances when the host sends a `tick` envelope (deterministic mode
// used by tests, stories, and emulator-style harnesses).
//
// Self-ticking only drives the boot runtime; runtimes created later via
// `create_runtime` (preview lab) are always ticked explicitly by their host.
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
      case "create_runtime": {
        requireBooted();
        const label = message.label || "browser-runtime";
        const created = JSON.parse(fwBrowser.create_runtime(label, message.tier || "cpu"));
        postMany(fwBrowser.drain_output_json(created.runtime_id));
        self.postMessage({
          kind: "runtime_created",
          runtime_id: created.runtime_id,
          label,
          tier: created.tier,
          tier_reason: created.tier_reason ?? null,
        });
        break;
      }
      case "destroy_runtime":
        requireBooted();
        destroyRuntime(message);
        break;
      case "attach_surface":
        requireBooted();
        attachSurface(message);
        break;
      case "present_frame":
        requireBooted();
        presentFrame(message);
        break;
      case "protocol_in":
        requireBooted();
        postMany(
          fwBrowser.handle_envelope_json(targetRuntime(message), JSON.stringify(message)),
        );
        break;
      case "tick":
        requireBooted();
        postMany(fwBrowser.tick_runtime(targetRuntime(message), message.delta_ms || 16));
        break;
      case "preview_frame":
        requireBooted();
        previewFrame(message);
        break;
      case "drain":
        requireBooted();
        postMany(fwBrowser.drain_output_json(targetRuntime(message)));
        break;
      case "start":
      case "stop":
        requireBooted();
        postMany(
          fwBrowser.handle_envelope_json(targetRuntime(message), JSON.stringify(message)),
        );
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
    wasmExports = await fwBrowser.default(wasmPath || undefined);
    fwBrowser.fw_browser_init_exports(wasmExports);
    // One WebGPU device request per worker, at boot. The outcome (available
    // or unavailable with a reason) is recorded inside the wasm module and
    // applied to every later `gpu` tier request — boot never fails over it.
    const gpuInit = JSON.parse(await fwBrowser.init_gpu_device());
    if (!gpuInit.available) {
      console.info("[fw-browser-worker] webgpu unavailable:", gpuInit.reason);
    }
    // The boot runtime is always CPU-tier (the authoritative sim tier).
    bootRuntimeId = JSON.parse(fwBrowser.create_runtime(label, "cpu")).runtime_id;
    booted = true;
    tickMode = mode;
    postMany(fwBrowser.drain_output_json(bootRuntimeId));
    self.postMessage({ kind: "status", status: "ready" });
    if (tickMode === "self_ticking") {
      startSelfTick();
    }
  }
}

// Binary preview path: tick + render in one worker turn, then transfer the
// RGBA8 pixel buffer to the page. Pixels intentionally bypass the JSON
// envelope path; only the small timing header is structured-cloned.
function previewFrame(message) {
  const runtimeId = message.runtime_id;
  const frameId = message.frame_id || 0;
  try {
    const t0 = performance.now();
    if (message.delta_ms != null) {
      postMany(fwBrowser.tick_runtime(runtimeId, Math.max(1, message.delta_ms)));
    }
    const t1 = performance.now();
    const pixels = fwBrowser.render_bus_texture_rgba8(
      runtimeId,
      message.channel || "visual.out",
      message.width,
      message.height,
    );
    const t2 = performance.now();
    self.postMessage(
      {
        kind: "preview_pixels",
        runtime_id: runtimeId,
        frame_id: frameId,
        width: message.width,
        height: message.height,
        tick_ms: t1 - t0,
        render_ms: t2 - t1,
        posted_epoch_ms: performance.timeOrigin + performance.now(),
        wasm_memory_bytes: wasmExports?.memory?.buffer?.byteLength || 0,
        pixels: pixels.buffer,
      },
      [pixels.buffer],
    );
  } catch (error) {
    self.postMessage({
      kind: "preview_error",
      runtime_id: runtimeId,
      frame_id: frameId,
      message: String(error?.stack || error),
    });
  }
}

// Runtime disposal: release a preview lease so the worker can be recycled.
// Destroying the boot runtime is an error (it is the authoritative sim
// serving single-runtime consumers); destroying an unknown id is a no-op ack
// so releases are idempotent.
function destroyRuntime(message) {
  const runtimeId = message.runtime_id;
  try {
    if (runtimeId === bootRuntimeId) {
      throw new Error(`refusing to destroy the boot runtime (${runtimeId})`);
    }
    fwBrowser.destroy_runtime(runtimeId);
    self.postMessage({ kind: "runtime_destroyed", runtime_id: runtimeId });
  } catch (error) {
    self.postMessage({
      kind: "preview_error",
      runtime_id: runtimeId,
      frame_id: 0,
      message: String(error?.stack || error),
    });
  }
}

// GPU-tier surface attachment: the OffscreenCanvas arrives in the message
// transfer list and moves into the wasm runtime as the card's wgpu surface.
function attachSurface(message) {
  const runtimeId = message.runtime_id;
  try {
    fwBrowser.attach_preview_surface(runtimeId, message.canvas);
    postMany(fwBrowser.drain_output_json(runtimeId));
    self.postMessage({ kind: "surface_attached", runtime_id: runtimeId });
  } catch (error) {
    self.postMessage({
      kind: "preview_error",
      runtime_id: runtimeId,
      frame_id: 0,
      message: String(error?.stack || error),
    });
  }
}

// GPU-tier present path: tick + render straight to the attached card surface
// in one worker turn. No pixels leave the GPU; only the timing header is
// posted back (mirrors the binary preview path's measurements).
function presentFrame(message) {
  const runtimeId = message.runtime_id;
  const frameId = message.frame_id || 0;
  try {
    const t0 = performance.now();
    if (message.delta_ms != null) {
      postMany(fwBrowser.tick_runtime(runtimeId, Math.max(1, message.delta_ms)));
    }
    const t1 = performance.now();
    fwBrowser.present_bus_texture(runtimeId, message.channel || "visual.out");
    const t2 = performance.now();
    self.postMessage({
      kind: "preview_presented",
      runtime_id: runtimeId,
      frame_id: frameId,
      tick_ms: t1 - t0,
      render_ms: t2 - t1,
      posted_epoch_ms: performance.timeOrigin + performance.now(),
      wasm_memory_bytes: wasmExports?.memory?.buffer?.byteLength || 0,
    });
  } catch (error) {
    self.postMessage({
      kind: "preview_error",
      runtime_id: runtimeId,
      frame_id: frameId,
      message: String(error?.stack || error),
    });
  }
}

function targetRuntime(message) {
  return message.runtime_id != null ? message.runtime_id : bootRuntimeId;
}

function startSelfTick() {
  if (selfTickTimer != null) {
    return;
  }
  lastTickAtMs = performance.now();
  selfTickTimer = setInterval(() => {
    if (!booted || bootRuntimeId == null || fwBrowser == null) {
      return;
    }
    const now = performance.now();
    const deltaMs = Math.max(1, Math.round(now - lastTickAtMs));
    lastTickAtMs = now;
    try {
      postMany(fwBrowser.tick_runtime(bootRuntimeId, deltaMs));
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
  if (!booted || bootRuntimeId == null || fwBrowser == null) {
    throw new Error("worker runtime has not booted");
  }
}

function postMany(envelopesJson) {
  for (const envelope of JSON.parse(envelopesJson)) {
    self.postMessage(envelope);
  }
}
