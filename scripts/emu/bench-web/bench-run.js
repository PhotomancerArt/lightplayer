// One bench run, and the readouts taken off it.
//
// Shared by `worker.js` (a browser Worker) and `bench-cli.mjs` (`bun`/`node`),
// because a desk engine row and a phone row are only comparable if the same
// code built the argument list, drove `_start`, and parsed the same lines out
// of the same streams. Neither the module bytes nor the ELF bytes are fetched
// here — the caller has them.
'use strict';

import { makeWasi } from './wasi-shim.js';
import { makeJitHost } from './jit-host.js';

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
  const wasi = makeWasi(argsFor(o), o.image.elf, o.elfBytes);
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
    timeout: o.timeout, exit, trap, selftest, selftestError,
    instantiateMs: t1 - t0, wallMs,
    ...readouts(text),
    jsEvents: host.events,
    uartSha256: await sha256(wasi.bytesAt(o.image.slug + '.uart')),
    framesSha256: await sha256(wasi.bytesAt(o.image.slug + '.jsonl')),
    uartBytes: wasi.bytesAt(o.image.slug + '.uart').length,
    tail: text.trim().split('\n').slice(-8).join('\n'),
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
