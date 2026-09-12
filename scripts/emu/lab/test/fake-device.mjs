// A fake page client: joins as a device, posts state, and answers every
// `press` after `pressMs` with a synthetic payload built from a table of
// numbers, so the report is checkable exactly (D21).
'use strict';

import { readSse } from './helpers.mjs';

/// `numbers(build, pressN)` returns the rows for that press:
/// `[{key: 'render-basic/t2/jit/8', realtime: 0.863}, …]`. `taintOn` and
/// `failOn` are sets of global press numbers.
export async function fakeDevice(lab, opts = {}) {
  const id = opts.id || 'fake-' + Math.random().toString(16).slice(2, 8);
  const name = opts.name || id;
  const pressMs = opts.pressMs ?? 20;
  const taintOn = opts.taintOn || new Set();
  const failOn = opts.failOn || new Set();
  const deferOn = opts.deferOn || new Set();
  const numbers = opts.numbers || (() => [{ key: 'render-basic/t2/jit/8', realtime: 1.0 }, { key: 'render-basic/t2/interp', realtime: 0.5 }]);
  const ac = new AbortController();
  const headers = { Authorization: 'Bearer ' + lab.token, 'Content-Type': 'application/json' };
  const post = (path, body) => fetch(lab.url + path, { method: 'POST', headers, body: JSON.stringify(body) });

  const log = [];        // {press, build, sentAt, answeredAt}
  const sse = await readSse(lab.url + '/events?device=' + id + '&t=' + lab.token, ac.signal);
  await sse.next('hello');
  const state = async (visibility = 'visible', wakeLock = 'active') => {
    await post('/devices/' + id + '/state', { name, ua: 'fake-ua/' + id, cores: 4, deviceMemory: null, visibility, hasFocus: true, wakeLock });
  };
  await state();

  const answered = [];
  let stopped = false;
  const pump = (async () => {
    while (!stopped) {
      let p;
      try { p = await sse.next('press', 60000); } catch { return; }
      const sentAt = Date.now();
      if (deferOn.has(p.press)) await post('/jobs/' + p.job + '/presses/' + p.press + '/deferred', { reason: 'hidden' });
      await new Promise((r) => setTimeout(r, pressMs));
      const rows = numbers(p.build, p.press).map((r) => {
        const [slug, grade, mode, fn] = r.key.split('/');
        return { slug, grade, mode, fnBlocks: mode === 'jit' ? Number(fn) : null, timeout: '5500ms', realtime: r.realtime, nsPerInstr: r.nsPerInstr ?? 10, wallMs: 5000, uartSha256: r.uartSha256 ?? '2407828f80684331deadbeef', tainted: taintOn.has(p.press), taintReasons: taintOn.has(p.press) ? ['hidden'] : [] };
      });
      const failed = failOn.has(p.press);
      const payload = {
        ua: 'fake-ua/' + id, cores: 4, at: new Date().toISOString(), build: { short: p.build, id: p.build }, deviceMemory: null, preset: p.rows === 'gate-rows' ? 'gate-rows' : undefined,
        results: failed ? [] : rows,
        device: id, deviceName: name, job: p.job, press: p.press, buildId: p.build, manual: false,
        visibility: 'visible', hasFocus: true, wakeLock: 'active', tainted: taintOn.has(p.press), taintReasons: taintOn.has(p.press) ? ['hidden'] : [], deferredMs: 0,
      };
      if (failed) payload.failed = 'Worker died: synthetic';
      // `beforeAnswer` may throw to mean "do not answer this press" (the
      // restart and lost tests use it as the device that goes quiet).
      if (opts.beforeAnswer) { try { await opts.beforeAnswer(p); } catch { continue; } }
      let r;
      try { r = await post('/jobs/' + p.job + '/presses/' + p.press + '/result', payload); } catch { continue; }
      answered.push({ job: p.job, press: p.press, build: p.build, sentAt, answeredAt: Date.now(), status: r.status });
      log.push(answered[answered.length - 1]);
    }
  })();

  return {
    id, name, answered, sse,
    state,
    async stop() { stopped = true; ac.abort(); await sse.closed; await pump.catch(() => {}); },
  };
}
