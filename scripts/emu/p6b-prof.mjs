#!/usr/bin/env node
// M7 P6b (H4 cross-check): read a `node --cpu-prof` profile of a bench row and
// say how much of the run was inside the translated module, how much was
// inside the emulator's own wasm, and how much was JavaScript.
//
//   node --cpu-prof --cpu-prof-dir=target/p6b/prof \
//     target/emu-bench-web/bench-cli.mjs --stage target/emu-bench-web \
//     --image render-basic --grade t2 --mode jit --fn-blocks 64 --timeout 5500ms
//   node scripts/emu/p6b-prof.mjs target/p6b/prof/CPU.*.cpuprofile
//
// V8 gives a wasm frame a `url` of `wasm://wasm/<hash>` and a name of
// `<hash>-<index>` or `wasm-function[<index>]`, so the two modules in the
// process are told apart by their url and NOT by any name this repository
// chooses. The emulator's own module is the one with far more distinct
// functions attributed to it and the one that owns `_start`; the translated
// module is the other. Both are reported by url so the attribution can be
// checked rather than trusted.
import { readFileSync } from 'node:fs';

const path = process.argv[2];
if (!path) { console.error('usage: p6b-prof.mjs <file.cpuprofile>'); process.exit(2); }
const p = JSON.parse(readFileSync(path, 'utf8'));

const byId = new Map(p.nodes.map((n) => [n.id, n]));
// Self time, from the sample stream and the delta stream, because `hitCount`
// is samples and the deltas are what those samples actually cost.
const self = new Map();
let total = 0;
for (let i = 0; i < p.samples.length; i++) {
  const dt = p.timeDeltas[i] ?? 0;
  total += dt;
  self.set(p.samples[i], (self.get(p.samples[i]) ?? 0) + dt);
}

/** Which bucket a frame belongs to. */
function bucket(n) {
  const f = n.callFrame;
  const url = f.url || '';
  if (url.startsWith('wasm://')) return 'wasm ' + url;
  if (f.functionName === '(program)' || f.functionName === '(idle)' || f.functionName === '(garbage collector)') {
    return f.functionName;
  }
  if (url.includes('node:')) return 'node internals';
  if (url) return 'JS ' + url.split('/').pop();
  return 'JS ' + (f.functionName || '(anonymous)');
}

const buckets = new Map();
const wasmFns = new Map();
for (const [id, us] of self) {
  const n = byId.get(id);
  if (!n) continue;
  const b = bucket(n);
  buckets.set(b, (buckets.get(b) ?? 0) + us);
  if (b.startsWith('wasm ')) {
    const k = b + ' :: ' + n.callFrame.functionName;
    wasmFns.set(k, (wasmFns.get(k) ?? 0) + us);
  }
}

const ms = (us) => (us / 1000).toFixed(1);
const pct = (us) => ((100 * us) / total).toFixed(2);

console.log(`profile   ${path}`);
console.log(`samples   ${p.samples.length}, ${ms(total)} ms of wall clock attributed`);
console.log('');
console.log('bucket'.padEnd(56) + 'ms'.padStart(10) + '%'.padStart(8));
console.log('-'.repeat(74));
for (const [b, us] of [...buckets].sort((a, b2) => b2[1] - a[1])) {
  console.log(b.slice(0, 56).padEnd(56) + ms(us).padStart(10) + pct(us).padStart(8));
}

console.log('');
console.log('the ten hottest wasm functions');
console.log('-'.repeat(74));
for (const [k, us] of [...wasmFns].sort((a, b2) => b2[1] - a[1]).slice(0, 10)) {
  console.log(k.slice(0, 56).padEnd(56) + ms(us).padStart(10) + pct(us).padStart(8));
}
