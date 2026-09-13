// One notification when jobs wait and no device is present (F2, vision Q3).
//
// The lab's failure mode is silence: a director queues an A/B pair, the phone
// is in another room with the tab closed, and nothing happens until somebody
// looks at `lab.sh status`. This module is the one line that closes that loop
// — and it is deliberately the smallest thing that can: no subscription
// store, no crypto, no service worker, one HTTPS POST to an endpoint that is
// Yona's and lives only in her `config.json`.
//
// **Nothing ships configured.** `config.json` with no `notify` key is off, and
// the repo carries no default topic, URL or host (E-outward): the outward
// reach is hers to grant, in her own file, after the merge.
//
// The channel (see the PR's investigation paragraph): **ntfy** — `POST
// https://ntfy.sh/<topic>`, where the unguessable topic is the address. Web
// Push from the lab page would need hand-written ES256 VAPID JWTs plus
// ECDH/HKDF/AES128GCM payload encryption (no dependencies are allowed here),
// a service worker, a manifest, a subscribe button and a stored subscription,
// and on iOS it only exists at all once the page is added to the Home Screen
// — several hundred lines whose failure mode is a silent non-delivery, to
// reach a phone whose lab tab is by definition closed. ntfy costs one free
// app on the phone and about twenty lines here. `kind: "webhook"` is the same
// sender pointed at any other https endpoint (Pushcut, a Shortcuts relay, a
// self-hosted ntfy), so a channel change after the hand test is a config
// edit, not another PR.
//
// The gate (`check` below) is three rules, not one:
//
//   armed        one notification per episode: sending disarms, and only the
//                condition clearing (a device present, or the queue draining)
//                re-arms. Three jobs queued in a minute are one notification
//                because they are one episode.
//   graceMs      the condition must hold continuously for this long. A phone
//                that blinks out of `visible` between two spaced presses is a
//                transient, not an absence.
//   minIntervalMs a floor between two sends however the arming went. iOS
//                hides a backgrounded tab, and hidden is not present (T4), so
//                a phone being picked up and put down would otherwise be a
//                notification machine.
//
// State lives in `$LAB_HOME/notify.json`, so a server restart does not re-send
// what it already sent.
'use strict';

import fs from 'node:fs';
import path from 'node:path';
import https from 'node:https';
import { execFile } from 'node:child_process';

const DEFAULTS = { server: 'https://ntfy.sh', graceMs: 30000, minIntervalMs: 600000, priority: 'default', clickUrl: null };

/// Header values reach ntfy as latin-1; an em dash in a title turns into
/// mojibake on the phone. Everything the phone sees is ASCII by construction.
function ascii(s) {
  return String(s).replace(/[‐-―]/g, '-').replace(/[^\x20-\x7E]/g, '').slice(0, 200);
}

/// Validate one `notify` block. Returns the effective config, or `null` with
/// `why` set for the log — a mistyped block is off and says so once, never a
/// crash loop on a machine-wide service.
export function readNotifyConfig(raw) {
  if (raw === undefined || raw === null || raw === false) return { cfg: null, why: null };
  if (typeof raw !== 'object' || Array.isArray(raw)) return { cfg: null, why: 'notify must be an object' };
  const cfg = { ...DEFAULTS, ...raw };
  for (const k of ['graceMs', 'minIntervalMs']) {
    const v = Number(cfg[k]);
    if (!Number.isFinite(v) || v < 0) return { cfg: null, why: k + ' must be a number of ms >= 0' };
    cfg[k] = v;
  }
  if (cfg.kind === 'ntfy') {
    if (typeof cfg.topic !== 'string' || !/^[A-Za-z0-9_-]{4,64}$/.test(cfg.topic)) {
      return { cfg: null, why: 'ntfy needs a topic of 4-64 chars [A-Za-z0-9_-] (make it unguessable: it IS the address)' };
    }
    if (!String(cfg.server).startsWith('https://')) return { cfg: null, why: 'ntfy server must be https' };
    cfg.target = String(cfg.server).replace(/\/+$/, '') + '/' + cfg.topic;
  } else if (cfg.kind === 'webhook') {
    if (typeof cfg.url !== 'string' || !cfg.url.startsWith('https://')) return { cfg: null, why: 'webhook needs an https url' };
    if (cfg.headers !== undefined && (typeof cfg.headers !== 'object' || Array.isArray(cfg.headers))) return { cfg: null, why: 'webhook headers must be an object' };
    cfg.target = cfg.url;
  } else {
    return { cfg: null, why: 'kind must be "ntfy" or "webhook", not ' + JSON.stringify(cfg.kind) };
  }
  if (cfg.clickUrl !== null && cfg.clickUrl !== undefined && !String(cfg.clickUrl).startsWith('https://')) {
    return { cfg: null, why: 'clickUrl must be https (and must NOT carry the token: the body passes through someone else\'s server)' };
  }
  return { cfg, why: null };
}

/// POST, https only, ten seconds, at most 2 KB of the answer kept for the log.
/// http:// is refused rather than downgraded: this is the one place the lab
/// speaks outward, and it does it encrypted or not at all.
function httpsPost(urlStr, headers, body) {
  return new Promise((resolve, reject) => {
    let u;
    try { u = new URL(urlStr); } catch { return reject(new Error('bad url ' + urlStr)); }
    if (u.protocol !== 'https:') return reject(new Error('notify sends over https only, not ' + u.protocol));
    const buf = Buffer.from(body, 'utf8');
    const req = https.request({
      hostname: u.hostname, port: u.port || 443, path: u.pathname + u.search, method: 'POST',
      headers: { 'Content-Length': buf.length, ...headers },
      timeout: 10000,
    }, (res) => {
      let out = '';
      res.setEncoding('utf8');
      res.on('data', (c) => { if (out.length < 2048) out += c; });
      res.on('end', () => resolve({ status: res.statusCode, body: out.trim().slice(0, 300) }));
    });
    req.on('timeout', () => req.destroy(new Error('no answer in 10 s')));
    req.on('error', reject);
    req.end(buf);
  });
}

/// The test seam the brief asked for: with `LAB_NOTIFY_CMD` set, that command
/// is the channel and NOTHING leaves the machine. The tests set it; a hand run
/// that wants to see the wiring without waking a phone sets it too.
function runCmd(cmd, env) {
  return new Promise((resolve, reject) => {
    execFile('/bin/sh', ['-c', cmd], { env: { ...process.env, ...env }, timeout: 10000 }, (err, stdout, stderr) => {
      if (err) return reject(new Error(String(stderr || err.message).trim().slice(0, 300)));
      resolve({ status: 0, body: String(stdout).trim().slice(0, 300) });
    });
  });
}

const EMPTY_STATE = { armed: true, notifiedAt: null, waitingSince: null, lastResult: null, lastError: null };

export function createNotifier({ home, configPath, log, now = () => Date.now() }) {
  const statePath = path.join(home, 'notify.json');
  let st = { ...EMPTY_STATE };
  try { st = { ...EMPTY_STATE, ...JSON.parse(fs.readFileSync(statePath, 'utf8')) }; } catch { /* first start */ }

  // The notify block is re-read when config.json's mtime moves, so Yona can
  // set her topic (and change it after a hand test) without a `restart
  // server` — which would cut a press in flight. Nothing else in the config
  // reloads; the port and the cooldown are still start-time.
  let cfg = null, mtime = null, lastWhy = null;
  function current() {
    let m = null;
    try { m = fs.statSync(configPath).mtimeMs; } catch { m = null; }
    if (m === mtime) return cfg;
    mtime = m;
    let raw;
    try { raw = JSON.parse(fs.readFileSync(configPath, 'utf8')).notify; } catch { raw = undefined; }
    const r = readNotifyConfig(raw);
    if (r.why && r.why !== lastWhy) log('notify: config ignored (' + r.why + ')');
    if (!r.why && r.cfg && (!cfg || cfg.target !== r.cfg.target)) log('notify: ' + r.cfg.kind + ' -> ' + (r.cfg.kind === 'ntfy' ? r.cfg.server + '/<topic>' : r.cfg.target) + ', grace ' + r.cfg.graceMs + ' ms, floor ' + r.cfg.minIntervalMs + ' ms');
    if (!r.cfg && cfg) log('notify: off (no notify block in config.json)');
    lastWhy = r.why;
    cfg = r.cfg;
    return cfg;
  }

  function save() {
    const tmp = statePath + '.tmp-' + process.pid;
    try {
      fs.writeFileSync(tmp, JSON.stringify(st, null, 2) + '\n');
      fs.renameSync(tmp, statePath);
    } catch (e) { log('notify: could not write ' + statePath + ': ' + e.message); }
  }

  function compose(waiting) {
    const presses = waiting.reduce((n, j) => n + j.presses.filter((p) => p.state !== 'done' && p.state !== 'failed').length, 0);
    const ids = waiting.slice(0, 3).map((j) => j.id);
    const more = waiting.length > 3 ? ' +' + (waiting.length - 3) + ' more' : '';
    return {
      title: 'emu-lab: ' + waiting.length + ' job' + (waiting.length === 1 ? '' : 's') + ' waiting',
      body: 'No device present. ' + presses + ' press' + (presses === 1 ? '' : 'es') + ' pending: ' + ids.join(', ') + more + '. Open the lab tab and tap Join.',
    };
  }

  async function deliver(c, title, body) {
    const cmd = process.env.LAB_NOTIFY_CMD;
    if (cmd) return runCmd(cmd, { LAB_NOTIFY_TITLE: title, LAB_NOTIFY_BODY: body, LAB_NOTIFY_KIND: c.kind, LAB_NOTIFY_TARGET: c.target });
    if (c.kind === 'ntfy') {
      const headers = { 'Content-Type': 'text/plain; charset=utf-8', Title: ascii(title), Priority: ascii(c.priority), Tags: 'hourglass' };
      if (c.clickUrl) headers.Click = ascii(c.clickUrl);
      return httpsPost(c.target, headers, body);
    }
    return httpsPost(c.target, { 'Content-Type': 'application/json', ...(c.headers || {}) }, JSON.stringify({ title, body, at: new Date(now()).toISOString(), source: 'emu-lab' }));
  }

  /// Send, and record what happened either way. A dead endpoint is logged and
  /// forgotten: the queue is not the notifier's business, and a phone that
  /// missed one is what `lab.sh status` has always been for.
  async function send(c, title, body, why) {
    try {
      const r = await deliver(c, title, body);
      st.lastResult = why + ': ' + c.kind + ' ' + r.status + (r.body ? ' ' + r.body : '');
      st.lastError = null;
      log('notify: sent (' + why + ') ' + c.kind + ' -> ' + r.status + ' — ' + title);
    } catch (e) {
      st.lastError = why + ': ' + e.message;
      st.lastResult = null;
      log('notify: send failed (' + why + '): ' + e.message);
    }
    save();
    return { ok: !st.lastError, result: st.lastResult, error: st.lastError };
  }

  return {
    /// One pass, called from the server's tick with the jobs that are not
    /// terminal and how many devices are present. Never throws and never
    /// blocks the tick: the send is a promise nobody awaits.
    check(waiting, presentCount) {
      const c = current();
      if (!c) return;
      const t = now();
      // The condition does not hold: re-arm. A device becoming present lands
      // here through `hooks.onPresenceChange` -> `tick`, which is the reset
      // the brief asked for; a queue that drained is the same thing.
      if (presentCount > 0 || waiting.length === 0) {
        if (!st.armed || st.waitingSince !== null) { st.armed = true; st.waitingSince = null; save(); }
        return;
      }
      if (st.waitingSince === null) { st.waitingSince = t; save(); }
      if (!st.armed) return;
      if (t - st.waitingSince < c.graceMs) return;
      if (st.notifiedAt && t - Date.parse(st.notifiedAt) < c.minIntervalMs) return;
      st.armed = false;
      st.notifiedAt = new Date(t).toISOString();
      save();
      const { title, body } = compose(waiting);
      send(c, title, body, 'queue waiting, no device').catch(() => { /* recorded in state */ });
    },

    /// `lab.sh notify test`: one send now, whatever the gate thinks. It does
    /// not consume the arm — a test must not cost the real notification.
    async sendTest() {
      const c = current();
      if (!c) return { ok: false, configured: false, error: 'no notify block in ' + configPath };
      const r = await send(c, 'emu-lab: test', 'A test notification from the perf lab. Nothing is waiting.', 'test');
      return { ok: r.ok, configured: true, kind: c.kind, result: r.result, error: r.error };
    },

    /// For `/status`. The topic and the URL are the address AND the secret, so
    /// they never leave the desk: kind and dates only.
    status() {
      const c = current();
      return {
        configured: !!c,
        kind: c ? c.kind : null,
        why: c ? null : lastWhy,
        graceMs: c ? c.graceMs : null,
        minIntervalMs: c ? c.minIntervalMs : null,
        armed: st.armed,
        notifiedAt: st.notifiedAt,
        waitingSince: st.waitingSince === null ? null : new Date(st.waitingSince).toISOString(),
        lastResult: st.lastResult,
        lastError: st.lastError,
      };
    },
  };
}
