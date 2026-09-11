// One bench run, and the readouts taken off it.
//
// Shared by `worker.js` (a browser Worker) and `bench-cli.mjs` (`bun`/`node`),
// because a desk engine row and a phone row are only comparable if the same
// code built the argument list, drove `_start`, and parsed the same lines out
// of the same streams. Neither the module bytes nor the ELF bytes are fetched
// here — the caller has them.
'use strict';

// DD33 — this module passes on the stamp it was loaded with.
//
// `worker.js` is fetched as `worker.js?v=<build short>` and pulls this file in
// as `./bench-run.js?v=<build short>`; a static `import './wasi-shim.js'` here
// would drop the stamp again and leave a fresh `bench-run.js` free to pair with
// a cached `wasi-shim.js` or `jit-host.js` — the same defect one level down,
// and the one that killed Safari with a missing export and a bare stack. So
// this module reads its OWN stamp off `import.meta.url` and hands it to both
// siblings. The rule is self-propagating: every module in this directory that
// imports a sibling stamps that import from its own URL.
//
// It is written out here rather than shared from a helper module because the
// import of THAT helper would be the unstamped edge.
//
// `bench-cli.mjs` imports this file with no stamp (node/bun read the stage off
// the filesystem, where there is no cache to go stale), so `STAMP` is null
// there and the specifiers are the plain ones.
const STAMP = new URL(import.meta.url).searchParams.get('v');
const stamped = (name) => name + (STAMP ? '?v=' + STAMP : '');

// Kicked off at module scope so both fetches are in flight immediately, and
// awaited inside `runOnce` rather than at the top level — a top-level await
// here would become a top-level await in `worker.js`'s graph, and a module
// worker's message queue and TLA are a bad pair.
const siblings = Promise.all([
  import(stamped('./wasi-shim.js')),
  import(stamped('./jit-host.js')),
]).then(([wasiMod, jitMod]) => ({ makeWasi: wasiMod.makeWasi, makeJitHost: jitMod.makeJitHost }));

/// The gate rows, in the order the phone takes them.
///
/// A phone in someone's hand is not a place to drive six selects, and a row
/// taken at a different bound than the row beside it is not a comparison. So
/// the sequence the M7B gate quotes lives here, fixed, as data: `render-basic`
/// at `t2` inside the 5500 ms `GATE_US` window, the JD26 blocks-per-function
/// knob swept 8 -> 16 -> 32, and the same image re-taken under `--interpreter`
/// on the same binary in the same session.
///
/// ⚠️ 8 and 16 are deliberately BELOW the `fnBlocksChoices` floor the staged
/// manifest offers (32). That floor is there because V8 has died below it —
/// `Fatal process out of memory: Zone` from a background compile job, the
/// outer selector being 730 KB and 25,156 nested blocks at 8 against 176 KB at
/// 32 (P6b) — and a browser tab cannot catch that. The preset asks for the two
/// rows anyway, because whether the phone's own engine survives them IS the
/// question the gate is asking; `node` 25's V8 took both on `render-basic`
/// when this was written. On an engine that dies there, the page dies with it
/// and nothing uploads — which is that engine's answer, and the one failure
/// mode of this preset the rig cannot turn into a failed row.
export const GATE_ROWS = [
  { slug: 'render-basic', grade: 't2', mode: 'jit', fnBlocks: 8, timeout: '5500ms' },
  { slug: 'render-basic', grade: 't2', mode: 'jit', fnBlocks: 16, timeout: '5500ms' },
  { slug: 'render-basic', grade: 't2', mode: 'jit', fnBlocks: 32, timeout: '5500ms' },
  { slug: 'render-basic', grade: 't2', mode: 'interp', fnBlocks: null, timeout: '5500ms' },
];

/// `GATE_ROWS` as a plan the Worker can run, taking the wall-clock guard and
/// the `--exit-on` policy from the staged manifest so the preset and a
/// hand-driven run differ in nothing but the rows.
export function gateRowsPlan(manifest) {
  const d = (manifest && manifest.defaults) || {};
  const wallTimeout = d.wallTimeout ?? 600;
  const exitOn = !!d.exitOn;
  return GATE_ROWS.map((row) => ({ ...row, wallTimeout, exitOn }));
}

/// Build the emulator's own argv for one row.
///
/// The two flags P6's table forks on are `mode` and `fnBlocks`, and they are
/// argv and nothing else: the same binary, the same image, the same device,
/// the same session — which is the whole point of having the interpreter row
/// selectable from the page rather than taken from another run on another day.
export function argsFor(o) {
  const args = [
    'lp-emu-esp32c6',
    '--elf', '/w/' + o.image.elf,
    '--timeout', o.timeout,
    '--wall-timeout', String(o.wallTimeout ?? 600),
    '--uart0', 'file:/w/' + o.image.slug + '.uart',
    '--dump-frames', 'file:/w/' + o.image.slug + '.jsonl',
    '--time-grade', o.grade,
  ];
  if (o.mode === 'jit') {
    args.push('--jit', '--jit-report');
    if (o.fnBlocks) args.push('--jit-fn-blocks', String(o.fnBlocks));
  } else {
    args.push('--interpreter');
  }
  if (o.exitOn && o.image.exitOn) args.push('--exit-on', o.image.exitOn);
  // Anything else the caller wants on the emulator's own command line, in
  // order. P6b uses it for `--jit-blocks <n>`, which shrinks the installed
  // block set and so the module, and is how the footprint question gets
  // asked without a native cranelift run. The page never sets it.
  if (o.extraArgs) args.push(...o.extraArgs);
  return args;
}

const num = (s) => (s === undefined ? undefined : Number(s));

/// Everything the rig reports that is not a stopwatch, read off the run's own
/// streams rather than recomputed.
///
/// `--jit-report` already prints the boot-cost line per translation event, the
/// installed-coverage line, and the exit-reason census; the interpreter's
/// block-cache line already prints the `fence.i` count. Parsing them keeps one
/// source of truth — if a number here disagrees with a native `--jit-report`
/// run, one of them is wrong, and that is the finding.
export function readouts(text) {
  const r = { boot: [], exits: [] };

  const stopped = /stopped after (\d+) cycles \((\d+) us emulated, (\d+) instructions, grade ([^)]+)\)/.exec(text);
  if (stopped) {
    r.stoppedLine = stopped[0];
    r.cycles = num(stopped[1]);
    r.us = num(stopped[2]);
    r.instr = num(stopped[3]);
    r.gradeName = stopped[4];
  }

  // JD20's boot-cost line, one per translation event, verbatim plus the five
  // numbers the gate table quotes.
  for (const m of text.matchAll(/^jit: (boot:|fence\.i:)\s+discovered (\d+) blocks \/ (\d+) instr in\s+([\d.]+) ms; installed (\d+) blocks \/ (\d+) instr; emitted (\d+) B in ([\d.]+) ms in (\d+) fn x (\d+) blocks \(largest (\d+) B, targets (\d+) B\); compiled in ([\d.]+) ms; instantiated in ([\d.]+) ms/gm)) {
    r.boot.push({
      event: m[1].replace(':', ''),
      discoveredBlocks: num(m[2]), discoveredInstr: num(m[3]), discoverMs: num(m[4]),
      installedBlocks: num(m[5]), installedInstr: num(m[6]),
      moduleBytes: num(m[7]), emitMs: num(m[8]),
      functions: num(m[9]), fnBlocks: num(m[10]), largestBodyBytes: num(m[11]), targetTableBytes: num(m[12]),
      compileMs: num(m[13]), instantiateMs: num(m[14]),
      line: m[0],
    });
  }

  const cov = /^jit: coverage ([\d.]+) % of retired \((\d+) of (\d+) instructions ran inside translated code\); (\d+) retranslation\(s\) after (\d+) `fence\.i`/m.exec(text);
  if (cov) {
    r.coverage = num(cov[1]);
    r.coveredInstr = num(cov[2]);
    r.retiredInstr = num(cov[3]);
    r.retranslations = num(cov[4]);
    r.fenceI = num(cov[5]);
    r.coverageLine = cov[0];
  }

  for (const m of text.matchAll(/^jit: exits by reason: (\S+)\s+(\d+) exit\(s\),\s+(\d+) instructions interpreted after/gm)) {
    r.exits.push({ why: m[1], exits: num(m[2]), interpretedAfter: num(m[3]) });
  }

  const summary = /entries (\d+), retired (\d+), escape_hatch (\d+), cross (\d+), indirect_miss (\d+), interpreted_between (\d+)/.exec(text);
  if (summary) {
    r.entries = num(summary[1]);
    r.retiredInCode = num(summary[2]);
    r.escapeHatch = num(summary[3]);
    r.cross = num(summary[4]);
    r.indirectMiss = num(summary[5]);
    r.interpretedBetween = num(summary[6]);
    // The number P5's director notes asked for by name: an MMIO store always
    // leaves, so a stay is short, and how short decides whether an
    // `after_store_poll` import is worth building.
    r.meanStay = r.entries ? r.retiredInCode / r.entries : 0;
    r.escapeRate = r.retiredInCode ? r.escapeHatch / r.retiredInCode : 0;
  }

  // The interpreter's own block-cache line carries the `fence.i` count on a
  // run with no translated core, which is how the `--interpreter` row reports
  // the same number.
  if (r.fenceI === undefined) {
    const fence = /; (\d+) fence\.i/.exec(text);
    if (fence) r.fenceI = num(fence[1]);
  }
  return r;
}

async function sha256(bytes) {
  if (!bytes || bytes.length === 0) return null;
  const sub = globalThis.crypto && globalThis.crypto.subtle;
  if (!sub) return null;
  // A copy, because `digest` is async and the bytes it is handed must not be a
  // view into a linear memory that can grow under it.
  const d = await sub.digest('SHA-256', bytes.slice().buffer);
  return Array.from(new Uint8Array(d)).map((b) => b.toString(16).padStart(2, '0')).join('');
}

/// Run one row and return everything the gate table and the uploaded JSON need.
///
/// `compiled` is an already-compiled `WebAssembly.Module` of the emulator;
/// `elfBytes` are the staged firmware image's. Everything else is the row.
export async function runOnce(o) {
  // `o.env` is P6c's way to reach the emulator's environment-gated
  // diagnostics from a wasm row (`LP_EMU_JIT_MMIO_CENSUS` and the rest).
  // Absent on every row the page takes.
  const { makeWasi, makeJitHost } = await siblings;
  const wasi = makeWasi(argsFor(o), o.image.elf, o.elfBytes, o.env ?? {});
  const host = makeJitHost();

  const t0 = performance.now();
  const inst = await WebAssembly.instantiate(o.compiled, { ...wasi.imports, ...host.imports });
  wasi.setMemory(inst.exports.memory);

  // Before `_start`, and it throws rather than returns: an engine whose table
  // entry mechanism does not hold must not be measured, it must be reported.
  let selftest = null, selftestError = null;
  if (o.mode === 'jit') {
    try { selftest = host.attach(inst); } catch (e) { selftestError = String((e && e.message) || e); }
  }
  const t1 = performance.now();

  let exit = 0, trap = null;
  if (!selftestError) {
    try {
      inst.exports._start();
    } catch (e) {
      if (e && typeof e.wasiExit === 'number') exit = e.wasiExit;
      // A trap out of translated code cannot be caught inside the module and
      // kills the instance; catching it here is what turns "the phone hung"
      // into a row that says what happened.
      else { trap = String((e && e.stack) || e); exit = -1; }
    }
  }
  const t2 = performance.now();

  const text = wasi.text(1) + '\n' + wasi.text(2);
  const wallMs = t2 - t1;
  const r = {
    slug: o.image.slug, grade: o.grade, mode: o.mode, fnBlocks: o.mode === 'jit' ? o.fnBlocks : null,
    extraArgs: o.extraArgs ?? null,
    timeout: o.timeout, exit, trap, selftest, selftestError,
    instantiateMs: t1 - t0, wallMs,
    ...readouts(text),
    jsEvents: host.events,
    uartSha256: await sha256(wasi.bytesAt(o.image.slug + '.uart')),
    framesSha256: await sha256(wasi.bytesAt(o.image.slug + '.jsonl')),
    uartBytes: wasi.bytesAt(o.image.slug + '.uart').length,
    tail: text.trim().split('\n').slice(-8).join('\n'),
    // The whole of stdout+stderr, only when the caller asked for it (M7 P6c's
    // `--dump`). Eight lines of tail is the right size for a bench table and
    // the wrong size for a census the run printed 60 lines of.
    fullText: o.keepText ? text : undefined,
  };
  if (r.instr) {
    r.ips = r.instr / (wallMs / 1000);
    // The milestone's own unit, and the first honest one for the whole image:
    // the run's own `stopped after` instruction count over the wall time the
    // Worker measured around `_start`.
    r.nsPerInstr = (wallMs * 1e6) / r.instr;
    r.realtime = (r.us / 1000) / wallMs;
  }
  return r;
}
