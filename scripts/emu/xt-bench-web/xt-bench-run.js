// One classic-ESP32 bench run, and the readouts taken off it.
//
// **A twin of `scripts/emu/bench-web/bench-run.js`, not an edit of it.**
// `scripts/emu/bench-web/**` is the perf lab's and is never edited by another
// plan (the lab director's standing rule); the C6 rig hard-codes its own
// argv[0], its own flag list and its own report-line grammar, and the classic
// shares none of the three. So this file is a copy with the four things that
// differ changed, and everything that does not differ left alone so the two
// can be diffed:
//
// | | the C6 rig | here |
// |---|---|---|
// | argv[0] | `lp-emu-esp32c6` | `lp-emu-esp32v3` |
// | the flags | `--trap-log`, `t1`/`t2` | no trap log (the classic has none), `--core-quantum 256`, `t1` only |
// | the stop line | `stopped after N cycles (N us emulated, …)` | `DEADLINE\|EXIT MATCHED\|WALL TIMEOUT cycle=N (N us emulated)` + `run: cycles=… instructions=…` |
// | coverage | `jit: coverage X % of retired` | derived: the per-core report's "retired natively" over the `run:` line's own per-core retired count |
//
// Shared by `xt-worker.js` (a browser Worker — the shape the perf lab's page
// spawns out of a staged build) and `xt-bench-cli.mjs` (`bun`/`node`), because
// a desk-engine row and a phone row are only comparable if the same code built
// the argument list, drove `_start`, and parsed the same lines out of the same
// streams. Neither the module bytes nor the ELF bytes are fetched here — the
// caller has them.
'use strict';

// DD33 — this module passes on the stamp it was loaded with, exactly as the
// C6 rig's does and for the reason its comment gives: a static specifier
// cannot carry the stamp, so a fresh entry module is free to pair with a
// cached sibling. `xt-bench-cli.mjs` imports this with no stamp (node/bun read
// the stage off the filesystem, where there is no cache to go stale).
const STAMP = new URL(import.meta.url).searchParams.get('v');
const stamped = (name) => name + (STAMP ? '?v=' + STAMP : '');

// Both siblings are the C6 rig's files, **staged unchanged**:
//
// - `wasi-shim.js` implements exactly the preview1 imports the wasip1 build
//   declares, and the classic's import list is the C6's list — checked against
//   `WebAssembly.Module.imports` of both modules, which agree name for name.
// - `jit-host.js` is the product half of the browser seam and lives in the
//   translator crate (`lp-emu/lp-emu-jit/js/jit-host.js`); the rig stages a
//   copy. It knows nothing about which machine emitted the module it runs.
const siblings = Promise.all([
  import(stamped('./wasi-shim.js')),
  import(stamped('./jit-host.js')),
]).then(([wasiMod, jitMod]) => ({ makeWasi: wasiMod.makeWasi, makeJitHost: jitMod.makeJitHost }));

/// The rows the milestone bar is read on (acceptance 3): `render-loop` at t1,
/// both cores at `--core-quantum 256`, the translated core at the three sizes
/// the desk table sweeps, and the same image re-taken under `--interpreter`
/// on the same binary in the same session.
///
/// **8, 16 and 64**, not the RV32 ladder's 8 and 16: P08's desk tables put
/// the classic's best at **64 in V8** and **16 in JSC**, and 64 is the
/// emulator's own `JIT_FN_BLOCKS_DEFAULT` — so a phone press has to cover the
/// default it would otherwise not measure. The phone's own preference is
/// G-M7P-XT's question, and a preset that cannot ask it is not the preset.
///
/// The interpreter row is not a historical number: it is the denominator of
/// the ratio the gate quotes, and it has to come from the same invocation on
/// the same device at the same load or it is not a ratio of anything.
export const GATE_ROWS = [
  { slug: 'render-loop', grade: 't1', mode: 'jit', fnBlocks: 8, timeout: '5500ms' },
  { slug: 'render-loop', grade: 't1', mode: 'jit', fnBlocks: 16, timeout: '5500ms' },
  { slug: 'render-loop', grade: 't1', mode: 'jit', fnBlocks: 64, timeout: '5500ms' },
  { slug: 'render-loop', grade: 't1', mode: 'interp', fnBlocks: null, timeout: '5500ms' },
];

/// `GATE_ROWS` as a plan a Worker can run, taking the wall-clock guard and the
/// `--exit-on` policy from the staged manifest so the preset and a hand-driven
/// run differ in nothing but the rows.
export function gateRowsPlan(manifest) {
  const d = (manifest && manifest.defaults) || {};
  const wallTimeout = d.wallTimeout ?? 600;
  const exitOn = !!d.exitOn;
  return GATE_ROWS.map((row) => ({ ...row, wallTimeout, exitOn }));
}

/// Build the emulator's own argv for one row.
///
/// `mode` and `fnBlocks` are argv and nothing else: the same binary, the same
/// image, the same engine, the same session — which is what makes the
/// interpreter row a denominator rather than a different experiment.
export function argsFor(o) {
  const args = [
    'lp-emu-esp32v3',
    '--elf', '/w/' + o.image.elf,
    '--timeout', o.timeout,
    '--wall-timeout', String(o.wallTimeout ?? 600),
    // The plan's protocol names the quantum on every classic row (acceptance
    // 3: "both cores at `--core-quantum 256`"). It is also the default, and
    // it is written out anyway: two quanta are two interleavings, and a row
    // that does not name its own is not comparable to one that does.
    '--core-quantum', '256',
    '--uart0', 'file:/w/' + o.image.slug + '.uart',
    '--dump-frames', 'file:/w/' + o.image.slug + '.jsonl',
    '--time-grade', o.grade,
  ];
  if (o.mode === 'jit') {
    args.push('--jit', '--jit-report');
    if (o.fnBlocks) args.push('--jit-fn-blocks', String(o.fnBlocks));
  } else {
    // NOT "the absence of `--jit`": the wasip1 build translates by default
    // (`machine::TRANSLATED_BY_DEFAULT`), so the oracle leg is an argument.
    args.push('--interpreter');
  }
  if (o.exitOn && o.image.exitOn) args.push('--exit-on', o.image.exitOn);
  if (o.extraArgs) args.push(...o.extraArgs);
  return args;
}

const num = (s) => (s === undefined ? undefined : Number(s));

/// Everything the rig reports that is not a stopwatch, read off the run's own
/// streams rather than recomputed.
///
/// The classic prints no `stopped after …` line; it prints the outcome
/// (`DEADLINE` / `EXIT MATCHED` / `WALL TIMEOUT`) with the cycle and the
/// emulated microseconds, and a `run:` line with the retired instruction
/// counts per core. Those two together are the C6's one line.
export function readouts(text) {
  const r = { boot: [], cores: [] };

  const outcome =
    /^(DEADLINE|EXIT MATCHED|WALL TIMEOUT|BREAKPOINT|FAULT|STRICT BUS) cycle=(\d+)(?: \((\d+) us emulated\))?/m.exec(text);
  if (outcome) {
    r.outcome = outcome[1];
    r.stoppedLine = outcome[0];
    r.us = num(outcome[3]);
  }

  const run = /^run: cycles=(\d+) instructions=(\d+) \(core0=(\d+) core1=(\d+)\)/m.exec(text);
  if (run) {
    r.runLine = run[0];
    r.cycles = num(run[1]);
    r.instr = num(run[2]);
    r.retiredPerCore = [num(run[3]), num(run[4])];
  }

  // JD20's boot-cost line, one per translation event per core, verbatim plus
  // the numbers the gate table quotes. `lp-emu-esp32v3/src/jit.rs`'s
  // `BuildReport::boot_line` is the grammar; `event` is `boot`,
  // `app-core release` or `publish-by-store #N` — the classic's two events
  // plus the APP core's DPORT release, and two of the three carry a space, so
  // the event is `[^:]+` rather than one token. The same lines appear again
  // inside each per-core report line; those are mid-line and the `^` anchor is
  // what keeps them from being counted twice.
  for (const m of text.matchAll(
    /^jit: core(\d+) ([^:]+): (\d+) block\(s\) from (\d+) seed\(s\), (\d+) instruction\(s\) \((\d+) escaped, (\d+) emitted natively, static escape share ([\d.]+) %[^)]*\); (\d+) function\(s\) at (\d+) blocks each, largest body (\d+) B, module (\d+) B \(([\d.]+) B per instruction\); discover ([\d.]+) ms, emit ([\d.]+) ms, compile ([\d.]+) ms, instantiate ([\d.]+) ms/gm,
  )) {
    r.boot.push({
      core: num(m[1]), event: m[2],
      blocks: num(m[3]), seeds: num(m[4]), instr: num(m[5]),
      escapedInstr: num(m[6]), nativeInstr: num(m[7]), staticEscapePct: num(m[8]),
      functions: num(m[9]), fnBlocks: num(m[10]), largestBodyBytes: num(m[11]),
      moduleBytes: num(m[12]), bytesPerInstr: num(m[13]),
      discoverMs: num(m[14]), emitMs: num(m[15]), compileMs: num(m[16]), instantiateMs: num(m[17]),
      line: m[0],
    });
  }

  // `--jit-report`'s per-core line. The numbers the milestone reads off it are
  // the entry count (the entry protocol's ns/entry is a first-order term at
  // 7.18 M entries), the mean stay against the ~155-instruction runway, and
  // the instructions that retired natively rather than through the escape.
  for (const m of text.matchAll(
    /^jit: core(\d+): (\d+) entries, (\d+) instruction\(s\) inside translated code \((\d+) escaped to the interpreter, (\d+) retired natively, ([\d.]+) % of the entered\); mean stay ([\d.]+) instruction\(s\)[^;]*; (\d+) publish-by-store retranslation\(s\)/gm,
  )) {
    r.cores.push({
      core: num(m[1]), entries: num(m[2]), retiredInCode: num(m[3]),
      escapeHatch: num(m[4]), retiredNatively: num(m[5]), nativeSharePct: num(m[6]),
      meanStay: num(m[7]), retranslations: num(m[8]),
      line: m[0],
    });
  }

  // The `why` histogram, so a row can say what ended its stays without the
  // caller re-parsing the report line.
  const why = /exits by reason: ([^;]+);/.exec(text);
  if (why) {
    r.why = {};
    for (const pair of why[1].split(',')) {
      const t = pair.trim().split(/\s+/);
      if (t.length === 2) r.why[t[0]] = Number(t[1]);
    }
  }

  if (r.cores.length) {
    const entries = r.cores.reduce((a, c) => a + c.entries, 0);
    const inCode = r.cores.reduce((a, c) => a + c.retiredInCode, 0);
    const native = r.cores.reduce((a, c) => a + c.retiredNatively, 0);
    const escaped = r.cores.reduce((a, c) => a + c.escapeHatch, 0);
    r.entries = entries;
    r.retiredInCode = inCode;
    r.escapeHatch = escaped;
    r.meanStay = entries ? inCode / entries : 0;
    r.escapeRate = inCode ? escaped / inCode : 0;
    r.retranslations = r.cores.reduce((a, c) => a + c.retranslations, 0);
    // **Coverage against what actually retired**, which the classic does not
    // print: the C6's `jit: coverage` line comes from its own census, and the
    // classic's census (`LP_EMU_XT_BLOCKPROF`) costs a test per retire and is
    // not something a timed row may carry. The `run:` line's per-core retired
    // counts are the same denominator that census would use, so the division
    // is done here — and it is labelled `coverage` for the same reason the
    // C6's is: instructions that retired *inside translated code*, natively.
    if (r.instr) r.coverage = (100 * native) / r.instr;
  }
  return r;
}

/// SHA-256 in plain JavaScript, for the context `crypto.subtle` is not in.
///
/// ⚠️ Copied verbatim from the C6 rig's `bench-run.js`, and it is not
/// belt-and-braces: `crypto.subtle` exists only in a *secure context*, and the
/// lab serves plain `http://` to a LAN IP, which is not one. Without this every
/// row a phone uploads carries `uartSha256: null` and the identity column of
/// the gate table is blank for exactly the device the gate is about (M7b P5).
const K256 = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

function sha256Js(bytes) {
  const h = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ]);
  const n = bytes.length;
  const blocks = Math.ceil((n + 9) / 64);
  const m = new Uint8Array(blocks * 64);
  m.set(bytes);
  m[n] = 0x80;
  const view = new DataView(m.buffer);
  view.setUint32(m.length - 8, Math.floor(n / 0x20000000), false);
  view.setUint32(m.length - 4, (n << 3) >>> 0, false);

  const w = new Uint32Array(64);
  const rotr = (x, k) => ((x >>> k) | (x << (32 - k))) >>> 0;
  for (let b = 0; b < blocks; b++) {
    for (let t = 0; t < 16; t++) w[t] = view.getUint32(b * 64 + t * 4, false);
    for (let t = 16; t < 64; t++) {
      const s0 = (rotr(w[t - 15], 7) ^ rotr(w[t - 15], 18) ^ (w[t - 15] >>> 3)) >>> 0;
      const s1 = (rotr(w[t - 2], 17) ^ rotr(w[t - 2], 19) ^ (w[t - 2] >>> 10)) >>> 0;
      w[t] = (w[t - 16] + s0 + w[t - 7] + s1) >>> 0;
    }
    let [a, bb, c, d, e, f, g, hh] = h;
    for (let t = 0; t < 64; t++) {
      const S1 = (rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25)) >>> 0;
      const ch = ((e & f) ^ (~e & g)) >>> 0;
      const t1 = (hh + S1 + ch + K256[t] + w[t]) >>> 0;
      const S0 = (rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22)) >>> 0;
      const maj = ((a & bb) ^ (a & c) ^ (bb & c)) >>> 0;
      const t2 = (S0 + maj) >>> 0;
      hh = g; g = f; f = e;
      e = (d + t1) >>> 0;
      d = c; c = bb; bb = a;
      a = (t1 + t2) >>> 0;
    }
    h[0] = (h[0] + a) >>> 0; h[1] = (h[1] + bb) >>> 0;
    h[2] = (h[2] + c) >>> 0; h[3] = (h[3] + d) >>> 0;
    h[4] = (h[4] + e) >>> 0; h[5] = (h[5] + f) >>> 0;
    h[6] = (h[6] + g) >>> 0; h[7] = (h[7] + hh) >>> 0;
  }
  let out = '';
  for (const x of h) out += x.toString(16).padStart(8, '0');
  return out;
}

function countLines(bytes) {
  if (!bytes) return 0;
  let n = 0;
  for (let i = 0; i < bytes.length; i++) if (bytes[i] === 10) n++;
  return n;
}

async function sha256(bytes) {
  if (!bytes || bytes.length === 0) return null;
  const sub = globalThis.crypto && globalThis.crypto.subtle;
  // A copy, because `digest` is async and the bytes it is handed must not be a
  // view into a linear memory that can grow under it.
  const copy = bytes.slice();
  if (sub) {
    try {
      const d = await sub.digest('SHA-256', copy.buffer);
      return Array.from(new Uint8Array(d)).map((b) => b.toString(16).padStart(2, '0')).join('');
    } catch {
      // Some engines expose `crypto.subtle` in an insecure context and then
      // reject the call. Either way the answer is the same digest.
    }
  }
  return sha256Js(copy);
}

/// Run one row and return everything the gate table and an uploaded JSON need.
///
/// `compiled` is an already-compiled `WebAssembly.Module` of the emulator;
/// `elfBytes` are the staged firmware image's. Everything else is the row.
export async function runOnce(o) {
  const { makeWasi, makeJitHost } = await siblings;
  const wasi = makeWasi(argsFor(o), o.image.elf, o.elfBytes, o.env ?? {});
  const host = makeJitHost();

  const t0 = performance.now();
  const inst = await WebAssembly.instantiate(o.compiled, { ...wasi.imports, ...host.imports });
  wasi.setMemory(inst.exports.memory);

  // Before `_start`, and it throws rather than returns: an engine whose table
  // entry mechanism does not hold must not be measured, it must be reported.
  // Unconditional, including on an `interp` row — that row passes
  // `--interpreter`, which vetoes the core from inside the emulator, and it is
  // the ARGUMENT that selects the interpreter, never the absence of a seam.
  let selftest = null, selftestError = null;
  try { selftest = host.attach(inst); } catch (e) { selftestError = String((e && e.message) || e); }
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
    frameLines: countLines(wasi.bytesAt(o.image.slug + '.jsonl')),
    tail: text.trim().split('\n').slice(-8).join('\n'),
    fullText: o.keepText ? text : undefined,
  };
  if (r.instr) {
    r.ips = r.instr / (wallMs / 1000);
    r.nsPerInstr = (wallMs * 1e6) / r.instr;
  }
  // The milestone's own unit: emulated microseconds over the wall milliseconds
  // the caller measured around `_start`. The emulated half comes off the run's
  // OWN outcome line, never from the requested `--timeout` — an `--exit-on`
  // row stops early and a wall-timeout row stops earlier still.
  if (r.us) r.realtime = (r.us / 1000) / wallMs;
  return r;
}
