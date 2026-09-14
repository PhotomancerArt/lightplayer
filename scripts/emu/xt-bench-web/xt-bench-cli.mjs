// Run the classic ESP32's bench rows in `bun` or `node`, off the same staged
// directory a phone would load.
//
//   bun  target/emu-xt-bench-web/xt-bench-cli.mjs --stage target/emu-xt-bench-web [options]
//   /opt/homebrew/bin/node target/emu-xt-bench-web/xt-bench-cli.mjs --stage target/emu-xt-bench-web [options]
//
//   --image <slug>        repeatable; default: render-loop
//   --grade <t1>          repeatable; default: t1 (the only grade this machine defines)
//   --mode <jit|interp>   repeatable; default: jit interp
//   --fn-blocks <N>       repeatable; default: 8 16
//   --rows <spec,…>       explicit rows, `slug:grade:mode[:fnBlocks]`, in order.
//                         Takes precedence over the four above, and is what an
//                         INTERLEAVED table needs: the product order runs all
//                         of one mode and then all of the other, which puts
//                         the two legs of a ratio ten minutes apart.
//   --best-of <N>         repeat the whole row sequence N times and report the
//                         best wall time per row [1]. Best-of, not mean: a
//                         slower run is a run that met something else on this
//                         desk, and the fastest is the only one with a
//                         defensible claim about the code.
//   --timeout <spec>      the emulated bound; default 5500ms
//   --wall-timeout <s>    default 600
//   --exit-on             also pass the image's `--exit-on` marker
//   --json <path>         write the rows out as JSON as well as printing them
//   --tail                print the last lines of each run's own output
//   --arg <flag>          repeatable; passed straight through to the emulator's
//                         own argv, after everything above
//   --dump <dir>          write each run's WHOLE stdout+stderr there
//   --env <NAME=value>    repeatable; the module's WASI environment (the only
//                         way to reach an environment-gated diagnostic —
//                         `LP_EMU_XT_JIT_EXITS`, `LP_EMU_XT_JIT_MMIO_CENSUS` —
//                         inside a wasm row)
//
// **A twin of `scripts/emu/bench-web/bench-cli.mjs`, not an edit of it** — see
// the header of `xt-bench-run.js` for the four things that differ and why the
// C6's files are never edited by this plan.
//
// Two things this one has that the C6's does not, and both are the milestone's
// protocol rather than a convenience:
//
// - `--rows` and `--best-of`, because acceptance 3 reads **one invocation,
//   interleaved, best of five**. Rows from two invocations never share a
//   table, so the interleaving and the repetition have to happen inside one.
// - `sysctl -n vm.loadavg` for the load column. `os.loadavg()` under `bun` is
//   always 0 — a fiction, not a reading — and a wall-clock number without a
//   load is not comparable to any other wall-clock number.
//
// JD19: every generated-code A/B is measured in BOTH engines before it reaches
// a conclusion. `bun` is JavaScriptCore — the phone's family — and `node` is
// V8. This is the half of that which does not need a device.
import { execFileSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

import { runOnce } from './xt-bench-run.js';

function parseArgs(argv) {
  const o = {
    stage: 'target/emu-xt-bench-web', images: [], grades: [], modes: [], fnBlocks: [], rows: null,
    bestOf: 1, timeout: '5500ms', wallTimeout: 600, exitOn: false, json: null, tail: false,
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
    else if (a === '--rows') o.rows = next().split(',').map((s) => s.trim()).filter(Boolean);
    else if (a === '--best-of') o.bestOf = Number(next());
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
  if (!o.images.length) o.images = ['render-loop'];
  if (!o.grades.length) o.grades = ['t1'];
  if (!o.modes.length) o.modes = ['jit', 'interp'];
  if (!o.fnBlocks.length) o.fnBlocks = [8, 16];
  if (!Number.isFinite(o.bestOf) || o.bestOf < 1) throw new Error('--best-of wants a count of 1 or more');
  return o;
}

/// `slug:grade:mode[:fnBlocks]` — the order the rows are taken in.
function rowsFromSpecs(specs, o) {
  return specs.map((s) => {
    const [slug, grade, mode, fn] = s.split(':');
    if (!slug || !grade || !mode) throw new Error('--rows wants slug:grade:mode[:fnBlocks], got ' + s);
    if (mode !== 'jit' && mode !== 'interp') throw new Error('--rows mode is jit or interp, got ' + mode);
    return { slug, grade, mode, fnBlocks: mode === 'jit' ? Number(fn || 8) : null };
  });
}

function rowsFromProduct(o) {
  const rows = [];
  for (const slug of o.images) {
    for (const grade of o.grades) {
      for (const mode of o.modes) {
        for (const fnBlocks of mode === 'jit' ? o.fnBlocks : [null]) rows.push({ slug, grade, mode, fnBlocks });
      }
    }
  }
  return rows;
}

/// Which engine this is, said by the runtime rather than guessed from a version
/// string: a row that does not name its engine is not a JD19 row.
function engineName() {
  if (typeof globalThis.Bun !== 'undefined') return 'bun/JavaScriptCore ' + globalThis.Bun.version;
  if (globalThis.process && process.versions && process.versions.v8) return 'node/V8 ' + process.versions.node + ' (v8 ' + process.versions.v8 + ')';
  return 'unknown';
}

/// The one-minute load average, from the kernel.
///
/// NOT `os.loadavg()`: under `bun` that returns 0 always, and a zero that is a
/// fiction is worse than no column, because it looks like an idle desk. The
/// director's protocol refuses any row above ~8, so the number has to be real.
function loadavg() {
  try {
    return Number(execFileSync('/usr/sbin/sysctl', ['-n', 'vm.loadavg'], { encoding: 'utf8' }).trim().split(/\s+/)[1]);
  } catch {
    return NaN;
  }
}

const o = parseArgs(process.argv.slice(2));
const manifest = JSON.parse(readFileSync(join(o.stage, 'manifest.json'), 'utf8'));
const bySlug = Object.fromEntries(manifest.images.map((i) => [i.slug, i]));
const plan = o.rows ? rowsFromSpecs(o.rows, o) : rowsFromProduct(o);

const wasmBytes = readFileSync(join(o.stage, 'emu.wasm'));
const t0 = performance.now();
const compiled = new WebAssembly.Module(wasmBytes);
const emuCompileMs = performance.now() - t0;

console.log('engine       ' + engineName());
console.log('build        ' + manifest.build.short + ' (' + manifest.build.branch + (manifest.build.dirty ? ', dirty' : '') + ')');
console.log('emu.wasm     ' + wasmBytes.length + ' B, compiled in ' + emuCompileMs.toFixed(1) + ' ms');
console.log('load         ' + loadavg().toFixed(2) + ' (sysctl vm.loadavg, 1 min) at the start of this invocation');
console.log('rows         ' + plan.map((r) => r.slug + ':' + r.grade + ':' + r.mode + (r.fnBlocks ? ':' + r.fnBlocks : '')).join(' ') + '  x' + o.bestOf + ' interleaved');
console.log('');

const head = ['image', 'grade', 'mode', 'fn', 'wall s', 'ns/instr', 'real time', 'cover %', 'stay', 'esc %', 'load', 'uart sha256'];
const w = [12, 5, 6, 4, 9, 9, 10, 8, 8, 7, 6, 18];
console.log(head.map((h, i) => (i === 0 ? h.padEnd(w[i]) : h.padStart(w[i]))).join(' '));
console.log(head.map((_, i) => '-'.repeat(w[i])).join(' '));

const printRow = (r) => console.log([
  r.slug.padEnd(w[0]),
  r.grade.padStart(w[1]),
  r.mode.padStart(w[2]),
  String(r.fnBlocks ?? '-').padStart(w[3]),
  (r.wallMs / 1000).toFixed(2).padStart(w[4]),
  (r.nsPerInstr ? r.nsPerInstr.toFixed(2) : '-').padStart(w[5]),
  (r.realtime ? r.realtime.toFixed(4) + 'x' : '-').padStart(w[6]),
  (r.coverage !== undefined ? r.coverage.toFixed(2) : '-').padStart(w[7]),
  (r.meanStay ? r.meanStay.toFixed(1) : '-').padStart(w[8]),
  (r.escapeRate !== undefined ? (100 * r.escapeRate).toFixed(3) : '-').padStart(w[9]),
  (Number.isFinite(r.loadavg) ? r.loadavg.toFixed(1) : '?').padStart(w[10]),
  (r.uartSha256 || '-').slice(0, 16).padStart(w[11]),
].join(' '));

const all = [];
const best = new Map();   // row key -> the fastest reading of it
for (let pass = 0; pass < o.bestOf; pass++) {
  if (o.bestOf > 1) console.log('-- pass ' + (pass + 1) + ' of ' + o.bestOf);
  for (const step of plan) {
    const image = bySlug[step.slug];
    if (!image) throw new Error('no image ' + step.slug + ' in ' + o.stage + '/manifest.json');
    const elfBytes = new Uint8Array(readFileSync(join(o.stage, image.elf)));
    const r = await runOnce({
      compiled, image, elfBytes, grade: step.grade, mode: step.mode, fnBlocks: step.fnBlocks,
      timeout: o.timeout, wallTimeout: o.wallTimeout, exitOn: o.exitOn,
      extraArgs: o.extraArgs, env: o.env, keepText: !!o.dump,
    });
    if (o.dump) {
      mkdirSync(o.dump, { recursive: true });
      const p2 = join(o.dump, `${step.slug}-${step.grade}-${step.mode}-${step.fnBlocks ?? 'x'}-p${pass + 1}.log`);
      writeFileSync(p2, r.fullText ?? '');
      console.log('  -> ' + p2);
    }
    delete r.fullText;
    r.engine = engineName();
    r.loadavg = loadavg();
    r.pass = pass + 1;
    all.push(r);
    printRow(r);
    if (r.selftestError) console.log('  !! table selftest: ' + r.selftestError);
    if (r.trap) console.log('  !! trap: ' + r.trap.split('\n')[0]);
    if (o.tail) console.log(r.tail.split('\n').map((l) => '  | ' + l).join('\n'));
    const key = step.slug + ':' + step.grade + ':' + step.mode + ':' + (step.fnBlocks ?? '-');
    if (!best.has(key) || r.wallMs < best.get(key).wallMs) best.set(key, r);
  }
}

if (o.bestOf > 1) {
  console.log('');
  console.log('-- best of ' + o.bestOf + ', one invocation, interleaved');
  console.log(head.map((h, i) => (i === 0 ? h.padEnd(w[i]) : h.padStart(w[i]))).join(' '));
  console.log(head.map((_, i) => '-'.repeat(w[i])).join(' '));
  for (const r of best.values()) printRow(r);
}

// The ratio the gate quotes, beside the absolute, from THIS invocation: the
// translated leg over the interpreter leg of the same image and grade. Quoted
// with the identity column, because a ratio whose two legs printed different
// UART bytes is not a ratio of the same run.
const interp = [...best.values()].filter((r) => r.mode === 'interp');
if (interp.length) {
  console.log('');
  console.log('-- translated / interpreter, same invocation');
  for (const r of best.values()) {
    if (r.mode !== 'jit') continue;
    const d = interp.find((x) => x.slug === r.slug && x.grade === r.grade);
    if (!d) continue;
    const same = r.uartSha256 === d.uartSha256 ? 'uart SAME' : 'uart DIFFERS — NOT A ROW';
    console.log(
      `${r.slug} ${r.grade} fn=${r.fnBlocks}: ${(d.wallMs / r.wallMs).toFixed(3)}x the interpreter ` +
      `(${(r.wallMs / 1000).toFixed(2)} s vs ${(d.wallMs / 1000).toFixed(2)} s); ` +
      `${(r.realtime ?? 0).toFixed(4)}x real time vs ${(d.realtime ?? 0).toFixed(4)}x; ${same}`,
    );
  }
}

if (o.json) {
  writeFileSync(o.json, JSON.stringify({
    engine: engineName(), build: manifest.build, emuWasmBytes: wasmBytes.length, emuCompileMs,
    at: new Date().toISOString(), bestOf: o.bestOf, plan, rows: all,
    best: [...best.values()],
  }, null, 2));
  console.log('\nwrote ' + o.json);
}
