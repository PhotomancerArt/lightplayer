// Run the bench rig's rows in `bun` or `node`, off the same staged directory
// the phone loads.
//
//   bun  scripts/emu/bench-web/bench-cli.mjs --stage target/emu-bench-web [options]
//   node scripts/emu/bench-web/bench-cli.mjs --stage target/emu-bench-web [options]
//
//   --image <slug>        repeatable; default: render-basic render-rocaille
//   --grade <t1|t2>       repeatable; default: t1 t2
//   --mode <jit|interp>   repeatable; default: jit interp
//   --fn-blocks <N>       repeatable; default: 32   (JD26's knob; DD20's default)
//   --timeout <spec>      the emulated bound; default 5500ms
//   --wall-timeout <s>    default 600
//   --exit-on             also pass the image's `--exit-on` marker
//   --json <path>         write the rows out as JSON as well as printing them
//   --tail                print the last lines of each run's own output
//   --arg <flag>          repeatable; passed straight through to the
//                         emulator's own argv, after everything above
//   --dump <dir>          write each run's WHOLE stdout+stderr there, as
//                         <image>-<grade>-<mode>-<fn>.log. The bench table
//                         shows eight lines of tail; a census is sixty.
//   --env <NAME=value>    repeatable; the module's WASI environment. P6c uses
//                         it for `LP_EMU_JIT_MMIO_CENSUS=1`, which is the only
//                         way to reach an environment-gated diagnostic inside
//                         a wasm row. The page never sets one.
//
// JD19: every generated-code A/B is measured in BOTH engines before it reaches
// a conclusion, and `bun` is JavaScriptCore — the phone's family — while `node`
// is V8. This is the half of that which does not need a device.
//
// It imports `bench-run.js`, which imports `jit-host.js`, which is the module
// Studio's worker will import too (JD25). A row taken here and a row taken on
// the phone go through the same code.
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { loadavg } from 'node:os';
import { join } from 'node:path';

import { runOnce } from './bench-run.js';

function parseArgs(argv) {
  const o = {
    stage: 'target/emu-bench-web', images: [], grades: [], modes: [], fnBlocks: [],
    timeout: '5500ms', wallTimeout: 600, exitOn: false, json: null, tail: false,
    extraArgs: [], env: {}, dump: null,
  };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    const next = () => { const v = argv[++i]; if (v === undefined) throw new Error(a + ' needs a value'); return v; };
    if (a === '--stage') o.stage = next();
    else if (a === '--image') o.images.push(next());
    else if (a === '--grade') o.grades.push(next());
    else if (a === '--mode') o.modes.push(next());
    else if (a === '--fn-blocks') o.fnBlocks.push(Number(next()));
    else if (a === '--timeout') o.timeout = next();
    else if (a === '--wall-timeout') o.wallTimeout = Number(next());
    else if (a === '--exit-on') o.exitOn = true;
    else if (a === '--json') o.json = next();
    else if (a === '--tail') o.tail = true;
    else if (a === '--arg') o.extraArgs.push(next());
    else if (a === '--dump') o.dump = next();
    else if (a === '--env') { const kv = next(); const i2 = kv.indexOf('='); if (i2 < 0) throw new Error('--env wants NAME=value, got ' + kv); o.env[kv.slice(0, i2)] = kv.slice(i2 + 1); }
    else if (a === '-h' || a === '--help') { console.log(readFileSync(new URL(import.meta.url)).toString().split('\n').filter((l) => l.startsWith('//')).join('\n')); process.exit(0); }
    else throw new Error('unknown option ' + a);
  }
  if (!o.images.length) o.images = ['render-basic', 'render-rocaille'];
  if (!o.grades.length) o.grades = ['t1', 't2'];
  if (!o.modes.length) o.modes = ['jit', 'interp'];
  if (!o.fnBlocks.length) o.fnBlocks = [32];
  return o;
}

/// Which engine this is, said by the runtime rather than guessed from a version
/// string: a row that does not name its engine is not a JD19 row.
function engineName() {
  if (typeof globalThis.Bun !== 'undefined') return 'bun/JavaScriptCore ' + globalThis.Bun.version;
  if (globalThis.process && process.versions && process.versions.v8) return 'node/V8 ' + process.versions.node + ' (v8 ' + process.versions.v8 + ')';
  return 'unknown';
}

const o = parseArgs(process.argv.slice(2));
const manifest = JSON.parse(readFileSync(join(o.stage, 'manifest.json'), 'utf8'));
const bySlug = Object.fromEntries(manifest.images.map((i) => [i.slug, i]));

const wasmBytes = readFileSync(join(o.stage, 'emu.wasm'));
const t0 = performance.now();
const compiled = new WebAssembly.Module(wasmBytes);
const emuCompileMs = performance.now() - t0;

console.log('engine       ' + engineName());
console.log('build        ' + manifest.build.short + ' (' + manifest.build.branch + (manifest.build.dirty ? ', dirty' : ') ') + ')');
console.log('emu.wasm     ' + wasmBytes.length + ' B, compiled in ' + emuCompileMs.toFixed(1) + ' ms');
console.log('');

// The load average rides every row. This desk is shared — P5's sizing table
// carries "desk load average 11–17" for the same reason — and a wall-clock
// number without one is not comparable to any other wall-clock number. Rows
// taken in ONE invocation are comparable to each other whatever the load;
// rows from two invocations are not, unless the loads match.
const head = ['image', 'grade', 'mode', 'fn', 'wall s', 'ns/instr', 'real time', 'cover %', 'stay', 'esc %', 'load', 'uart sha256'];
const w = [16, 5, 6, 5, 9, 9, 10, 8, 8, 7, 6, 18];
const row = (cells) => cells.map((c, i) => String(c).padStart(i === 0 ? -w[i] : w[i]).slice(0, Math.max(w[i], String(c).length))).join(' ');
console.log(head.map((h, i) => (i === 0 ? h.padEnd(w[i]) : h.padStart(w[i]))).join(' '));
console.log(head.map((_, i) => '-'.repeat(w[i])).join(' '));

const rows = [];
for (const slug of o.images) {
  const image = bySlug[slug];
  if (!image) throw new Error('no image ' + slug + ' in ' + o.stage + '/manifest.json');
  const elfBytes = new Uint8Array(readFileSync(join(o.stage, image.elf)));
  for (const grade of o.grades) {
    for (const mode of o.modes) {
      for (const fnBlocks of mode === 'jit' ? o.fnBlocks : [null]) {
        const r = await runOnce({
          compiled, image, elfBytes, grade, mode, fnBlocks,
          timeout: o.timeout, wallTimeout: o.wallTimeout, exitOn: o.exitOn,
          extraArgs: o.extraArgs, env: o.env, keepText: !!o.dump,
        });
        if (o.dump) {
          mkdirSync(o.dump, { recursive: true });
          const p2 = join(o.dump, `${slug}-${grade}-${mode}-${fnBlocks ?? 'x'}.log`);
          writeFileSync(p2, r.fullText ?? '');
          delete r.fullText;
          console.log('  -> ' + p2);
        }
        r.engine = engineName();
        r.loadavg = loadavg()[0];
        rows.push(r);
        console.log([
          slug.padEnd(w[0]),
          grade.padStart(w[1]),
          mode.padStart(w[2]),
          String(fnBlocks ?? '-').padStart(w[3]),
          (r.wallMs / 1000).toFixed(2).padStart(w[4]),
          (r.nsPerInstr ? r.nsPerInstr.toFixed(2) : '-').padStart(w[5]),
          (r.realtime ? r.realtime.toFixed(3) + 'x' : '-').padStart(w[6]),
          (r.coverage !== undefined ? r.coverage.toFixed(2) : '-').padStart(w[7]),
          (r.meanStay ? r.meanStay.toFixed(1) : '-').padStart(w[8]),
          (r.escapeRate !== undefined ? (100 * r.escapeRate).toFixed(3) : '-').padStart(w[9]),
          r.loadavg.toFixed(1).padStart(w[10]),
          (r.uartSha256 || '-').slice(0, 16).padStart(w[11]),
        ].join(' '));
        if (r.selftestError) console.log('  !! table selftest: ' + r.selftestError);
        if (r.trap) console.log('  !! trap: ' + r.trap.split('\n')[0]);
        if (o.tail) console.log(r.tail.split('\n').map((l) => '  | ' + l).join('\n'));
      }
    }
  }
}

if (o.json) {
  writeFileSync(o.json, JSON.stringify({
    engine: engineName(), build: manifest.build, emuWasmBytes: wasmBytes.length, emuCompileMs,
    at: new Date().toISOString(), rows,
  }, null, 2));
  console.log('\nwrote ' + o.json);
}
