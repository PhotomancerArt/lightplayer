// M7 P6: drive `bench-web/worker.js` the way `index.html` drives it, without a
// browser.
//
//   node scripts/emu/p6-worker-harness.mjs <stage-dir> [<slug> <grade> <mode> <fnBlocks> <timeout>]
//
// `bench-cli.mjs` imports `bench-run.js` directly, so it proves the seam and
// the readouts but **not** the worker: not the module-worker import graph, not
// the `fetch` of `emu.wasm` and the ELF, and not the `loaded`/`progress`/
// `result`/`done` message protocol the page is written against. A
// worktree-isolated agent cannot serve the stage to a real browser (see the
// phase report), so this is what can be checked without one: the worker file
// itself, imported and driven, with `self`, `fetch` and `postMessage` supplied
// from the filesystem.
//
// It is not a substitute for a browser run and does not pretend to be one —
// the engines it proves are `node`'s and `bun`'s, which `bench-cli.mjs`
// already measures. What it proves is that `worker.js` is not broken.
import { readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

const [stage = 'target/emu-bench-web', slug = 'render-basic', grade = 't2',
       mode = 'jit', fnBlocks = '64', timeout = '20ms'] = process.argv.slice(2);

const dir = resolve(stage);
const manifest = JSON.parse(readFileSync(join(dir, 'manifest.json'), 'utf8'));

// The worker's global surface, in the order `worker.js` touches it.
globalThis.self = globalThis;
globalThis.self.location = { href: 'http://localhost/worker.js?v=' + manifest.build.short };
globalThis.onmessage = null;                    // so the module's assignment has a target
globalThis.fetch = async (url) => {
  const name = String(url).split('?')[0];
  const bytes = readFileSync(join(dir, name));
  return { arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) };
};

let done = false, failed = null;
const seen = [];
globalThis.postMessage = (m) => {
  seen.push(m.type);
  if (m.type === 'loaded') console.log(`loaded: emu.wasm ${m.wasmBytes} B compiled in ${m.compileMs.toFixed(1)} ms`);
  if (m.type === 'progress') console.log(`progress: ${m.i + 1}/${m.n} ${m.step.slug} ${m.step.grade} ${m.step.mode}`);
  if (m.type === 'result') {
    const r = m.r;
    if (r.failed) { failed = r.failed; return; }
    console.log(`result: ${r.slug} ${r.grade} ${r.mode} fn=${r.fnBlocks} ` +
      `wall ${(r.wallMs / 1000).toFixed(2)} s, ${r.nsPerInstr ? r.nsPerInstr.toFixed(2) : '-'} ns/instr, ` +
      `cover ${r.coverage !== undefined ? r.coverage.toFixed(2) : '-'} %, ` +
      `selftest ${r.selftest ? 'slot ' + r.selftest.slot + ' ok' : r.selftestError || 'n/a'}, ` +
      `uart ${(r.uartSha256 || '-').slice(0, 16)}`);
    console.log(`        stopped: ${r.stoppedLine || '(none)'}`);
    for (const b of r.boot || []) console.log(`        ${b.line}`);
  }
  if (m.type === 'done') done = true;
  if (m.type === 'error') failed = m.message;
};

await import(pathToFileURL(join(dir, 'worker.js')).href);

const image = manifest.images.find((i) => i.slug === slug);
if (!image) throw new Error('no image ' + slug + ' in ' + stage);
await globalThis.onmessage({
  data: {
    images: manifest.images,
    plan: [{ slug, grade, mode, fnBlocks: Number(fnBlocks), timeout, wallTimeout: 600, exitOn: false }],
  },
});

if (failed) { console.error('FAILED: ' + failed); process.exit(1); }
if (!done) { console.error('FAILED: the worker never said done; saw ' + seen.join(',')); process.exit(1); }
console.log('worker protocol: ' + seen.join(' -> '));
