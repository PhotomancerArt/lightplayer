#!/usr/bin/env node
// The emulator perf lab's server: the page holds the device, this process
// holds the queue, the director drives it over HTTP.
//
//   node scripts/emu/lab/server.mjs                 # LAB_HOME=~/.photomancer/emu-lab, port from config.json (41111)
//   LAB_HOME=/tmp/x LAB_PORT=0 node scripts/emu/lab/server.mjs   # tests: a temp home, an OS-assigned port
//
// Dependency-free by rule (D11, D13): `node:http`, `node:fs`, `node:crypto`,
// `node:path` and nothing else — no package.json anywhere under
// scripts/emu/lab/. Runs under /opt/homebrew/bin/node from launchd (P4), so
// nothing here may need an rc file or an nvm shim.
//
// What it serves and what it guards (D9, D17):
//
//   GET  /  /index.html  /lab-page.js         the lab page (this directory's, not a build's — T9)
//   GET  /builds/<id>/<file>                  a staged build; ELFs are symlinks into images/ (D16)
//   GET  /images/<sha12>.elf                  the content-addressed ELF store
//   GET  /healthz                             {ok, build, uptime} — the tunnel's liveness probe, no token
//   ---- everything below needs the token: `Authorization: Bearer <t>` or `?t=<t>` ----
//   GET  /events?device=<id>                  SSE presence stream (hello, press, cooldown, queue events)
//   POST /devices/<id>/state                  {name, ua, cores, deviceMemory, visibility, hasFocus, wakeLock}
//   POST /results/manual?device=<id>          a hand-taken run, the legacy result shape + manual:true (D12)
//   GET  /status                              devices, builds, job counts, result count
//
// The token is the guard, not the interface: the server binds 0.0.0.0 because
// the LAN and the tunnel both reach it, and every write and the presence
// stream refuse without the token. Static files are open — a build directory
// is not a secret, and the page must load before it has the token in hand.
//
// The lab home (D6) is never under a worktree: cargo-clean.sh deletes idle
// worktree target/ directories nightly with no age check, and took a night's
// uploads with one once. Everything the server knows is a file under HOME so
// a session can `ls` it and a restart loses nothing.
'use strict';

import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import os from 'node:os';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const HOME = path.resolve(process.env.LAB_HOME || path.join(os.homedir(), '.photomancer', 'emu-lab'));
const STARTED = Date.now();

// --- the home: directories, config, token ------------------------------------

for (const d of ['builds', 'images', 'jobs', 'results', 'devices', 'log']) {
  fs.mkdirSync(path.join(HOME, d), { recursive: true });
}

const DEFAULT_CONFIG = { port: 41111, domain: null, cooldownMs: 60000, maxResultBytes: 2000000 };
const configPath = path.join(HOME, 'config.json');
let config = { ...DEFAULT_CONFIG };
if (fs.existsSync(configPath)) {
  config = { ...DEFAULT_CONFIG, ...JSON.parse(fs.readFileSync(configPath, 'utf8')) };
} else {
  fs.writeFileSync(configPath, JSON.stringify(config, null, 2) + '\n');
}
// The port is pinned (D14): a machine-wide service cannot hash per worktree
// the way dev-port.sh does, and the plan declared the pin. LAB_PORT is for
// tests (0 = OS-assigned), never for a second lab.
const PORT = process.env.LAB_PORT !== undefined ? Number(process.env.LAB_PORT) : config.port;

// 32 hex, 0600, generated once (D17). The director reads the file; the phone
// gets it in a bookmark's fragment, which never reaches a server log.
const tokenPath = path.join(HOME, 'token');
let TOKEN;
if (fs.existsSync(tokenPath)) {
  TOKEN = fs.readFileSync(tokenPath, 'utf8').trim();
} else {
  TOKEN = crypto.randomBytes(16).toString('hex');
  fs.writeFileSync(tokenPath, TOKEN + '\n', { mode: 0o600 });
}
const TOKEN_BUF = Buffer.from(TOKEN);

const logPath = path.join(HOME, 'log', 'server.log');
function log(line) {
  const l = new Date().toISOString() + ' ' + line;
  try { fs.appendFileSync(logPath, l + '\n'); } catch { /* the log is a courtesy, never a failure */ }
  if (!process.env.LAB_QUIET) process.stderr.write('emu-lab: ' + line + '\n');
}

let SERVER_BUILD = null;
try {
  SERVER_BUILD = execFileSync('git', ['-C', HERE, 'rev-parse', '--short', 'HEAD'], { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim();
} catch { /* not a checkout (an installed copy); healthz says null */ }

// --- helpers -----------------------------------------------------------------

function send(res, status, body, headers = {}) {
  const buf = Buffer.from(typeof body === 'string' ? body : JSON.stringify(body));
  res.writeHead(status, {
    'Content-Type': typeof body === 'string' ? 'text/plain; charset=utf-8' : 'application/json',
    'Content-Length': buf.length,
    ...headers,
  });
  res.end(buf);
}

let tokenFailures = 0;
/// The token check: header or query, compared in constant time on equal
/// lengths. A miss is a 401 with nothing written and nothing logged beyond a
/// counter — a public URL sees noise, and noise must not fill the log.
function authed(req, url) {
  let t = url.searchParams.get('t');
  const h = req.headers.authorization;
  if (!t && h && h.startsWith('Bearer ')) t = h.slice(7).trim();
  if (!t) return false;
  const b = Buffer.from(t);
  if (b.length !== TOKEN_BUF.length) return false;
  return crypto.timingSafeEqual(b, TOKEN_BUF);
}

function readBody(req, cap) {
  return new Promise((resolve, reject) => {
    const chunks = [];
    let n = 0;
    let over = false;
    req.on('data', (c) => {
      if (over) return;
      n += c.length;
      if (n > cap) {
        // Refuse, but drain rather than destroy: a torn socket reaches the
        // client as ECONNRESET, and the 413 is the message.
        over = true; chunks.length = 0;
        reject(Object.assign(new Error('body over ' + cap + ' bytes'), { status: 413 }));
        return;
      }
      chunks.push(c);
    });
    req.on('end', () => resolve(Buffer.concat(chunks)));
    req.on('error', reject);
  });
}

async function readJson(req, cap) {
  const buf = await readBody(req, cap);
  try { return JSON.parse(buf.toString('utf8')); } catch { throw Object.assign(new Error('body is not JSON'), { status: 400 }); }
}

const ID_RE = /^[A-Za-z0-9_.-]{1,64}$/;
const isId = (s) => typeof s === 'string' && ID_RE.test(s) && !s.startsWith('.');

function readJsonFile(p, fallback = null) {
  try { return JSON.parse(fs.readFileSync(p, 'utf8')); } catch { return fallback; }
}

function writeJsonFile(p, obj) {
  const tmp = p + '.tmp-' + process.pid;
  fs.writeFileSync(tmp, JSON.stringify(obj, null, 2) + '\n');
  fs.renameSync(tmp, p);
}

// --- static files ------------------------------------------------------------

const TYPES = {
  '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.mjs': 'text/javascript; charset=utf-8',
  '.json': 'application/json', '.wasm': 'application/wasm', '.elf': 'application/octet-stream',
};
const PAGE_FILES = new Set(['index.html', 'lab-page.js']);
const HOME_REAL = fs.realpathSync(HOME);
const HERE_REAL = fs.realpathSync(HERE);

/// Serve one file, or 404. `root` is the directory whose real path the file's
/// real path must stay inside: the ELF symlinks (D16) resolve into images/,
/// which is inside the home, and anything pointing out of it — a `..`, a
/// planted symlink — is a 404 and not an explanation.
function serveFile(req, res, root, rel, url) {
  if (rel.split('/').some((s) => s === '..' || s === '' || s.startsWith('.'))) return send(res, 404, { error: 'not found' });
  let real;
  try { real = fs.realpathSync(path.join(root, rel)); } catch { return send(res, 404, { error: 'not found' }); }
  if (real !== root && !real.startsWith(root + path.sep)) return send(res, 404, { error: 'not found' });
  let st;
  try { st = fs.statSync(real); } catch { return send(res, 404, { error: 'not found' }); }
  if (!st.isFile()) return send(res, 404, { error: 'not found' });
  const ext = path.extname(real);
  const base = path.basename(real);
  // The page and every manifest are where a stamp COMES FROM, so they never
  // come out of a cache; a stamped build file is immutable by construction
  // (the ?v= chain from #712) and can be cached for good.
  let cache = 'no-cache';
  if (base === 'index.html' || base === 'manifest.json') cache = 'no-store';
  else if (url.searchParams.has('v')) cache = 'public, max-age=31536000, immutable';
  res.writeHead(200, {
    'Content-Type': TYPES[ext] || 'application/octet-stream',
    'Content-Length': st.size,
    'Cache-Control': cache,
  });
  if (req.method === 'HEAD') return res.end();
  fs.createReadStream(real).pipe(res);
}

// --- devices and presence (D10, the beacon half) -----------------------------

/// deviceId -> Set<res>: every open /events stream for that device.
const streams = new Map();

function devicePath(id) { return path.join(HOME, 'devices', id + '.json'); }
function readDevice(id) { return readJsonFile(devicePath(id)); }
function saveDevice(d) { writeJsonFile(devicePath(d.id), d); return d; }
function touchDevice(id, patch) {
  const d = readDevice(id) || { id, name: null, ua: null, cores: null, deviceMemory: null, lastState: null, lastSeen: null, lastPressEndAt: null };
  Object.assign(d, patch, { lastSeen: new Date().toISOString() });
  return saveDevice(d);
}

/// Present = at least one open stream AND the last posted visibility is
/// `visible`. A hidden tab is connected and useless (T4), so it is not present.
function isPresent(d) {
  const s = streams.get(d.id);
  return !!(s && s.size > 0 && d.lastState && d.lastState.visibility === 'visible');
}

function allDevices() {
  return fs.readdirSync(path.join(HOME, 'devices'))
    .filter((f) => f.endsWith('.json'))
    .map((f) => readJsonFile(path.join(HOME, 'devices', f)))
    .filter(Boolean)
    .sort((a, b) => String(b.lastSeen).localeCompare(String(a.lastSeen)));
}

function present() { return allDevices().filter(isPresent); }

function sseWrite(res, event, data) {
  res.write('event: ' + event + '\ndata: ' + JSON.stringify(data) + '\n\n');
}

/// Send one event to every open stream of a device; returns how many took it.
function sendToDevice(id, event, data) {
  const s = streams.get(id);
  if (!s) return 0;
  for (const res of s) sseWrite(res, event, data);
  return s.size;
}

function openEvents(req, res, url) {
  const id = url.searchParams.get('device');
  if (!isId(id)) return send(res, 400, { error: 'device' });
  res.writeHead(200, {
    'Content-Type': 'text/event-stream',
    'Cache-Control': 'no-cache',
    'X-Accel-Buffering': 'no',
    Connection: 'keep-alive',
  });
  res.socket.setTimeout(0);
  res.socket.setNoDelay(true);
  if (!streams.has(id)) streams.set(id, new Set());
  streams.get(id).add(res);
  const d = touchDevice(id, {});
  sseWrite(res, 'hello', { serverTime: new Date().toISOString(), config: { cooldownMs: config.cooldownMs }, device: d });
  // ngrok and Safari both drop a silent stream; a comment every 15 s is the
  // cheapest thing that keeps both honest.
  const ka = setInterval(() => { try { res.write(': keepalive\n\n'); } catch { /* closing */ } }, 15000);
  log('device ' + id + ' stream open (' + streams.get(id).size + ')');
  const close = () => {
    clearInterval(ka);
    const s = streams.get(id);
    if (s) { s.delete(res); if (s.size === 0) streams.delete(id); }
    touchDevice(id, {});
    log('device ' + id + ' stream closed');
    hooks.onPresenceChange(id);
  };
  res.on('close', close);
  hooks.onPresenceChange(id);
}

async function postState(req, res, id) {
  const body = await readJson(req, 64 * 1024);
  const state = {
    visibility: body.visibility ?? null,
    hasFocus: body.hasFocus ?? null,
    wakeLock: body.wakeLock ?? null,
  };
  const patch = { lastState: state };
  for (const k of ['name', 'ua', 'cores', 'deviceMemory']) if (body[k] !== undefined) patch[k] = body[k];
  const d = touchDevice(id, patch);
  send(res, 200, d);
  hooks.onPresenceChange(id);
}

// --- results (D9, the write half of D12) --------------------------------------

/// Write one result file in the legacy `result-<ISO>.json` shape. The server
/// names it from its own clock — never a client name — and two writes in one
/// millisecond get `-2`, `-3` rather than a collision or an overwrite (`wx`).
function writeResult(payload) {
  const at = new Date().toISOString();
  const stem = 'result-' + at.replace(/[:.]/g, '-');
  let name = stem + '.json';
  for (let n = 2; ; n++) {
    try {
      fs.writeFileSync(path.join(HOME, 'results', name), JSON.stringify(payload, null, 2) + '\n', { flag: 'wx' });
      return name;
    } catch (e) {
      if (e.code !== 'EEXIST') throw e;
      name = stem + '-' + n + '.json';
    }
  }
}

function checkResultPayload(body) {
  if (!body || typeof body !== 'object' || Array.isArray(body)) throw Object.assign(new Error('payload must be an object'), { status: 400 });
  if (!Array.isArray(body.results)) throw Object.assign(new Error('results must be an array'), { status: 400 });
}

async function postManualResult(req, res, url) {
  const body = await readJson(req, config.maxResultBytes);
  checkResultPayload(body);
  const device = url.searchParams.get('device');
  const payload = { ...body, manual: true, device: isId(device) ? device : null, receivedAt: new Date().toISOString() };
  const file = writeResult(payload);
  log('manual result from ' + (payload.device || '?') + ' -> results/' + file + ' (' + body.results.length + ' rows)');
  send(res, 200, { ok: true, file: 'results/' + file });
}

// --- builds ------------------------------------------------------------------

function listBuilds() {
  const dir = path.join(HOME, 'builds');
  return fs.readdirSync(dir)
    .filter((f) => !f.startsWith('.'))
    .map((id) => {
      const m = readJsonFile(path.join(dir, id, 'manifest.json'));
      if (!m || !m.build) return null;
      const b = m.build;
      return { id, short: b.short, branch: b.branch, dirty: !!b.dirty, built_at: b.built_at, wasm_bytes: b.wasm_bytes };
    })
    .filter(Boolean)
    .sort((a, b) => String(b.built_at).localeCompare(String(a.built_at)));
}

function hasBuild(id) { return isId(id) && fs.existsSync(path.join(HOME, 'builds', id, 'manifest.json')); }

// --- status ------------------------------------------------------------------

function status() {
  return {
    serverTime: new Date().toISOString(),
    uptimeS: Math.round((Date.now() - STARTED) / 1000),
    home: HOME,
    port: PORT,
    config: { cooldownMs: config.cooldownMs, domain: config.domain },
    devices: allDevices().map((d) => ({ id: d.id, name: d.name, present: isPresent(d), streams: (streams.get(d.id) || new Set()).size, lastSeen: d.lastSeen, lastState: d.lastState, ua: d.ua, cores: d.cores, lastPressEndAt: d.lastPressEndAt })),
    builds: listBuilds(),
    jobs: hooks.jobCounts(),
    results: fs.readdirSync(path.join(HOME, 'results')).filter((f) => f.startsWith('result-') && f.endsWith('.json')).length,
  };
}

// --- the seam the queue (P3) plugs into --------------------------------------

/// P1 ships presence and results; the scheduler arrives in P3 and replaces
/// these. Keeping the seam explicit is what lets `status` report zeros
/// honestly today rather than a shape that changes later.
const hooks = {
  onPresenceChange(_id) {},
  jobCounts() { return { queued: 0, running: 0, done: 0 }; },
  route(_req, _res, _url) { return false; },
};

// --- the router --------------------------------------------------------------

async function handle(req, res) {
  const url = new URL(req.url, 'http://localhost');
  const p = url.pathname;
  const m = req.method;

  // Open routes: the page, the store, liveness.
  if ((m === 'GET' || m === 'HEAD') && (p === '/' || p === '/index.html')) return serveFile(req, res, HERE_REAL, 'index.html', url);
  if ((m === 'GET' || m === 'HEAD') && p.startsWith('/') && PAGE_FILES.has(p.slice(1))) return serveFile(req, res, HERE_REAL, p.slice(1), url);
  if ((m === 'GET' || m === 'HEAD') && p.startsWith('/builds/')) return serveFile(req, res, HOME_REAL, p.slice(1), url);
  if ((m === 'GET' || m === 'HEAD') && p.startsWith('/images/')) return serveFile(req, res, HOME_REAL, p.slice(1), url);
  if (m === 'GET' && p === '/healthz') return send(res, 200, { ok: true, build: SERVER_BUILD, uptimeS: Math.round((Date.now() - STARTED) / 1000) });

  if (!authed(req, url)) { tokenFailures++; return send(res, 401, { error: 'token' }); }

  if (m === 'GET' && p === '/events') return openEvents(req, res, url);
  if (m === 'GET' && p === '/status') return send(res, 200, status());
  if (m === 'POST' && p === '/results/manual') return postManualResult(req, res, url);
  let mm;
  if (m === 'POST' && (mm = /^\/devices\/([^/]+)\/state$/.exec(p))) {
    if (!isId(mm[1])) return send(res, 400, { error: 'device' });
    return postState(req, res, mm[1]);
  }
  if (await hooks.route(req, res, url)) return undefined;
  return send(res, 404, { error: 'not found' });
}

const server = http.createServer((req, res) => {
  handle(req, res).catch((e) => {
    const status = e && e.status ? e.status : 500;
    if (status === 500) log('500 ' + req.method + ' ' + req.url + ': ' + (e && e.stack || e));
    if (!res.headersSent) send(res, status, { error: String(e && e.message || e) });
    else res.end();
  });
});
// Blocking waits (P3) hold a response for up to an hour; node's defaults would
// cut them at five minutes.
server.requestTimeout = 0;
server.headersTimeout = 60000;
server.keepAliveTimeout = 65000;

export { HOME, config, TOKEN, hooks, log, present, allDevices, readDevice, touchDevice, isPresent, sendToDevice, streams, writeResult, checkResultPayload, readJson, readJsonFile, writeJsonFile, hasBuild, listBuilds, send, isId };

server.listen(PORT, '0.0.0.0', () => {
  const port = server.address().port;
  // stdout carries exactly one line, for the test that spawns us and for a
  // launchd log a human reads; the rest goes to stderr and log/server.log.
  process.stdout.write('emu-lab: listening on http://127.0.0.1:' + port + '\n');
  log('started, home ' + HOME + ', port ' + port + ', build ' + (SERVER_BUILD || '?') + ', token failures reset');
});

process.on('SIGTERM', () => { log('SIGTERM'); server.close(); process.exit(0); });
process.on('SIGINT', () => { log('SIGINT'); server.close(); process.exit(0); });
