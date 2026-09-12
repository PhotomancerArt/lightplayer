// Shared by the lab's tests: spawn the real server on port 0 in a temp home,
// and read an SSE stream event by event.
'use strict';

import { spawn } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SERVER = path.join(path.dirname(fileURLToPath(import.meta.url)), '..', 'server.mjs');

/// Start `server.mjs` with `LAB_HOME=<home>` and `LAB_PORT=0`; resolve once it
/// prints its listen line. `env` adds to the child's environment (tests use
/// `LAB_COOLDOWN_MS` and `LAB_TICK_MS` to run the queue at test speed).
export function startServer(home, env = {}) {
  return new Promise((resolve, reject) => {
    const child = spawn(process.execPath, [SERVER], {
      env: { ...process.env, LAB_HOME: home, LAB_PORT: '0', LAB_QUIET: '1', ...env },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    let out = '', err = '';
    child.stderr.on('data', (d) => { err += d; });
    child.stdout.on('data', (d) => {
      out += d;
      const m = /listening on (http:\/\/[^\s]+)/.exec(out);
      if (m) {
        const token = fs.readFileSync(path.join(home, 'token'), 'utf8').trim();
        resolve({
          url: m[1], token, home, child,
          stop() { child.kill('SIGTERM'); },
          stderr() { return err; },
        });
      }
    });
    child.on('exit', (code) => reject(new Error('server exited ' + code + '\n' + err)));
    setTimeout(() => reject(new Error('server did not listen in 5 s\n' + err)), 5000).unref();
  });
}

/// Open an SSE stream and hand back `next(event)` — resolves with the parsed
/// `data` of the next event of that name — and `closed`, a promise that
/// settles when the stream ends (abort the signal to end it).
export async function readSse(url, signal, headers = {}) {
  const resp = await fetch(url, { signal, headers });
  if (resp.status !== 200) throw new Error('SSE open ' + resp.status);
  const reader = resp.body.getReader();
  const dec = new TextDecoder();
  const queue = [];
  const waiters = [];
  let buf = '';
  let done = false;
  const closed = (async () => {
    try {
      for (;;) {
        const { value, done: d } = await reader.read();
        if (d) break;
        buf += dec.decode(value, { stream: true });
        let i;
        while ((i = buf.indexOf('\n\n')) >= 0) {
          const block = buf.slice(0, i); buf = buf.slice(i + 2);
          let event = 'message', data = '';
          for (const line of block.split('\n')) {
            if (line.startsWith('event:')) event = line.slice(6).trim();
            else if (line.startsWith('data:')) data += line.slice(5).trim();
          }
          if (!data) continue; // a keepalive comment
          const ev = { event, data: JSON.parse(data) };
          const w = waiters.findIndex((x) => x.name === event);
          if (w >= 0) waiters.splice(w, 1)[0].resolve(ev.data);
          else queue.push(ev);
        }
      }
    } catch (e) {
      if (!(e && e.name === 'AbortError')) throw e;
    } finally {
      done = true;
      for (const w of waiters) w.reject(new Error('stream closed before ' + w.name));
    }
  })();
  return {
    closed,
    next(name, timeoutMs = 10000) {
      const i = queue.findIndex((x) => x.event === name);
      if (i >= 0) return Promise.resolve(queue.splice(i, 1)[0].data);
      if (done) return Promise.reject(new Error('stream closed before ' + name));
      return new Promise((resolve, reject) => {
        const w = { name, resolve, reject };
        waiters.push(w);
        setTimeout(() => { const j = waiters.indexOf(w); if (j >= 0) { waiters.splice(j, 1); reject(new Error('no ' + name + ' in ' + timeoutMs + ' ms')); } }, timeoutMs).unref();
      });
    },
    events() { return queue.map((x) => x.event); },
  };
}
