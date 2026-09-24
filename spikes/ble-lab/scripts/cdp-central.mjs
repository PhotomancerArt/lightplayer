#!/usr/bin/env node
// Drive a desktop Chrome as the lab's Web Bluetooth central — no human, no
// phone. The ble-lab page needs one Join (a `requestDevice` chooser); this
// answers it over the Chrome DevTools Protocol:
//
//   - `DeviceAccess.enable` makes Chrome report the chooser as a
//     `DeviceAccess.deviceRequestPrompted` event (with the devices found so
//     far, re-sent as more appear) instead of only drawing it;
//   - the Join click runs through `Runtime.evaluate` with `userGesture: true`,
//     which is the user activation `requestDevice` requires;
//   - `DeviceAccess.selectPrompt` picks the device by the name it advertises.
//
// Launch Chrome in the BACKGROUND with its own scratch profile, never a
// foreground window (it would steal the desk's focus):
//
//   open -g -n -a "Google Chrome" --args --remote-debugging-port=9333 \
//       --user-data-dir=<scratch>/chrome-ble --no-first-run \
//       --no-default-browser-check http://localhost:<lab port>/
//   node spikes/ble-lab/scripts/cdp-central.mjs --debug-port 9333 \
//       --page localhost:<lab port> join --name "LP-Zook dome"
//
// Commands:
//   join [--name <exact>|--prefix <p>] [--timeout-ms N] [--click-expr <js>]
//                                                         click Join, answer the chooser
//   js <expr>                                             evaluate in the page, print the value
//   targets                                               list the page targets
//
// Chrome needs macOS's Bluetooth permission (TCC) for itself; if it has
// none, the chooser reports no devices and this times out naming that. It
// never changes a system setting.

const args = process.argv.slice(2);
function opt(name, dflt) {
  const i = args.indexOf(name);
  if (i < 0) return dflt;
  const v = args[i + 1];
  args.splice(i, 2);
  return v;
}
const debugPort = Number(opt("--debug-port", "9333"));
const pageMatch = opt("--page", "localhost");
const [command, ...rest] = args;

async function pageTarget() {
  const r = await fetch(`http://127.0.0.1:${debugPort}/json`);
  const list = await r.json();
  const pages = list.filter((t) => t.type === "page");
  const hit = pages.find((t) => t.url.includes(pageMatch));
  if (!hit) throw new Error(`no page target matching ${pageMatch}: ${pages.map((p) => p.url).join(", ")}`);
  return hit;
}

function session(wsUrl) {
  const ws = new WebSocket(wsUrl);
  let next = 1;
  const pending = new Map();
  const listeners = [];
  ws.onmessage = (ev) => {
    const m = JSON.parse(ev.data);
    if (m.id && pending.has(m.id)) {
      const { resolve, reject } = pending.get(m.id);
      pending.delete(m.id);
      m.error ? reject(new Error(`${m.error.message} (${m.error.code})`)) : resolve(m.result);
    } else if (m.method) {
      for (const l of listeners) l(m);
    }
  };
  const open = new Promise((res, rej) => {
    ws.onopen = res;
    ws.onerror = () => rej(new Error(`cannot open ${wsUrl}`));
  });
  return {
    open,
    send(method, params = {}) {
      const id = next++;
      ws.send(JSON.stringify({ id, method, params }));
      return new Promise((resolve, reject) => pending.set(id, { resolve, reject }));
    },
    on(fn) {
      listeners.push(fn);
    },
    close() {
      ws.close();
    },
  };
}

async function evaluate(s, expression, userGesture = false) {
  const r = await s.send("Runtime.evaluate", {
    expression,
    userGesture,
    awaitPromise: true,
    returnByValue: true,
  });
  if (r.exceptionDetails) throw new Error(`page threw: ${JSON.stringify(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text)}`);
  return r.result.value;
}

async function join(s) {
  const name = opt("--name", undefined);
  const prefix = opt("--prefix", name ? undefined : "LP-");
  const timeoutMs = Number(opt("--timeout-ms", "30000"));
  // What the gesture runs; the lab page's Join by default.
  const clickExpr = opt("--click-expr", `document.getElementById("btn-join").click(); true`);
  const wanted = (d) => (name ? d.name === name : d.name.startsWith(prefix));
  await s.send("DeviceAccess.enable");
  const t0 = Date.now();
  const seen = new Set();
  let prompts = 0;
  const picked = new Promise((resolve, reject) => {
    const timer = setTimeout(async () => {
      reject(
        new Error(
          `no advertiser matching ${name ? `name "${name}"` : `prefix "${prefix}"`} in ${timeoutMs} ms; ${prompts} chooser event(s), which saw: [${[...seen].join(", ")}]` +
            (prompts === 0 ? " — Chrome never opened a chooser (did the click carry a user gesture? did requestDevice throw?)" : seen.size === 0 ? " — no devices at all: is Chrome allowed Bluetooth (System Settings › Privacy › Bluetooth)?" : ""),
        ),
      );
    }, timeoutMs);
    s.on(async (m) => {
      if (m.method !== "DeviceAccess.deviceRequestPrompted") return;
      const { id, devices } = m.params;
      prompts++;
      if (process.env.CDP_VERBOSE) console.error(`prompt ${id} +${Date.now() - t0} ms: ${JSON.stringify(devices)}`);
      for (const d of devices) seen.add(d.name);
      const hit = devices.find(wanted);
      if (!hit) return;
      clearTimeout(timer);
      try {
        await s.send("DeviceAccess.selectPrompt", { id, deviceId: hit.id });
        resolve({ name: hit.name, deviceId: hit.id, chooserMs: Date.now() - t0, seen: [...seen] });
      } catch (e) {
        reject(e);
      }
    });
  });
  // The click is not awaited: `requestDevice` resolves only once we pick.
  s.send("Runtime.evaluate", {
    expression: clickExpr,
    userGesture: true,
  });
  const res = await picked;
  // The page's own view once it has connected and subscribed.
  let status = null;
  for (let i = 0; i < 40; i++) {
    status = await evaluate(s, `(() => { const S = window.lab && window.lab.S; return S ? { connected: S.connected, device: S.device && S.device.name } : null; })()`);
    if (status && status.connected) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  return { ...res, page: status };
}

async function main() {
  const t = await pageTarget();
  if (command === "targets") {
    console.log(JSON.stringify(t, null, 1));
    return;
  }
  const s = session(t.webSocketDebuggerUrl);
  await s.open;
  try {
    if (command === "join") console.log(JSON.stringify(await join(s)));
    else if (command === "js") console.log(JSON.stringify(await evaluate(s, rest.join(" "))));
    else throw new Error(`unknown command ${command} (join | js | targets)`);
  } finally {
    s.close();
  }
}

main().catch((e) => {
  console.error(`cdp-central: ${e.message}`);
  process.exit(1);
});
