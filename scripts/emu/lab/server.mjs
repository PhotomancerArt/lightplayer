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
//   POST /jobs                                queue a bench job (single build or an A/B pair)
//   GET  /jobs   GET /jobs/<id>   DELETE /jobs/<id>
//   POST /jobs/<id>/presses/<n>/result        the page's press result (legacy shape + taint fields)
//   POST /jobs/<id>/presses/<n>/deferred      the page received the press hidden; it will run when visible
//   GET  /wait?job=<id>|device=any|<name>|queue=idle [&timeout=3600]   ONE blocking call (D10)
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

import { computeReport, renderReportMd } from './report.mjs';

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
    tokenFailures,
    results: fs.readdirSync(path.join(HOME, 'results')).filter((f) => f.startsWith('result-') && f.endsWith('.json')).length,
  };
}

// --- the queue (D3, D5, D19, D20, D23) ----------------------------------------
//
// Jobs are files under jobs/<id>.json; presses land in jobs/<id>/presses/<n>.json
// and the report beside them. Everything below is rebuilt from those files on
// start, so a kill -9 loses nothing but the in-flight press, which is re-sent
// once (a `sent` press with no result is `lost`).

const TICK_MS = Number(process.env.LAB_TICK_MS || 1000);
const COOLDOWN_MS = process.env.LAB_COOLDOWN_MS !== undefined ? Number(process.env.LAB_COOLDOWN_MS) : config.cooldownMs;
// A press with no result after wallTimeout × rows + 120 s is lost. Tests
// shrink it; the build's manifest sets it in life.
const LOST_MS_OVERRIDE = process.env.LAB_LOST_MS !== undefined ? Number(process.env.LAB_LOST_MS) : null;
const MAX_WAIT_S = 3600;

const jobs = new Map(); // id -> job (the file's contents)

function jobPath(id) { return path.join(HOME, 'jobs', id + '.json'); }
function jobDir(id) { return path.join(HOME, 'jobs', id); }
function saveJob(j) { writeJsonFile(jobPath(j.id), j); return j; }

for (const f of fs.readdirSync(path.join(HOME, 'jobs')).filter((f) => f.endsWith('.json')).sort()) {
  const j = readJsonFile(path.join(HOME, 'jobs', f));
  if (!j || !j.id) continue;
  // Restart: a press we sent and never heard back from is lost; give it its
  // one re-send. A device that reconnects will get it again, once.
  for (const p of j.presses) if (p.state === 'sent' || p.state === 'deferred') markLost(j, p, 'server restarted');
  jobs.set(j.id, j);
}
if (jobs.size) log('loaded ' + jobs.size + ' job(s) from ' + path.join(HOME, 'jobs'));

function newJobId() {
  const d = new Date();
  const pad = (n) => String(n).padStart(2, '0');
  return 'j-' + d.getUTCFullYear() + pad(d.getUTCMonth() + 1) + pad(d.getUTCDate()) + '-' + pad(d.getUTCHours()) + pad(d.getUTCMinutes()) + '-' + crypto.randomBytes(2).toString('hex');
}

const TERMINAL = new Set(['done', 'expired', 'failed', 'cancelled']);
const PRESS_TERMINAL = new Set(['done', 'failed']);
const KNOWN_MODES = new Set(['jit', 'interp']);

/// Validate a job request and expand its presses A1 B1 A2 B2 … (D5). Only
/// `kind: bench` exists; the field is reserved so the shape does not forbid
/// another kind later (Q6).
function makeJob(body) {
  const bad = (m) => { throw Object.assign(new Error(m), { status: 400 }); };
  if (body.kind !== undefined && body.kind !== 'bench') bad('kind must be "bench"');
  const builds = Array.isArray(body.builds) ? body.builds : (body.build ? [body.build] : []);
  if (builds.length < 1 || builds.length > 2) bad('builds must name one build or an A/B pair');
  for (const b of builds) if (!hasBuild(b)) bad('unknown build ' + b + ' (stage it: bench-web.sh --stage-into ' + HOME + ')');
  if (builds.length === 2 && builds[0] === builds[1]) bad('an A/B pair needs two different builds');
  let rows = body.rows ?? 'gate-rows';
  if (rows !== 'gate-rows') {
    if (!Array.isArray(rows) || !rows.length) bad('rows must be "gate-rows" or a non-empty list');
    const slugs = new Set();
    for (const b of builds) for (const im of (readJsonFile(path.join(HOME, 'builds', b, 'manifest.json')).images || [])) slugs.add(im.slug);
    rows = rows.map((r) => {
      if (!r || !slugs.has(r.slug)) bad('unknown image slug ' + (r && r.slug));
      if (!KNOWN_MODES.has(r.mode)) bad('mode must be jit or interp');
      if (!['t1', 't2'].includes(r.grade)) bad('grade must be t1 or t2');
      return { slug: r.slug, grade: r.grade, mode: r.mode, fnBlocks: r.mode === 'jit' ? (Number(r.fnBlocks) || 32) : null, timeout: r.timeout || '5500ms' };
    });
  }
  const repeats = Number(body.repeats ?? 1);
  if (!Number.isInteger(repeats) || repeats < 1 || repeats > 20) bad('repeats must be 1–20');
  const spacingMs = Number(body.spacingMs ?? 0);
  if (!Number.isFinite(spacingMs) || spacingMs < 0) bad('spacingMs must be ≥ 0');
  const ttlMs = Number(body.ttlMs ?? 86400000);
  if (!Number.isFinite(ttlMs) || ttlMs < 60000) bad('ttlMs must be ≥ 60 s');
  const retryTainted = Number(body.retryTainted ?? 1);
  if (!Number.isInteger(retryTainted) || retryTainted < 0 || retryTainted > 5) bad('retryTainted must be 0–5');
  const device = typeof body.device === 'string' && body.device ? body.device : 'any';
  const presses = [];
  for (let r = 0; r < repeats; r++) for (const b of builds) presses.push({ n: presses.length + 1, build: b, state: 'pending', sentAt: null, deferredAt: null, resultAt: null, resends: 0, tainted: false, taintReasons: [] });
  return {
    id: newJobId(), kind: 'bench', builds, rows, repeats, spacingMs, device, ttlMs, retryTainted,
    note: typeof body.note === 'string' ? body.note.slice(0, 200) : null,
    createdAt: new Date().toISOString(), state: 'queued', boundDevice: null, boundDeviceName: null, boundDeviceUa: null,
    presses, lastPressEndAt: null, reportAt: null, retriesUsed: 0, error: null,
  };
}

function jobRowCount(j) { return j.rows === 'gate-rows' ? 4 : j.rows.length; }

function lostMs(j) {
  if (LOST_MS_OVERRIDE !== null) return LOST_MS_OVERRIDE;
  let wall = 600;
  for (const b of j.builds) {
    const m = readJsonFile(path.join(HOME, 'builds', b, 'manifest.json'));
    if (m && m.defaults && m.defaults.wallTimeout) wall = Math.max(wall, Number(m.defaults.wallTimeout));
  }
  return (wall * jobRowCount(j) + 120) * 1000;
}

function markLost(j, p, why) {
  if (p.resends < 1) {
    p.resends++; p.state = 'pending'; p.sentAt = null; p.deferredAt = null;
    log('job ' + j.id + ' press ' + p.n + ' lost (' + why + '); will re-send once');
  } else {
    p.state = 'failed'; p.error = 'lost twice (' + why + ')';
    log('job ' + j.id + ' press ' + p.n + ' lost twice (' + why + '); failed');
  }
}

/// The one view the page shows: this device's jobs. Sent whenever it changes.
const lastQueueSent = new Map();
function queueViewFor(deviceId) {
  return Array.from(jobs.values())
    .filter((j) => !TERMINAL.has(j.state) || (j.reportAt && Date.now() - Date.parse(j.reportAt) < 3600000))
    .filter((j) => (j.device === 'any' || j.device === deviceId || deviceNameMatches(j.device, deviceId)) && (!j.boundDevice || j.boundDevice === deviceId))
    .map((j) => ({
      id: j.id, state: j.state, builds: j.builds, note: j.note,
      presses: { done: j.presses.filter((p) => PRESS_TERMINAL.has(p.state)).length, total: j.presses.length },
      // One entry per press so the page can draw the interleave as ticks.
      pressStates: j.presses.map((p) => ({ n: p.n, build: p.build, state: p.state, tainted: !!p.tainted })),
    }));
}
function pushQueueViews() {
  for (const id of streams.keys()) {
    const v = JSON.stringify(queueViewFor(id));
    if (lastQueueSent.get(id) !== v) { lastQueueSent.set(id, v); sendToDevice(id, 'queue', { jobs: JSON.parse(v) }); }
  }
}

function deviceNameMatches(want, deviceId) {
  if (want === 'any') return true;
  if (want === deviceId) return true;
  const d = readDevice(deviceId);
  return !!(d && d.name && d.name === want);
}

function inFlightOn(deviceId) {
  for (const j of jobs.values()) if (j.boundDevice === deviceId) for (const p of j.presses) if (p.state === 'sent' || p.state === 'deferred') return { j, p };
  return null;
}

function finalize(j, state) {
  j.state = state;
  const presses = j.presses.map((p) => {
    const file = readJsonFile(path.join(jobDir(j.id), 'presses', p.n + '.json')) || {};
    return { n: p.n, build: p.build, state: p.state, tainted: p.tainted, taintReasons: p.taintReasons, resultAt: p.resultAt, results: file.results || [] };
  });
  const report = computeReport(j, presses);
  fs.mkdirSync(jobDir(j.id), { recursive: true });
  writeJsonFile(path.join(jobDir(j.id), 'report.json'), report);
  fs.writeFileSync(path.join(jobDir(j.id), 'report.md'), renderReportMd(report) + '\n');
  j.reportAt = new Date().toISOString();
  saveJob(j);
  log('job ' + j.id + ' ' + state + '; report at ' + path.join(jobDir(j.id), 'report.md'));
}

const lastCooldownSent = new Map();

/// One pass of the scheduler: expire, detect lost presses, then for every
/// present device with nothing in flight send at most one press — the first
/// pending press of the first job that passes the four conditions (D3,
/// D19, D20) — or tell the device when its next press is due.
function tick() {
  const now = Date.now();
  for (const j of jobs.values()) {
    if (TERMINAL.has(j.state)) continue;
    const inFlight = j.presses.some((p) => p.state === 'sent' || p.state === 'deferred');
    if (!inFlight && now - Date.parse(j.createdAt) > j.ttlMs) { finalize(j, 'expired'); continue; }
    let changed = false;
    for (const p of j.presses) {
      if (p.state === 'sent' && now - Date.parse(p.sentAt) > lostMs(j)) { markLost(j, p, 'no result in ' + Math.round(lostMs(j) / 1000) + ' s'); changed = true; }
      if (p.state === 'deferred' && now - Date.parse(p.deferredAt) > lostMs(j)) { markLost(j, p, 'deferred too long'); changed = true; }
    }
    if (j.presses.every((p) => PRESS_TERMINAL.has(p.state))) { finalize(j, 'done'); continue; }
    if (changed) saveJob(j);
  }
  for (const d of present()) {
    if (inFlightOn(d.id)) continue;
    const cooldownEnd = d.lastPressEndAt ? Date.parse(d.lastPressEndAt) + COOLDOWN_MS : 0;
    let nextAt = null, nextJob = null;
    for (const j of Array.from(jobs.values()).sort((a, b) => a.id.localeCompare(b.id))) {
      if (TERMINAL.has(j.state)) continue;
      if (!deviceNameMatches(j.device, d.id)) continue;
      if (j.boundDevice && j.boundDevice !== d.id) continue;
      const press = j.presses.find((p) => p.state === 'pending');
      if (!press) continue;
      const spacingEnd = j.lastPressEndAt ? Date.parse(j.lastPressEndAt) + j.spacingMs : 0;
      const due = Math.max(cooldownEnd, spacingEnd);
      if (due > now) { if (nextAt === null || due < nextAt) { nextAt = due; nextJob = j.id; } continue; }
      // Send it. Binding happens on the first press a device takes (D20).
      press.state = 'sent'; press.sentAt = new Date().toISOString();
      if (!j.boundDevice) { j.boundDevice = d.id; j.boundDeviceName = d.name; j.boundDeviceUa = d.ua; }
      j.state = 'running';
      saveJob(j);
      sendToDevice(d.id, 'press', { job: j.id, press: press.n, of: j.presses.length, build: press.build, rows: j.rows, nextPressAt: null });
      lastCooldownSent.delete(d.id);
      log('job ' + j.id + ' press ' + press.n + ' (' + press.build + ') -> ' + d.id + (press.resends ? ' (re-send)' : ''));
      nextAt = null;
      break;
    }
    const key = nextAt === null ? '' : nextJob + '@' + nextAt;
    if (lastCooldownSent.get(d.id) !== key) {
      lastCooldownSent.set(d.id, key);
      sendToDevice(d.id, 'cooldown', nextAt === null ? { job: null, nextPressAt: null } : { job: nextJob, nextPressAt: new Date(nextAt).toISOString() });
    }
  }
  pushQueueViews();
  releaseWaits();
}
setInterval(tick, TICK_MS).unref();

function jobCounts() {
  const c = { queued: 0, running: 0, done: 0 };
  for (const j of jobs.values()) { if (j.state === 'queued') c.queued++; else if (j.state === 'running') c.running++; else c.done++; }
  return c;
}

function jobSummary(j) {
  return { id: j.id, state: j.state, builds: j.builds, rows: j.rows, repeats: j.repeats, spacingMs: j.spacingMs, device: j.device, boundDevice: j.boundDevice, boundDeviceName: j.boundDeviceName,
    note: j.note, createdAt: j.createdAt, reportAt: j.reportAt, presses: { done: j.presses.filter((p) => PRESS_TERMINAL.has(p.state)).length, total: j.presses.length, tainted: j.presses.filter((p) => p.tainted).length, failed: j.presses.filter((p) => p.state === 'failed').length } };
}

async function postJob(req, res) {
  const body = await readJson(req, 64 * 1024);
  const j = makeJob(body);
  jobs.set(j.id, j);
  fs.mkdirSync(path.join(jobDir(j.id), 'presses'), { recursive: true });
  saveJob(j);
  log('job ' + j.id + ' queued: ' + j.builds.join(' vs ') + ' × ' + j.repeats + ', spacing ' + j.spacingMs + ' ms, device ' + j.device + (j.note ? ' — ' + j.note : ''));
  send(res, 201, j);
  tick();
}

function findPress(id, n) {
  const j = jobs.get(id);
  if (!j) throw Object.assign(new Error('no job ' + id), { status: 404 });
  const p = j.presses.find((x) => x.n === Number(n));
  if (!p) throw Object.assign(new Error('no press ' + n), { status: 404 });
  return { j, p };
}

/// Ingest one press: the press file, the legacy result file (D12), the job's
/// and the device's `lastPressEndAt` (D19), a retry press when tainted (D23),
/// the report when the job is complete.
async function postPressResult(req, res, id, n) {
  const { j, p } = findPress(id, n);
  if (PRESS_TERMINAL.has(p.state)) return send(res, 409, { error: 'press ' + n + ' already ' + p.state });
  const body = await readJson(req, config.maxResultBytes);
  checkResultPayload(body);
  const device = body.device || j.boundDevice;
  const at = new Date().toISOString();
  const payload = { ...body, job: j.id, press: p.n, buildId: p.build, device, receivedAt: at, manual: false };
  writeJsonFile(path.join(jobDir(j.id), 'presses', p.n + '.json'), payload);
  const file = writeResult(payload);
  p.resultAt = at; p.file = 'results/' + file;
  p.tainted = !!body.tainted; p.taintReasons = Array.isArray(body.taintReasons) ? body.taintReasons : [];
  if (body.failed || !body.results.length) { p.state = 'failed'; p.error = String(body.failed || 'no rows'); }
  else p.state = 'done';
  j.lastPressEndAt = at;
  if (isId(device)) touchDevice(device, { lastPressEndAt: at });
  if (p.state === 'done' && p.tainted && j.retriesUsed < j.retryTainted) {
    j.retriesUsed++;
    j.presses.push({ n: j.presses.length + 1, build: p.build, state: 'pending', sentAt: null, deferredAt: null, resultAt: null, resends: 0, tainted: false, taintReasons: [], retryOf: p.n });
    log('job ' + j.id + ' press ' + p.n + ' tainted (' + p.taintReasons.join(',') + '); press ' + j.presses.length + ' appended');
  }
  saveJob(j);
  log('job ' + j.id + ' press ' + p.n + ' ' + p.state + (p.tainted ? ' TAINTED' : '') + ' from ' + device + ' -> ' + p.file);
  send(res, 200, { ok: true, file: p.file, press: p.n, state: p.state });
  tick();
}

async function postPressDeferred(req, res, id, n) {
  const { j, p } = findPress(id, n);
  await readJson(req, 4096).catch(() => ({}));
  if (p.state === 'sent') { p.state = 'deferred'; p.deferredAt = new Date().toISOString(); saveJob(j); log('job ' + j.id + ' press ' + p.n + ' deferred (page hidden)'); }
  send(res, 200, { ok: true, state: p.state });
}

function cancelJob(res, id) {
  const j = jobs.get(id);
  if (!j) return send(res, 404, { error: 'no job ' + id });
  if (TERMINAL.has(j.state)) return send(res, 200, jobSummary(j));
  finalize(j, 'cancelled');
  send(res, 200, jobSummary(j));
  tick();
}

// --- waits (D10, T7): one blocking GET, answered when the condition holds ---

const waiters = [];

function waitCheck(w) {
  const q = w.q;
  if (q.job) {
    const j = jobs.get(q.job);
    if (!j) return { status: 404, body: { error: 'no job ' + q.job } };
    if (TERMINAL.has(j.state)) return { status: 200, body: { job: jobSummary(j), report: readJsonFile(path.join(jobDir(j.id), 'report.json')), reportMd: path.join(jobDir(j.id), 'report.md') } };
    return null;
  }
  if (q.device) {
    const d = present().find((x) => q.device === 'any' || x.id === q.device || x.name === q.device);
    return d ? { status: 200, body: { device: { id: d.id, name: d.name, ua: d.ua, cores: d.cores, lastState: d.lastState } } } : null;
  }
  if (q.queue === 'idle') {
    const busy = Array.from(jobs.values()).filter((j) => !TERMINAL.has(j.state));
    return busy.length ? null : { status: 200, body: { idle: true, jobs: jobCounts() } };
  }
  return { status: 400, body: { error: 'wait needs ?job=<id>, ?device=any|<name>, or ?queue=idle' } };
}

function releaseWaits() {
  for (let i = waiters.length - 1; i >= 0; i--) {
    const w = waiters[i];
    const r = waitCheck(w);
    if (r) { waiters.splice(i, 1); clearTimeout(w.timer); send(w.res, r.status, r.body); }
  }
}

function openWait(req, res, url) {
  const q = { job: url.searchParams.get('job'), device: url.searchParams.get('device'), queue: url.searchParams.get('queue') };
  const timeoutS = Math.min(MAX_WAIT_S, Math.max(1, Number(url.searchParams.get('timeout') || MAX_WAIT_S)));
  const w = { q, res, timer: null };
  const r = waitCheck(w);
  if (r) return send(res, r.status, r.body);
  // No keep-alive bytes: the body is JSON and one chunk. The socket stays
  // open for the whole wait; the director passes --max-time on its side.
  res.socket.setTimeout(0);
  w.timer = setTimeout(() => {
    const i = waiters.indexOf(w);
    if (i >= 0) waiters.splice(i, 1);
    const j = q.job ? jobs.get(q.job) : null;
    send(res, 408, { timeout: true, state: j ? j.state : null, presses: j ? jobSummary(j).presses : null });
  }, timeoutS * 1000);
  res.on('close', () => { const i = waiters.indexOf(w); if (i >= 0) { waiters.splice(i, 1); clearTimeout(w.timer); } });
  waiters.push(w);
}

const hooks = {
  onPresenceChange(_id) { tick(); },
  jobCounts,
  async route(req, res, url) {
    const p = url.pathname, m = req.method;
    let mm;
    if (m === 'GET' && p === '/wait') { openWait(req, res, url); return true; }
    if (m === 'POST' && p === '/jobs') { await postJob(req, res); return true; }
    if (m === 'GET' && p === '/jobs') { send(res, 200, { jobs: Array.from(jobs.values()).sort((a, b) => a.id.localeCompare(b.id)).map(jobSummary) }); return true; }
    if ((mm = /^\/jobs\/([^/]+)$/.exec(p))) {
      if (m === 'GET') { const j = jobs.get(mm[1]); if (!j) send(res, 404, { error: 'no job ' + mm[1] }); else send(res, 200, j); return true; }
      if (m === 'DELETE') { cancelJob(res, mm[1]); return true; }
    }
    if (m === 'POST' && (mm = /^\/jobs\/([^/]+)\/presses\/(\d+)\/result$/.exec(p))) { await postPressResult(req, res, mm[1], mm[2]); return true; }
    if (m === 'POST' && (mm = /^\/jobs\/([^/]+)\/presses\/(\d+)\/deferred$/.exec(p))) { await postPressDeferred(req, res, mm[1], mm[2]); return true; }
    if (m === 'GET' && (mm = /^\/jobs\/([^/]+)\/report\.(json|md)$/.exec(p))) {
      const f = path.join(jobDir(mm[1]), 'report.' + mm[2]);
      if (!jobs.has(mm[1]) || !fs.existsSync(f)) send(res, 404, { error: 'no report yet' });
      else send(res, 200, mm[2] === 'md' ? fs.readFileSync(f, 'utf8') : readJsonFile(f));
      return true;
    }
    return false;
  },
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

server.listen(PORT, '0.0.0.0', () => {
  const port = server.address().port;
  // stdout carries exactly one line, for the test that spawns us and for a
  // launchd log a human reads; the rest goes to stderr and log/server.log.
  process.stdout.write('emu-lab: listening on http://127.0.0.1:' + port + '\n');
  log('started, home ' + HOME + ', port ' + port + ', build ' + (SERVER_BUILD || '?') + ', cooldown ' + COOLDOWN_MS + ' ms');
  tick();
});

process.on('SIGTERM', () => { log('SIGTERM'); server.close(); process.exit(0); });
process.on('SIGINT', () => { log('SIGINT'); server.close(); process.exit(0); });
