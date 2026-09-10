#!/usr/bin/env node
// M7 P2 browser-seam probe -- headless runner for `node` and `bun`.
//
//   node scripts/emu/bench-web/probe/run-headless.mjs [out.json]
//   bun  scripts/emu/bench-web/probe/run-headless.mjs [out.json]
//
// Spawns `worker-entry.mjs` in a real `node:worker_threads` Worker (both
// engines implement it) so S1-S7 run inside a dedicated worker thread, the
// same shape the product's emulator runs in (JD13). Prints the engine/
// version and the full result JSON to stdout, and writes it to `out.json`
// if given.
import { Worker } from 'node:worker_threads';
import { fileURLToPath } from 'node:url';
import { writeFileSync } from 'node:fs';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const outPath = process.argv[2];

const engine = typeof Bun !== 'undefined'
  ? { runtime: 'bun', runtimeVersion: Bun.version, jsEngine: 'JavaScriptCore' }
  : { runtime: 'node', runtimeVersion: process.version, jsEngine: 'V8' };

const worker = new Worker(path.join(here, 'worker-entry.mjs'));
worker.once('message', (result) => {
  const payload = { engine, ...result };
  console.log(JSON.stringify(payload, null, 2));
  if (outPath) writeFileSync(outPath, JSON.stringify(payload, null, 2));
  worker.terminate();
});
worker.once('error', (err) => {
  console.error('worker-entry.mjs error:', err);
  process.exitCode = 1;
});
