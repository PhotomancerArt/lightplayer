// The lab page: the one thing only a page can hold is the device, so this
// module's whole job is to hold it honestly — join once, keep the screen on,
// run exactly the press the server sends, and say on every row whether the
// tab was visible, focused and lock-held while it ran (D8).
//
// Loaded by index.html as `import { main } from './lab-page.js'`. The pure
// parts — the row record, the taint reducer, the payload builder — are
// exported and run under `node --test` with no DOM; `main()` is the only thing
// that touches `document`.
//
// The rig's worker contract is reused unchanged (T9): a press starts
// `builds/<id>/worker.js?v=<id>` as a module worker, posts
// `{plan, preset, defaults, images}`, and reads `loaded | progress | result |
// done | error` — exactly what scripts/emu/bench-web/index.html does. The id
// is the stamp, so every sibling the worker fetches lands inside that build
// directory with the #712 chain intact.
'use strict';

// --- pure: row records and taint (D8) ---------------------------------------

/// Start a row's record from the page's state as the row begins.
export function newRowRecord(s) {
  return {
    visibilityAtStart: s.visibility, visibilityAtEnd: null,
    sawHidden: s.visibility !== 'visible',
    hasFocusAtStart: !!s.hasFocus, hasFocusAtEnd: null,
    wakeLock: s.wakeLock, lockReleased: false,
  };
}

/// The tab went hidden (or came back) while the row ran.
export function noteVisibility(rec, visibility) {
  if (visibility !== 'visible') rec.sawHidden = true;
  return rec;
}

/// The screen lock was released while the row ran.
export function noteLockRelease(rec) {
  rec.lockReleased = true;
  rec.wakeLock = 'released';
  return rec;
}

/// Close the record and decide the taint. A row is tainted when the tab was
/// hidden at any point or a held lock was released under it; a lock that was
/// never held (`none`, `unsupported`, `denied`) is recorded, not a taint —
/// the row ran in the foreground, which is what the number needs.
export function finishRow(rec, s) {
  rec.visibilityAtEnd = s.visibility;
  rec.hasFocusAtEnd = !!s.hasFocus;
  if (s.visibility !== 'visible') rec.sawHidden = true;
  const reasons = [];
  if (rec.sawHidden) reasons.push('hidden');
  if (rec.lockReleased) reasons.push('lock-released');
  rec.tainted = reasons.length > 0;
  rec.taintReasons = reasons;
  return rec;
}

/// The uploaded payload: the rig's legacy shape first (every key `send()`
/// writes today, same names and types — D12), then the lab's additive keys.
export function buildPayload(o) {
  const results = o.results;
  const reasons = new Set();
  for (const r of results) for (const t of (r.taintReasons || [])) reasons.add(t);
  const payload = {
    ua: o.ua, cores: o.cores, at: o.at, build: o.manifest && o.manifest.build, deviceMemory: o.deviceMemory ?? null,
    results,
  };
  if (o.preset) payload.preset = o.preset;
  Object.assign(payload, {
    device: o.device, deviceName: o.deviceName ?? null,
    job: o.job ?? null, press: o.press ?? null, buildId: o.buildId ?? null,
    manual: !!o.manual,
    visibility: o.state.visibility, hasFocus: !!o.state.hasFocus, wakeLock: o.state.wakeLock,
    tainted: reasons.size > 0 || !!o.failed,
    taintReasons: Array.from(reasons),
    deferredMs: o.deferredMs ?? 0,
  });
  if (o.failed) payload.failed = String(o.failed);
  return payload;
}

/// A real default for the device name, derived from the UA — a placeholder
/// reads like a filled-in value on a phone, so the field is pre-filled with
/// something true and editable instead (G1 finding).
export function defaultDeviceName(ua) {
  ua = ua || '';
  if (/iPhone/.test(ua)) return 'iPhone';
  if (/iPad/.test(ua)) return 'iPad';
  if (/Android/.test(ua)) return 'Android';
  const browser = /CriOS|Chrome\//.test(ua) ? 'Chrome' : /Firefox\/|FxiOS/.test(ua) ? 'Firefox' : /Safari\//.test(ua) ? 'Safari' : 'browser';
  if (/Macintosh/.test(ua)) return 'Mac ' + browser;
  if (/Windows/.test(ua)) return 'Windows ' + browser;
  if (/Linux/.test(ua)) return 'Linux ' + browser;
  return browser;
}

/// The one line the board leads with. Pure so the state machine is testable:
/// `s` is `{joined, connected, pendingHidden, running, cooldown, queued, lastHeadline}`.
export function boardState(s) {
  if (!s.joined) return { kind: 'off', text: 'Not joined' };
  if (s.pendingHidden) return { kind: 'refused', text: 'Refused — press ' + s.pendingHidden.press + ' received while the tab is hidden; it runs when the tab is visible' };
  if (!s.connected) return { kind: 'off', text: 'Reconnecting to the lab…' };
  if (s.running) {
    const r = s.running;
    const where = r.row ? ' · row ' + r.row.i + ' of ' + r.row.n + ' · ' + r.row.what : ' · loading the build';
    return { kind: 'running', text: (r.manual ? 'Running manual' : 'Running press ' + r.press + ' of ' + r.of) + ' · ' + r.build + where, progress: r.row ? (r.row.i - 1) / r.row.n : 0 };
  }
  if (s.cooldown && s.cooldown.nextPressAt) {
    const left = Math.max(0, Math.round((new Date(s.cooldown.nextPressAt).getTime() - s.now) / 1000));
    return { kind: 'cooldown', text: 'Cooling down · next press in ' + Math.floor(left / 60) + ':' + String(left % 60).padStart(2, '0') + (s.cooldown.job ? ' · ' + s.cooldown.job : '') };
  }
  if (s.queued > 0) return { kind: 'waiting', text: 'Waiting · ' + s.queued + ' press' + (s.queued === 1 ? '' : 'es') + ' queued for this device' };
  if (s.lastHeadline) return { kind: 'done', text: 'Done · ' + s.lastHeadline + ' · nothing queued' };
  return { kind: 'idle', text: 'Idle · joined, nothing queued' };
}

/// Build a plan from an explicit row list the way the rig's `buildPlan` does,
/// with the wall guard and the exit policy from the build's own defaults.
export function planFromRows(rows, defaults) {
  const d = defaults || {};
  const wallTimeout = d.wallTimeout ?? 600;
  const exitOn = !!d.exitOn;
  return rows.map((r) => ({
    slug: r.slug, grade: r.grade, mode: r.mode,
    fnBlocks: r.mode === 'jit' ? (r.fnBlocks ?? d.fnBlocks ?? 32) : null,
    timeout: r.timeout || d.timeout || '5500ms', wallTimeout, exitOn,
  }));
}

// --- the page ----------------------------------------------------------------

export function main() {
  const $ = (id) => document.getElementById(id);
  const els = {
    ua: $('ua'), noToken: $('noToken'), join: $('join'), joinBtn: $('joinBtn'), name: $('name'),
    board: $('board'), presence: $('presence'), lockBtn: $('lockBtn'), jobs: $('jobs'), press: $('press'),
    state: $('state'), bar: $('bar'), last: $('last'),
    countdown: $('countdown'), status: $('status'), headlines: $('headlines'), table: $('results'),
    tbody: document.querySelector('#results tbody'), boot: $('boot'), tail: $('tail'),
    manual: $('manual'), build: $('build'), run: $('run'), runGate: $('runGate'),
  };

  // --- identity and the token (D17) ---
  const hash = new URLSearchParams(location.hash.replace(/^#/, ''));
  if (hash.get('t')) {
    localStorage.labToken = hash.get('t');
    history.replaceState(null, '', location.pathname + location.search);
  }
  const token = localStorage.labToken;
  if (!localStorage.labDeviceId) {
    localStorage.labDeviceId = (crypto.randomUUID ? crypto.randomUUID() : Math.random().toString(16).slice(2) + Math.random().toString(16).slice(2)).replace(/-/g, '').slice(0, 24);
  }
  const deviceId = localStorage.labDeviceId;
  els.ua.textContent = navigator.userAgent + ' · cores ' + (navigator.hardwareConcurrency || '?') +
    (navigator.deviceMemory ? ' · mem ' + navigator.deviceMemory + ' GB' : '') + ' · id ' + deviceId.slice(0, 8);
  if (!token) { els.noToken.hidden = false; return; }
  els.join.hidden = false;
  els.name.value = localStorage.labName || defaultDeviceName(navigator.userAgent);

  // Every fetch carries the token and the ngrok skip header; EventSource can
  // carry only the token, which is why Q8 is a gate question and not a fix.
  const api = (path, opts = {}) => fetch(path, {
    ...opts,
    headers: { Authorization: 'Bearer ' + token, 'ngrok-skip-browser-warning': '1', 'Content-Type': 'application/json', ...(opts.headers || {}) },
  });
  const postJson = (path, body) => api(path, { method: 'POST', body: JSON.stringify(body) });

  // --- state ---
  let lockState = ('wakeLock' in navigator && isSecureContext) ? 'none' : 'unsupported';
  let lock = null;
  let es = null;
  let joined = false;
  let closedInARow = 0;
  let current = null;       // the row record of the row running now
  let running = null;       // the press running now
  let pending = null;       // a press received hidden, waiting for visible
  let cooldownTimer = null;
  let lastQueue = [];
  let cooldown = null;      // {job, nextPressAt} from the server
  let rowNow = null;        // {i, n, what} while a row runs
  let lastHeadline = null;  // the previous press's best line, kept between presses

  const state = () => ({ visibility: document.visibilityState, hasFocus: document.hasFocus(), wakeLock: lockState });

  /// The banner: one line and a colour that say what the lab is doing.
  function renderState() {
    const queued = lastQueue.reduce((a, j) => a + (j.pressStates || []).filter((p) => p.state === 'pending').length, 0);
    const st = boardState({
      joined, connected: !!(es && es.readyState === 1), pendingHidden: pending && document.visibilityState !== 'visible' ? pending : null,
      running: running ? { manual: !!running.manual, press: running.press, of: running.of, build: running.build, row: rowNow } : null,
      cooldown, queued, lastHeadline, now: Date.now(),
    });
    els.state.textContent = st.text;
    els.state.className = 'state ' + st.kind;
    els.bar.hidden = st.kind !== 'running';
    if (st.kind === 'running') els.bar.firstElementChild.style.width = Math.round((st.progress || 0) * 100) + '%';
  }
  setInterval(renderState, 1000);

  function renderPresence() {
    const s = state();
    const lockWord = { active: 'screen lock held', released: 'screen lock RELEASED', none: 'no screen lock', unsupported: 'no wake lock on this browser', denied: 'screen lock denied' }[lockState] || lockState;
    els.presence.innerHTML = (joined ? (es && es.readyState === 1 ? 'joined' : 'reconnecting…') : 'not joined') +
      ' · ' + s.visibility + (s.hasFocus ? ' · focused' : ' · unfocused') + ' · ' + (lockState === 'released' ? '<span class="bad">' + lockWord + '</span>' : lockWord);
    els.lockBtn.hidden = !(joined && lockState !== 'active' && lockState !== 'unsupported');
    renderState();
  }

  async function requestLock() {
    if (lockState === 'unsupported') return;
    try {
      lock = await navigator.wakeLock.request('screen');
      lockState = 'active';
      lock.addEventListener('release', () => {
        // Released by the OS (the page went hidden, the screen locked), never
        // by us: the row running now cannot be trusted (T6).
        lockState = 'released';
        if (current) noteLockRelease(current);
        renderPresence();
      });
    } catch (e) {
      lockState = 'denied';
    }
    renderPresence();
  }

  async function postState() {
    try {
      await postJson('/devices/' + deviceId + '/state', {
        name: localStorage.labName || null, ua: navigator.userAgent, cores: navigator.hardwareConcurrency || null,
        deviceMemory: navigator.deviceMemory || null, ...state(),
      });
    } catch { /* the stream is the beacon; state is best effort */ }
  }

  // --- Join (T6, Q2): the tap is the gesture that makes the lock legal ---
  async function join() {
    localStorage.labName = els.name.value.trim() || localStorage.labName || 'unnamed';
    els.name.value = localStorage.labName;
    joined = true;
    localStorage.labJoined = '1';
    els.join.hidden = true; els.board.hidden = false; els.manual.hidden = false;
    await requestLock();
    openStream();
    await postState();
    renderPresence();
  }

  function openStream() {
    if (es) es.close();
    es = new EventSource('/events?device=' + encodeURIComponent(deviceId) + '&t=' + encodeURIComponent(token));
    es.onopen = () => { closedInARow = 0; renderPresence(); postState(); renderState(); };
    es.onerror = () => {
      // EventSource reconnects on its own; two CLOSED in a row means the
      // reconnect got something that was not a stream — the ngrok
      // interstitial (Q8) — and only a reload can get past it.
      if (es.readyState === 2) { closedInARow++; if (closedInARow >= 2) els.status.innerHTML = '<span class="bad">The lab stream will not reconnect (Q8). Reload the tab once.</span>'; setTimeout(openStream, 3000); }
      renderPresence();
    };
    es.addEventListener('hello', (ev) => {
      const d = JSON.parse(ev.data);
      if (!els.status.textContent || /Reload|reconnect/.test(els.status.textContent)) els.status.textContent = 'Joined. Waiting for the director.';
      if (d.device && d.device.lastPressEndAt) { /* nothing to show yet */ }
    });
    es.addEventListener('press', (ev) => onPress(JSON.parse(ev.data)));
    es.addEventListener('cooldown', (ev) => showCooldown(JSON.parse(ev.data)));
    es.addEventListener('queue', (ev) => { lastQueue = JSON.parse(ev.data).jobs || []; renderJobs(); });
  }

  // A card per job with one tick per press: the interleave is visible, and
  // a tainted or failed press shows as what it is rather than as a count.
  function renderJobs() {
    if (!lastQueue.length) { els.jobs.textContent = 'No jobs for this device.'; renderState(); return; }
    const glyph = (p) => {
      if (p.state === 'done' && p.tainted) return { c: 'tainted', g: '⚠', t: 'tainted' };
      if (p.state === 'done') return { c: 'ok', g: '✓', t: 'done' };
      if (p.state === 'failed') return { c: 'fail', g: '✗', t: 'failed' };
      if (p.state === 'sent' || p.state === 'deferred') return { c: 'now', g: '●', t: p.state };
      return { c: 'todo', g: '○', t: 'pending' };
    };
    els.jobs.innerHTML = lastQueue.map((j) => {
      const ticks = (j.pressStates || []).map((p) => { const g = glyph(p); return '<span class="tick ' + g.c + '" title="press ' + p.n + ' ' + p.build + ' ' + g.t + '">' + g.g + '</span>'; }).join('');
      const legend = j.builds.length === 2 ? ' <span class="note">(A ' + j.builds[0] + ', B ' + j.builds[1] + ', alternating)</span>' : '';
      return '<div class="job ' + j.state + '"><div><b>' + j.state + '</b> · ' + j.builds.join(' vs ') + ' · ' + j.presses.done + ' of ' + j.presses.total + ' presses' + legend + '</div>' +
        '<div class="ticks">' + ticks + '</div><div class="note">' + j.id + (j.note ? ' — ' + j.note : '') + '</div></div>';
    }).join('');
    renderState();
  }

  function showCooldown(d) {
    clearInterval(cooldownTimer);
    cooldown = d && d.nextPressAt ? d : null;
    if (!cooldown) { els.countdown.textContent = ''; renderState(); return; }
    const at = new Date(d.nextPressAt).getTime();
    const tick = () => {
      const left = Math.max(0, Math.round((at - Date.now()) / 1000));
      els.countdown.textContent = 'next press in ' + Math.floor(left / 60) + ':' + String(left % 60).padStart(2, '0') + (d.job ? ' · ' + d.job : '');
      if (left <= 0) { clearInterval(cooldownTimer); els.countdown.textContent = 'next press due' + (d.job ? ' · ' + d.job : ''); }
      renderState();
    };
    tick();
    cooldownTimer = setInterval(tick, 1000);
  }

  // --- visibility, focus, lock (D8, T4) ---
  document.addEventListener('visibilitychange', async () => {
    if (document.visibilityState === 'visible') {
      if (joined && lockState !== 'unsupported') await requestLock();
      if (current) noteVisibility(current, 'visible');
      renderPresence();
      await postState();
      if (pending && !running) { const p = pending; pending = null; runPress(p); }
    } else {
      if (current) noteVisibility(current, 'hidden');
      renderPresence();
      await postState();
    }
  });
  window.addEventListener('focus', renderPresence);
  window.addEventListener('blur', renderPresence);

  // --- a press (D22) ---
  let deferredAt = null;
  function onPress(p) {
    if (running) { pending = p; return; }
    if (document.visibilityState !== 'visible') {
      // Refusal: never start a row hidden. Hold it, tell the server, run on
      // the next `visible`.
      pending = p;
      deferredAt = Date.now();
      els.press.innerHTML = '<span class="bad">Press ' + p.press + ' received while hidden — waiting for the tab to be visible</span>';
      postJson('/jobs/' + p.job + '/presses/' + p.press + '/deferred', { reason: 'hidden' }).catch(() => {});
      renderState();
      return;
    }
    runPress(p);
  }

  function setBusy(busy) { els.run.disabled = busy; els.runGate.disabled = busy; els.build.disabled = busy; }

  /// One press, whoever asked for it: the server (`p.job` set) or the manual
  /// controls (`p.manual`). One fresh Worker per press — the warm-up the
  /// phone showed lives in the device, not in a reused worker, and a fresh
  /// Worker per press is what makes A/B presses comparable.
  async function runPress(p) {
    running = p;
    rowNow = null;
    const deferredMs = deferredAt ? Date.now() - deferredAt : 0;
    deferredAt = null;
    showCooldown(null);
    setBusy(true);
    const results = [];
    // The previous press's best line stays visible on the board while the
    // next press runs; the table and the boot lines are this press's own.
    if (lastHeadline) els.last.textContent = 'last press: ' + lastHeadline;
    els.tbody.innerHTML = ''; els.headlines.innerHTML = ''; els.boot.textContent = ''; els.tail.textContent = ''; els.table.hidden = true;
    renderState();
    const who = p.manual ? 'manual' : 'press ' + p.press + (p.of ? ' of ' + p.of : '');
    els.press.textContent = who + ' · build ' + p.build + ' · loading…';
    let manifest = null, worker = null, failed = null;
    const preset = p.rows === 'gate-rows' ? 'gate-rows' : null;
    try {
      manifest = await (await fetch('builds/' + p.build + '/manifest.json?t=' + Date.now(), { cache: 'no-store', headers: { 'ngrok-skip-browser-warning': '1' } })).json();
      const plan = preset ? null : planFromRows(p.rows, manifest.defaults);
      await new Promise((resolve) => {
        worker = new Worker('builds/' + p.build + '/worker.js?v=' + p.build, { type: 'module' });
        worker.onerror = (e) => { failed = 'Worker died: ' + (e.message || e); resolve(); };
        worker.onmessage = (ev) => {
          const d = ev.data;
          if (d.type === 'loaded') els.press.textContent = who + ' · build ' + p.build + ' · emu.wasm compiled in ' + d.compileMs.toFixed(0) + ' ms';
          if (d.type === 'progress') {
            current = newRowRecord(state());
            const what = d.step.slug + ' ' + d.step.grade + ' ' + (d.step.mode === 'jit' ? 'translated fn ' + d.step.fnBlocks : '--interpreter');
            rowNow = { i: d.i + 1, n: d.n, what };
            els.press.textContent = who + ' · build ' + p.build + ' · row ' + (d.i + 1) + ' of ' + d.n + ' (' + what + ')… keep the screen on';
            renderState();
          }
          if (d.type === 'result') {
            const rec = finishRow(current || newRowRecord(state()), state());
            current = null;
            const r = { ...d.r, ...rec };
            results.push(r); addRow(d.i, r, results);
          }
          if (d.type === 'done') resolve();
          if (d.type === 'error') { failed = d.message; resolve(); }
        };
        worker.postMessage({ plan, preset, defaults: manifest.defaults, images: manifest.images });
      });
    } catch (e) {
      failed = String((e && e.stack) || e);
    }
    if (worker) worker.terminate();
    current = null;
    rowNow = null;
    const bestRow = results.filter((r) => r.realtime && !r.tainted).reduce((a, b) => (!a || b.realtime > a.realtime ? b : a), null);
    if (bestRow) lastHeadline = bestRow.realtime.toFixed(3) + '× real time · ' + bestRow.slug + ' ' + bestRow.grade + ' ' + (bestRow.mode === 'jit' ? bestRow.fnBlocks + '/fn' : 'interpreter') + ' · ' + p.build;
    const payload = buildPayload({
      ua: navigator.userAgent, cores: navigator.hardwareConcurrency, at: new Date().toISOString(), manifest,
      deviceMemory: navigator.deviceMemory, results, preset, device: deviceId, deviceName: localStorage.labName,
      job: p.job, press: p.press, buildId: p.build, manual: !!p.manual, state: state(), deferredMs, failed,
    });
    const route = p.manual ? '/results/manual?device=' + encodeURIComponent(deviceId) : '/jobs/' + p.job + '/presses/' + p.press + '/result';
    try {
      const resp = await postJson(route, payload);
      els.status.textContent = failed ? 'Press failed: ' + failed.split('\n')[0] + ' (reported)' :
        (resp.ok ? 'Done' + (payload.tainted ? ' — TAINTED (' + payload.taintReasons.join(', ') + '), the director will not quote it' : '') + '. Sent to the lab.' :
          'Done, but sending failed (' + resp.status + ').');
    } catch (e) { els.status.textContent = 'Done, but sending failed. Is the lab up?'; }
    els.press.textContent = who + ' · build ' + p.build + ' · finished';
    running = null;
    setBusy(false);
    renderState();
    if (pending) { const n = pending; pending = null; onPress(n); }
  }

  function addRow(i, r, results) {
    els.table.hidden = false;
    const tr = document.createElement('tr');
    const bootMs = (r.boot || []).reduce((a, b) => a + b.discoverMs + b.emitMs + b.compileMs + b.instantiateMs, 0);
    const cells = [
      i + 1, r.slug, r.mode, r.grade, r.fnBlocks ?? '–',
      r.failed ? '–' : (r.wallMs / 1000).toFixed(2),
      r.nsPerInstr ? r.nsPerInstr.toFixed(2) : '–',
      r.realtime ? r.realtime.toFixed(3) + '×' : '–',
      r.coverage !== undefined ? r.coverage.toFixed(2) : '–',
      bootMs ? bootMs.toFixed(0) : '–',
      r.uartSha256 ? r.uartSha256.slice(0, 8) : '–',
      r.tainted ? r.taintReasons.join(',') : (r.wakeLock === 'active' ? 'ok' : r.wakeLock),
    ];
    tr.innerHTML = cells.map((c) => '<td>' + c + '</td>').join('');
    if (r.failed || r.trap || r.selftestError || r.tainted) tr.classList.add('bad');
    els.tbody.appendChild(tr);
    if (r.boot && r.boot.length) els.boot.textContent = r.boot.map((b) => b.line).join('\n');
    els.tail.textContent = r.failed || r.selftestError || r.trap || r.tail || '';
    const by = {};
    for (const x of results) { if (!x.realtime || x.tainted) continue; (by[x.slug + ' ' + x.grade + ' ' + x.mode + (x.mode === 'jit' ? ' fn ' + x.fnBlocks : '')] ||= []).push(x); }
    els.headlines.innerHTML = Object.entries(by).map(([key, rs]) => {
      const best = rs.reduce((a, b) => (b.realtime > a.realtime ? b : a));
      return '<div class="headline"><span class="big">' + best.realtime.toFixed(3) + '× real time</span> · ' + best.nsPerInstr.toFixed(2) + ' ns/instr · ' + key + '</div>';
    }).join('');
  }

  // --- manual Run (kept): the rig's controls over a build from the store ---
  let statusBuilds = [];
  async function loadBuilds() {
    try {
      const s = await (await api('/status')).json();
      statusBuilds = s.builds || [];
      els.build.innerHTML = statusBuilds.map((b) => '<option value="' + b.id + '">' + b.id + ' (' + b.branch + (b.dirty ? ', dirty' : '') + ' ' + b.built_at + ')</option>').join('');
      if (statusBuilds[0]) fillControls(statusBuilds[0].id);
    } catch { /* offline */ }
  }
  async function fillControls(id) {
    const m = await (await fetch('builds/' + id + '/manifest.json?t=' + Date.now(), { cache: 'no-store', headers: { 'ngrok-skip-browser-warning': '1' } })).json();
    const opts = (el, values, labels) => { el.innerHTML = values.map((v, i) => '<option value="' + v + '">' + (labels ? labels[i] : v) + '</option>').join(''); };
    const d = m.defaults || {};
    opts($('mode'), m.modeChoices || ['jit', 'interp'], ['translated', '--interpreter']);
    opts($('fnBlocks'), m.fnBlocksChoices || [32]);
    opts($('timeout'), m.timeoutChoices || ['5500ms']);
    opts($('grade'), ['t2', 't1']);
    opts($('image'), m.images.map((i) => i.slug));
    $('mode').value = d.mode || 'jit'; $('fnBlocks').value = String(d.fnBlocks || 32); $('timeout').value = d.timeout || '5500ms';
    $('image').value = m.images.some((i) => i.slug === 'render-basic') ? 'render-basic' : m.images[0].slug;
  }
  els.build.addEventListener('change', () => fillControls(els.build.value));
  els.run.addEventListener('click', () => {
    const repeats = Number($('repeats').value);
    const rows = [];
    for (let r = 0; r < repeats; r++) rows.push({ slug: $('image').value, grade: $('grade').value, mode: $('mode').value, fnBlocks: Number($('fnBlocks').value), timeout: $('timeout').value });
    onPress({ manual: true, build: els.build.value, rows });
  });
  els.runGate.addEventListener('click', () => onPress({ manual: true, build: els.build.value, rows: 'gate-rows' }));

  // --- wiring ---
  els.joinBtn.addEventListener('click', join);
  els.lockBtn.addEventListener('click', requestLock);
  loadBuilds();
  renderPresence();
  // A reload re-joins on its own (Q2) — everything but the lock, which needs
  // a tap; the board shows the button for it and rows run either way.
  if (localStorage.labJoined === '1' && localStorage.labName) {
    joined = true;
    els.join.hidden = true; els.board.hidden = false; els.manual.hidden = false;
    openStream(); postState(); renderPresence();
    els.status.textContent = 'Re-joined after a reload. Tap "Keep the screen on" if the button is showing.';
  }
}
