// Interleaved rung rows off SEVERAL staged rigs, in ONE invocation.
//
//   /opt/homebrew/bin/node scripts/emu/tier-probes/rung-rows.mjs \
//     --stage R0=target/emu-bench-web-R0 --stage R1=target/emu-bench-web-R1 \
//     --image render-basic --grade t2 --mode jit --fn-blocks 16 \
//     --timeout 5500ms --repeats 5 --json rows.json
//
// Why this exists rather than `bench-cli.mjs` per stage: a rung is a
// DIFFERENT `emu.wasm`, and `bench-cli.mjs` loads exactly one. The desk is
// shared and its load average moves by 10 between invocations, so rows from
// two invocations are not comparable (see `bench-cli.mjs`'s own note). This
// compiles every rung's module up front and then interleaves the repeats —
// `R0, R1, R2, R3, R0, R1, …` — so every rung meets the same load in the same
// order, and the best-of column is a comparison rather than a coincidence.
//
// The GUEST bytes come from the FIRST stage for every rung: the rungs differ
// in the emulator, never in the firmware, and reading one ELF makes that a
// property of the runner instead of a thing to check.
//
// Every row carries the four transcript surfaces the invariant is proved on
// (UART0, frames, trap log — and the pin log when the rig is asked for one),
// the `stopped after` cycles and instret, and the decoded frame count, which
// is the USER-facing number: frames per emulated second is what "what fps
// will this pattern get on hardware" means, and it must not move on a lever
// that claims to keep the guest exact.
import { readFileSync, writeFileSync } from 'node:fs';
import { loadavg } from 'node:os';
import { join } from 'node:path';

function parseArgs(argv) {
  const o = {
    stages: [], image: 'render-basic', grade: 't2', mode: 'jit', fnBlocks: 16,
    timeout: '5500ms', wallTimeout: 900, repeats: 5, json: null,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const next = () => { const v = argv[++i]; if (v === undefined) throw new Error(a + ' needs a value'); return v; };
    if (a === '--stage') {
      const kv = next();
      const j = kv.indexOf('=');
      if (j < 0) throw new Error('--stage wants LABEL=dir, got ' + kv);
      o.stages.push({ label: kv.slice(0, j), dir: kv.slice(j + 1) });
    } else if (a === '--image') o.image = next();
    else if (a === '--grade') o.grade = next();
    else if (a === '--mode') o.mode = next();
    else if (a === '--fn-blocks') o.fnBlocks = Number(next());
    else if (a === '--timeout') o.timeout = next();
    else if (a === '--wall-timeout') o.wallTimeout = Number(next());
    else if (a === '--repeats') o.repeats = Number(next());
    else if (a === '--json') o.json = next();
    else throw new Error('unknown option ' + a);
  }
  if (!o.stages.length) throw new Error('at least one --stage LABEL=dir');
  return o;
}

function engineName() {
  if (typeof globalThis.Bun !== 'undefined') return 'bun/JavaScriptCore ' + globalThis.Bun.version;
  if (globalThis.process?.versions?.v8) return 'node/V8 ' + process.versions.node + ' (v8 ' + process.versions.v8 + ')';
  return 'unknown';
}

const o = parseArgs(process.argv.slice(2));

// One `bench-run.js` for every rung — the rig is the same rig; only the
// module under it changes.
const { runOnce } = await import(new URL('file://' + join(process.cwd(), o.stages[0].dir, 'bench-run.js')));

const manifest0 = JSON.parse(readFileSync(join(o.stages[0].dir, 'manifest.json'), 'utf8'));
const image = manifest0.images.find((i) => i.slug === o.image);
if (!image) throw new Error('no image ' + o.image + ' in ' + o.stages[0].dir);
const elfBytes = new Uint8Array(readFileSync(join(o.stages[0].dir, image.elf)));

for (const s of o.stages) {
  const bytes = readFileSync(join(s.dir, 'emu.wasm'));
  s.compiled = new WebAssembly.Module(bytes);
  s.wasmBytes = bytes.length;
  s.build = JSON.parse(readFileSync(join(s.dir, 'manifest.json'), 'utf8')).build;
}

console.log('engine       ' + engineName());
console.log('image        ' + o.image + ' ' + o.grade + ' ' + o.mode + (o.mode === 'jit' ? ' ' + o.fnBlocks + '/fn' : '') + ' ' + o.timeout);
console.log('repeats      ' + o.repeats + ', interleaved, one invocation');
for (const s of o.stages) console.log('  ' + s.label.padEnd(4) + ' ' + s.build.short + (s.build.dirty ? '-dirty' : '') + '  ' + s.wasmBytes + ' B  ' + s.dir);
console.log('');

const head = ['rung', 'rep', 'wall s', 'real time', 'cycles', 'instret', 'frames', 'load', 'uart', 'frames sha', 'trap'];
const w = [6, 4, 9, 10, 12, 12, 7, 6, 18, 18, 18];
console.log(head.map((h, i) => (i === 0 ? h.padEnd(w[i]) : h.padStart(w[i]))).join(' '));
console.log(head.map((_, i) => '-'.repeat(w[i])).join(' '));

const rows = [];
for (let rep = 1; rep <= o.repeats; rep++) {
  for (const s of o.stages) {
    const r = await runOnce({
      compiled: s.compiled, image, elfBytes, grade: o.grade, mode: o.mode,
      fnBlocks: o.mode === 'jit' ? o.fnBlocks : null,
      timeout: o.timeout, wallTimeout: o.wallTimeout, exitOn: false,
      extraArgs: [], env: {}, keepText: true,
    });
    // The user-facing number: how many frames the pattern actually produced.
    // `pin gpio18: 256 frames, 256 complete, 0 errors, 241 leds`
    const fm = /^pin gpio(\d+): (\d+) frames, (\d+) complete, (\d+) errors, (\d+) leds/m
      .exec((r.fullText ?? '').split('\n').filter((l) => !l.startsWith('pin gpio') || !/: 0 frames/.test(l)).join('\n'));
    r.framePad = fm ? Number(fm[1]) : null;
    r.frames = fm ? Number(fm[2]) : null;
    r.framesComplete = fm ? Number(fm[3]) : null;
    r.frameErrors = fm ? Number(fm[4]) : null;
    r.leds = fm ? Number(fm[5]) : null;
    delete r.fullText;
    r.rung = s.label;
    r.rep = rep;
    r.loadavg = loadavg()[0];
    rows.push(r);
    console.log([
      s.label.padEnd(w[0]),
      String(rep).padStart(w[1]),
      (r.wallMs / 1000).toFixed(2).padStart(w[2]),
      (r.realtime ? r.realtime.toFixed(3) + 'x' : '-').padStart(w[3]),
      String(r.cycles ?? '-').padStart(w[4]),
      String(r.instr ?? '-').padStart(w[5]),
      String(r.frames ?? '-').padStart(w[6]),
      r.loadavg.toFixed(1).padStart(w[7]),
      (r.uartSha256 || '-').slice(0, 16).padStart(w[8]),
      (r.framesSha256 || '-').slice(0, 16).padStart(w[9]),
      (r.trapSha256 || '-').slice(0, 16).padStart(w[10]),
    ].join(' '));
  }
}

console.log('\nbest of ' + o.repeats + ':');
const b = ['rung', 'best s', 'real time', 'vs R0', 'instret', 'frames', 'uart', 'frames sha', 'trap'];
const bw = [6, 9, 10, 8, 12, 7, 18, 18, 18];
console.log(b.map((h, i) => (i === 0 ? h.padEnd(bw[i]) : h.padStart(bw[i]))).join(' '));
console.log(b.map((_, i) => '-'.repeat(bw[i])).join(' '));
const bests = {};
for (const s of o.stages) {
  const mine = rows.filter((r) => r.rung === s.label);
  bests[s.label] = mine.reduce((a, c) => (c.wallMs < a.wallMs ? c : a));
}
const ref = bests[o.stages[0].label];
for (const s of o.stages) {
  const r = bests[s.label];
  console.log([
    s.label.padEnd(bw[0]),
    (r.wallMs / 1000).toFixed(2).padStart(bw[1]),
    (r.realtime ? r.realtime.toFixed(3) + 'x' : '-').padStart(bw[2]),
    (ref.wallMs / r.wallMs).toFixed(3) + 'x'.padStart(0),
    String(r.instr ?? '-').padStart(bw[4]),
    String(r.frames ?? '-').padStart(bw[5]),
    (r.uartSha256 || '-').slice(0, 16).padStart(bw[6]),
    (r.framesSha256 || '-').slice(0, 16).padStart(bw[7]),
    (r.trapSha256 || '-').slice(0, 16).padStart(bw[8]),
  ].join(' '));
}

if (o.json) {
  writeFileSync(o.json, JSON.stringify({
    engine: engineName(), at: new Date().toISOString(),
    stages: o.stages.map((s) => ({ label: s.label, dir: s.dir, build: s.build, wasmBytes: s.wasmBytes })),
    image: o.image, grade: o.grade, mode: o.mode, fnBlocks: o.fnBlocks, timeout: o.timeout,
    repeats: o.repeats, rows,
  }, null, 2));
  console.log('\nwrote ' + o.json);
}
