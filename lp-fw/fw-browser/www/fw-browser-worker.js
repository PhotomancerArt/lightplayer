import init, {
  create_runtime,
  drain_output_json,
  fw_browser_init_exports,
  handle_envelope_json,
  tick_runtime,
} from './pkg/fw_browser.js';

let runtimeId = null;
let booted = false;

self.onmessage = async (event) => {
  try {
    const message = event.data || {};
    switch (message.kind) {
      case 'boot':
        await boot(message.label || 'browser-worker');
        break;
      case 'protocol_in':
        requireBooted();
        postMany(handle_envelope_json(runtimeId, JSON.stringify(message)));
        break;
      case 'tick':
        requireBooted();
        postMany(tick_runtime(runtimeId, message.delta_ms || 16));
        break;
      case 'drain':
        requireBooted();
        postMany(drain_output_json(runtimeId));
        break;
      case 'start':
      case 'stop':
        requireBooted();
        postMany(handle_envelope_json(runtimeId, JSON.stringify(message)));
        break;
      default:
        throw new Error(`unknown worker message kind: ${message.kind}`);
    }
  } catch (error) {
    self.postMessage({
      kind: 'status',
      status: 'error',
      message: String(error?.stack || error),
    });
  }
};

async function boot(label) {
  if (!booted) {
    self.postMessage({ kind: 'status', status: 'booting' });
    const exports = await init();
    fw_browser_init_exports(exports);
    // Smoke page runs the CPU tier (the authoritative sim tier).
    runtimeId = JSON.parse(create_runtime(label, 'cpu')).runtime_id;
    booted = true;
    postMany(drain_output_json(runtimeId));
  }
}

function requireBooted() {
  if (!booted || runtimeId == null) {
    throw new Error('worker runtime has not booted');
  }
}

function postMany(envelopesJson) {
  for (const envelope of JSON.parse(envelopesJson)) {
    self.postMessage(envelope);
  }
}
