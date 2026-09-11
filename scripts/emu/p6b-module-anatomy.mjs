#!/usr/bin/env node
// M7 P6b: what one emitted module is actually made of, per function.
//
//   node scripts/emu/p6b-module-anatomy.mjs <module.wasm> [...]
//
// Reads the code section directly rather than shelling out to `wasm-tools`,
// because the only thing wanted here is the two numbers per function body that
// a printer would bury in a hundred megabytes of text:
//
//   - **declared locals**, both the count of `(n, type)` groups and the total
//     number of local slots past the parameters. H1 asks whether a
//     sub-dispatcher's locals grow with the number of blocks it holds — wasm
//     zeroes every local at function entry, and a cross-function hop is a
//     function entry, so a per-block local set would be paid 31 million times
//     on `render-basic`.
//   - **body bytes**, so the outer selector can be told apart from the
//     sub-dispatchers. `emit_module` reports `max_body_bytes` over the
//     sub-dispatchers ONLY; the selector is not in that maximum and at small
//     blocks-per-function it is by far the largest function in the module.
//
// The selector is the LAST function the module defines (`emit_module` pushes
// the bodies, then the selector), so it is reported on its own line.
import { readFileSync } from 'node:fs';

const paths = process.argv.slice(2);
if (!paths.length) {
  console.error('usage: p6b-module-anatomy.mjs <module.wasm> [...]');
  process.exit(2);
}

const TYPE = { 0x7f: 'i32', 0x7e: 'i64', 0x7d: 'f32', 0x7c: 'f64', 0x7b: 'v128' };

/** LEB128 unsigned, from `at`. Returns `[value, next]`. */
function uleb(b, at) {
  let v = 0, shift = 0;
  for (;;) {
    const byte = b[at++];
    v += (byte & 0x7f) * 2 ** shift;
    shift += 7;
    if ((byte & 0x80) === 0) return [v, at];
  }
}

function anatomy(path) {
  const b = readFileSync(path);
  if (b.readUInt32LE(0) !== 0x6d736100) throw new Error(path + ': not a wasm module');
  let at = 8;
  let code = null;
  const sections = [];
  while (at < b.length) {
    const id = b[at++];
    let size;
    [size, at] = uleb(b, at);
    sections.push({ id, size });
    if (id === 10) code = { at, size };
    at += size;
  }
  if (!code) throw new Error(path + ': no code section');

  let p = code.at;
  let count;
  [count, p] = uleb(b, p);
  const fns = [];
  for (let i = 0; i < count; i++) {
    let bodySize;
    [bodySize, p] = uleb(b, p);
    const bodyStart = p;
    let groups;
    let q;
    [groups, q] = uleb(b, p);
    let slots = 0;
    const byType = {};
    for (let g = 0; g < groups; g++) {
      let n;
      [n, q] = uleb(b, q);
      const t = TYPE[b[q++]] ?? ('0x' + b[q - 1].toString(16));
      slots += n;
      byType[t] = (byType[t] ?? 0) + n;
    }
    fns.push({ bodyBytes: bodySize, localGroups: groups, localSlots: slots, byType });
    p = bodyStart + bodySize;
  }
  return { path, bytes: b.length, fns, sections };
}

const num = (n) => n.toLocaleString('en-US');

for (const path of paths) {
  const a = anatomy(path);
  const bodies = a.fns.slice(0, -1);
  const selector = a.fns[a.fns.length - 1];
  const slots = bodies.map((f) => f.localSlots);
  const bytes = bodies.map((f) => f.bodyBytes);
  const sum = (xs) => xs.reduce((x, y) => x + y, 0);
  const distinct = [...new Set(slots)].sort((x, y) => x - y);
  console.log(path);
  console.log(`  module              ${num(a.bytes)} B, ${num(a.fns.length)} functions ` +
              `(${num(bodies.length)} sub-dispatchers + 1 selector)`);
  console.log(`  sub-dispatcher locals  distinct slot counts: ${distinct.join(', ')}` +
              `   (groups: ${[...new Set(bodies.map((f) => f.localGroups))].sort((x, y) => x - y).join(', ')})`);
  console.log(`  sub-dispatcher locals  by type on the first: ` +
              Object.entries(bodies[0].byType).map(([t, n]) => `${n} x ${t}`).join(', '));
  console.log(`  sub-dispatcher bytes   min ${num(Math.min(...bytes))}  mean ` +
              `${num(Math.round(sum(bytes) / bytes.length))}  max ${num(Math.max(...bytes))}  ` +
              `total ${num(sum(bytes))}`);
  console.log(`  SELECTOR               ${num(selector.bodyBytes)} B, ${selector.localSlots} locals ` +
              `(${(100 * selector.bodyBytes / a.bytes).toFixed(1)} % of the module)`);
  console.log('');
}
