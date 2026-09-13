#!/usr/bin/env node
// Join the lab from a headless Chrome over the DevTools protocol — the one
// sanctioned way an agent puts a browser on the lab (README "Verifying the
// page"). Types the device name and clicks Join; nothing else.
//
//   node scripts/emu/lab/test/cdp-join.mjs <debug-port> <device-name>
//
// Dependency-free: node's global WebSocket (22+) against /json on the port.
'use strict';

const [port, name] = process.argv.slice(2);
if (!port || !name) { console.error('usage: cdp-join.mjs <debug-port> <device-name>'); process.exit(2); }

const targets = await (await fetch('http://127.0.0.1:' + port + '/json')).json();
const page = targets.find((t) => t.type === 'page' && t.url.includes('127.0.0.1:41111') || t.type === 'page');
if (!page) { console.error('no page target on port ' + port); process.exit(1); }
const ws = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((r, j) => { ws.onopen = r; ws.onerror = j; });
let id = 0;
const pending = new Map();
ws.onmessage = (ev) => { const m = JSON.parse(ev.data); if (m.id && pending.has(m.id)) { pending.get(m.id)(m); pending.delete(m.id); } };
const call = (method, params = {}) => new Promise((r) => { const i = ++id; pending.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });

const evalIn = async (expression) => {
  const r = await call('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  if (r.result && r.result.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails));
  return r.result && r.result.result ? r.result.result.value : undefined;
};

// Wait for the page's module to wire the Join button up.
for (let i = 0; i < 50; i++) {
  const ready = await evalIn("!!document.getElementById('joinBtn') && !document.getElementById('join').hidden");
  if (ready) break;
  await new Promise((r) => setTimeout(r, 200));
}
await evalIn("document.getElementById('name').value = " + JSON.stringify(name));
await evalIn("document.getElementById('joinBtn').click(); 'clicked'");
await new Promise((r) => setTimeout(r, 1500));
const presence = await evalIn("document.getElementById('presence').textContent");
const vis = await evalIn('document.visibilityState');
const devId = await evalIn('localStorage.labDeviceId');
console.log(JSON.stringify({ joined: true, device: devId, name, visibility: vis, presence }));
ws.close();
