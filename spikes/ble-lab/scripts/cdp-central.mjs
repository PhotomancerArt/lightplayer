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
//   - `DeviceAccess.selectPrompt` picks the device by its chooser id or name.
//
// Any page works: the lab (`/`), the frame pipe (`/pipe`, for `lp-cli link
// capture blepipe:`), or Studio itself (`--click-expr` on the Bluetooth square
// of its home page's Connect a board section).
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
//   join [--id <device id>|--name <exact>|--prefix <p>] [--timeout-ms N]
//        [--click-text <text>|--click-expr <js>]           click Join, answer the chooser
//   list [--timeout-ms N] [--click-text <text>|--click-expr <js>]
//                                                         open the chooser, print every
//                                                         device it offers (name, id),
//                                                         then cancel it: picks nothing
//   click <text>                                          click the page's first enabled
//                                                         control whose text has <text>
//   shot <file.png>                                       screenshot the page
//   js <expr>                                             evaluate in the page, print the value
//   targets                                               list the page targets
//
// `--id` is the chooser's device id, which `list` (or `join`'s output)
// prints. Prefer it: two desk boards can share a name prefix, and the
// chooser shows the name macOS cached, not the one the board advertises.
// `--click-text` presses a control by its text; Studio's Bluetooth square is
// better pressed with an exact, scoped `--click-expr` (see the README);
// the default press is the lab and pipe pages' `#btn-join`.
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

/// The press `join` and `list` make with a user gesture: `--click-text`,
/// else `--click-expr`, else the lab and pipe pages' Join button.
function pressExpr() {
  const text = opt("--click-text", undefined);
  const expr = opt("--click-expr", undefined);
  if (text !== undefined) return clickTextExpr(text);
  return expr ?? `document.getElementById("btn-join").click(); true`;
}

/// Click the first enabled button, link or [role=button] whose text has
/// `text` (case and spacing ignored): the clicked control's text, or null.
function clickTextExpr(text) {
  return `(() => {
    const wanted = ${JSON.stringify(text.toLowerCase())};
    const norm = (el) => (el.textContent || "").replace(/\\s+/g, " ").trim().toLowerCase();
    const el = [...document.querySelectorAll('button, [role="button"], a')]
      .find((el) => !el.disabled && norm(el).includes(wanted));
    if (!el) return null;
    el.scrollIntoView({ block: "center" });
    el.click();
    return norm(el);
  })()`;
}

/// Press `press` and watch the chooser it opens. `onPrompt(promptId,
/// devices, seen)` returns a result once it is done with it (or undefined
/// to keep watching); rejects after `timeoutMs`, naming what was seen.
async function watchChooser(s, press, timeoutMs, what, onPrompt) {
  await s.send("DeviceAccess.enable");
  const t0 = Date.now();
  // id → name, every device any chooser event offered.
  const seen = new Map();
  let prompts = 0;
  const done = new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      reject(
        new Error(
          `no ${what} in ${timeoutMs} ms; ${prompts} chooser event(s), which saw: [${[...seen].map(([id, name]) => `${name} (${id})`).join(", ")}]` +
            (prompts === 0 ? " — Chrome never opened a chooser (did the click carry a user gesture? did requestDevice throw?)" : seen.size === 0 ? " — no devices at all: is Chrome allowed Bluetooth (System Settings › Privacy › Bluetooth)?" : ""),
        ),
      );
    }, timeoutMs);
    s.on(async (m) => {
      if (m.method !== "DeviceAccess.deviceRequestPrompted") return;
      const { id, devices } = m.params;
      prompts++;
      if (process.env.CDP_VERBOSE) console.error(`prompt ${id} +${Date.now() - t0} ms: ${JSON.stringify(devices)}`);
      for (const d of devices) seen.set(d.id, d.name);
      try {
        const result = await onPrompt(id, devices, seen);
        if (result === undefined) return;
        clearTimeout(timer);
        resolve({ ...result, chooserMs: Date.now() - t0 });
      } catch (e) {
        clearTimeout(timer);
        reject(e);
      }
    });
  });
  // The press is not awaited: `requestDevice` resolves only once we pick.
  s.send("Runtime.evaluate", { expression: press, userGesture: true });
  return done;
}

const seenList = (seen) => [...seen].map(([id, name]) => ({ name, id }));

async function join(s) {
  const name = opt("--name", undefined);
  // `--id`: the chooser's device id (stable per board on one Chrome
  // profile, unlike the name, which macOS caches and two boards can share a
  // prefix of).
  const id = opt("--id", undefined);
  const prefix = opt("--prefix", name || id ? undefined : "LP-");
  const timeoutMs = Number(opt("--timeout-ms", "30000"));
  const press = pressExpr();
  const wanted = (d) => (id ? d.id === id : name ? d.name === name : d.name.startsWith(prefix));
  const what = `advertiser matching ${id ? `id "${id}"` : name ? `name "${name}"` : `prefix "${prefix}"`}`;
  const res = await watchChooser(s, press, timeoutMs, what, async (promptId, devices, seen) => {
    const hit = devices.find(wanted);
    if (!hit) return undefined;
    await s.send("DeviceAccess.selectPrompt", { id: promptId, deviceId: hit.id });
    return { name: hit.name, deviceId: hit.id, seen: seenList(seen) };
  });
  // The page's own view once it has connected and subscribed: the lab
  // page's `lab.S`, the pipe's `pipe.S`. Studio has neither (its card is
  // the view; read it with `js` or `shot`).
  let status = null;
  for (let i = 0; i < 40; i++) {
    status = await evaluate(
      s,
      `(() => { const S = (window.lab && window.lab.S) || (window.pipe && window.pipe.S); return S ? { connected: S.connected, device: S.device && (S.device.name || S.device) } : null; })()`,
    );
    if (!status || status.connected) break;
    await new Promise((r) => setTimeout(r, 250));
  }
  return { ...res, page: status };
}

/// Every device the chooser offers within `--timeout-ms`, then cancel the
/// chooser: nothing is picked, nothing connects.
async function list(s) {
  const timeoutMs = Number(opt("--timeout-ms", "10000"));
  const press = pressExpr();
  let last = null;
  const watching = watchChooser(s, press, timeoutMs + 5_000, "chooser", async (promptId, _devices, seen) => {
    last = { promptId, seen };
    return undefined;
  });
  watching.catch(() => {});
  await new Promise((r) => setTimeout(r, timeoutMs));
  if (!last) {
    throw new Error(`Chrome opened no chooser in ${timeoutMs} ms (did the press carry a user gesture? did requestDevice throw?)`);
  }
  await s.send("DeviceAccess.cancelPrompt", { id: last.promptId }).catch(() => {});
  return seenList(last.seen);
}

async function click(s, text) {
  const clicked = await evaluate(s, clickTextExpr(text), true);
  if (clicked === null) throw new Error(`no enabled control whose text has "${text}"`);
  return { clicked };
}

async function shot(s, file) {
  const { writeFile } = await import("node:fs/promises");
  const r = await s.send("Page.captureScreenshot", { format: "png" });
  await writeFile(file, Buffer.from(r.data, "base64"));
  return { file };
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
    else if (command === "list") console.log(JSON.stringify(await list(s)));
    else if (command === "click") console.log(JSON.stringify(await click(s, rest.join(" "))));
    else if (command === "shot") console.log(JSON.stringify(await shot(s, rest[0] ?? "page.png")));
    else if (command === "js") console.log(JSON.stringify(await evaluate(s, rest.join(" "))));
    else throw new Error(`unknown command ${command} (join | list | click | shot | js | targets)`);
  } finally {
    s.close();
  }
}

main().catch((e) => {
  console.error(`cdp-central: ${e.message}`);
  process.exit(1);
});
