// M7 P2 browser-seam probe -- S1 through S7 test bodies.
//
// Every function here builds its own tiny wasm modules with `wasm-gen.mjs`
// and times the operation the phase spec asks about. Nothing here is
// product code; nothing under `lp-emu/` is touched. See
// `p02-browser-seam-probe.md` for what each Sn answers.
import { buildModule, Op, I32 } from './wasm-gen.mjs';

// ---- a tiny "for i in 0..limit" loop, shared by every module below ------
// Standard raw-wasm idiom: an outer `block` you `br` out of, an inner `loop`
// you `br` back to. `counterLocal` is read/written; `limitInstrs` computes
// the bound once per iteration (cheap -- it is always a `local.get`).
function countedLoop(counterLocal, limitInstrs, bodyInstrs, incrBy = 1) {
  return [
    Op.block(),
    Op.loop(),
    Op.localGet(counterLocal), ...limitInstrs, Op.i32GeU(), Op.brIf(1),
    ...bodyInstrs,
    Op.localGet(counterLocal), Op.i32Const(incrBy), Op.i32Add(), Op.localSet(counterLocal),
    Op.br(0),
    Op.end(), // loop
    Op.end(), // block
  ];
}

function memInfo() {
  try {
    if (typeof process !== 'undefined' && process.memoryUsage) {
      const m = process.memoryUsage();
      return { rssBytes: m.rss, externalBytes: m.external, arrayBuffersBytes: m.arrayBuffers };
    }
  } catch { /* ignore */ }
  try {
    // Chrome-only, non-standard, but the desk-Chrome row can use it.
    if (typeof performance !== 'undefined' && performance.memory) {
      return { usedJSHeapBytes: performance.memory.usedJSHeapSize, totalJSHeapBytes: performance.memory.totalJSHeapSize };
    }
  } catch { /* ignore */ }
  return null;
}

// ---- module builders ------------------------------------------------

// S1: a module sized to ~targetBytes, shaped like translated guest code --
// many small functions rather than one big data blob, because a data
// section compiles by memcpy and would understate compile cost. ~800
// bytes/function matches the spike's measured ~808 B/block average
// (m7/plan.md JD8), so the toy module's compile-cost shape is realistic.
export function buildBigModule(targetBytes) {
  const bodyLen = 800;
  const pairs = Math.max(1, Math.floor((bodyLen - 2) / 2));
  const bodyFor = () => {
    const instrs = [Op.localGet(0)];
    for (let i = 0; i < pairs; i++) { instrs.push(Op.i32Const(i & 0x3f)); instrs.push(Op.i32Add()); }
    return instrs;
  };
  const approxPerFunc = bodyLen + 5; // + size-uleb + empty-locals-vec + end
  const count = Math.max(1, Math.ceil(targetBytes / approxPerFunc));
  const funcs = [];
  for (let i = 0; i < count; i++) funcs.push({ typeIdx: 0, locals: [], body: bodyFor() });
  const bytes = buildModule({
    types: [[[I32], [I32]]],
    funcs,
    exports: [{ name: 'probe0', kind: 'func', index: 0 }],
  });
  return { bytes, count };
}

// A memory owner: stands in for "the emulator's memory", exported plus a
// load/store pair so a JS driver (or another module) can touch it. Used
// directly for S2/S6/S7; `buildEmuModule` below adds the table and MMIO
// exports S3-S5 need on top of the same shape.
export function buildMemoryOwnerModule({ minPages = 2, maxPages = 8 } = {}) {
  const T_LOAD = 0, T_STORE = 1;
  const types = [[[I32], [I32]], [[I32, I32], []]];
  const funcs = [
    { typeIdx: T_LOAD, locals: [], body: [Op.localGet(0), Op.i32Load(2, 0)] },
    { typeIdx: T_STORE, locals: [], body: [Op.localGet(0), Op.localGet(1), Op.i32Store(2, 0)] },
  ];
  const exports = [
    { name: 'memory', kind: 'mem', index: 0 },
    { name: 'load', kind: 'func', index: 0 },
    { name: 'store', kind: 'func', index: 1 },
  ];
  return buildModule({ types, funcs, memory: { min: minPages, max: maxPages }, exports });
}

// A module that imports someone else's memory instead of owning one --
// stands in for translated code reading/writing the emulator's memory
// directly, no accessor call. Used for S2 (memory-only) and as the many
// small instances of S7.
export function buildMemoryClientModule() {
  const T_LOAD = 0, T_STORE = 1;
  const types = [[[I32], [I32]], [[I32, I32], []]];
  const imports = [{ mod: 'a', name: 'memory', kind: 'mem', mem: { min: 1 } }];
  const funcs = [
    { typeIdx: T_LOAD, locals: [], body: [Op.localGet(0), Op.i32Load(2, 0)] },
    { typeIdx: T_STORE, locals: [], body: [Op.localGet(0), Op.localGet(1), Op.i32Store(2, 0)] },
  ];
  const exports = [{ name: 'load', kind: 'func', index: 0 }, { name: 'store', kind: 'func', index: 1 }];
  return buildModule({ types, imports, funcs, exports });
}

// The emulator stand-in ("A"): exported memory, exported
// `__indirect_function_table`, exported mmio_load/mmio_store, and two
// entry drivers -- one per JD12 candidate:
//   enter_via_table(tableIdx, entries)  -- (a) call_indirect through the table
//   enter_via_import(id, entries)       -- (b) an imported jit_enter per entry
// Each driver calls the entry `entries` times inside ONE wasm call, so the
// timed span is pure engine/call cost with no JS in the loop.
export function buildEmuModule({ memPages = 4, memMaxPages = 16, tableMin = 2, tableMax = 8, withJitEnterImport = true } = {}) {
  const T_MMIO_LOAD = 0;   // (i32) -> i32
  const T_MMIO_STORE = 1;  // (i32,i32) -> ()
  const T_RUN = 2;         // (i32,i32) -> i32     -- B's `run(pc, iters)`, and call_indirect's type
  const T_ENTER = 3;       // (i32,i32) -> i32     -- enter_via_table / enter_via_import
  const T_JIT_ENTER = 4;   // (i32,i32) -> i32     -- imported jit_enter(id, pc)

  const types = [
    [[I32], [I32]],
    [[I32, I32], []],
    [[I32, I32], [I32]],
    [[I32, I32], [I32]],
    [[I32, I32], [I32]],
  ];

  const imports = [];
  let jitEnterIdx = -1;
  if (withJitEnterImport) { imports.push({ mod: 'host', name: 'jit_enter', kind: 'func', typeIdx: T_JIT_ENTER }); jitEnterIdx = 0; }
  const base = imports.length;

  const fMmioLoad = base + 0, fMmioStore = base + 1, fEnterTable = base + 2;
  const funcs = [
    { typeIdx: T_MMIO_LOAD, locals: [], body: [Op.localGet(0), Op.i32Load(2, 0)] },
    { typeIdx: T_MMIO_STORE, locals: [], body: [Op.localGet(0), Op.localGet(1), Op.i32Store(2, 0)] },
    {
      typeIdx: T_ENTER, locals: [I32, I32], body: [
        Op.i32Const(0), Op.localSet(2),
        Op.i32Const(0), Op.localSet(3),
        ...countedLoop(3, [Op.localGet(1)], [
          Op.localGet(2),
          Op.i32Const(0), Op.i32Const(1), Op.localGet(0), // pc=0, iters=1, tableIdx
          Op.callIndirect(T_RUN),
          Op.i32Xor(), Op.localSet(2),
        ]),
        Op.localGet(2),
      ],
    },
  ];
  const exports = [
    { name: 'memory', kind: 'mem', index: 0 },
    { name: '__indirect_function_table', kind: 'table', index: 0 },
    { name: 'mmio_load', kind: 'func', index: fMmioLoad },
    { name: 'mmio_store', kind: 'func', index: fMmioStore },
    { name: 'enter_via_table', kind: 'func', index: fEnterTable },
  ];

  if (withJitEnterImport) {
    const fEnterImport = funcs.length + base;
    funcs.push({
      typeIdx: T_ENTER, locals: [I32, I32], body: [
        Op.i32Const(0), Op.localSet(2),
        Op.i32Const(0), Op.localSet(3),
        ...countedLoop(3, [Op.localGet(1)], [
          Op.localGet(2),
          Op.localGet(0), Op.i32Const(0), // id, pc=0
          Op.call(jitEnterIdx),
          Op.i32Xor(), Op.localSet(2),
        ]),
        Op.localGet(2),
      ],
    });
    exports.push({ name: 'enter_via_import', kind: 'func', index: fEnterImport });
  }

  return buildModule({
    types, imports, funcs,
    table: { min: tableMin, max: tableMax },
    memory: { min: memPages, max: memMaxPages },
    exports,
  });
}

// The translated-code stand-in ("B"): exported `run(pc, iters) -> i32`,
// which loops `iters` times calling one of three things -- A's exported
// mmio_load directly (wasm->wasm, the real design), a JS shim that
// forwards to A (the thing JD11 rejects), or a function defined inside B
// itself (the "no cross-instance call at all" baseline). Same call shape
// in all three so the comparison is apples to apples.
export function buildTranslatedModule({ mmioSource = 'a-import' } = {}) {
  const T_LOAD = 0, T_RUN = 1;
  const types = [[[I32], [I32]], [[I32, I32], [I32]]];
  const imports = [];
  if (mmioSource === 'a-import') imports.push({ mod: 'a', name: 'mmio_load', kind: 'func', typeIdx: T_LOAD });
  if (mmioSource === 'js-shim') imports.push({ mod: 'env', name: 'mmio_load_shim', kind: 'func', typeIdx: T_LOAD });
  const base = imports.length;

  const funcs = [];
  let targetIdx;
  if (mmioSource === 'inner') {
    funcs.push({ typeIdx: T_LOAD, locals: [], body: [Op.localGet(0), Op.i32Load(2, 0)] });
    targetIdx = base + 0;
  } else {
    targetIdx = 0; // the single func import
  }
  const runIdx = funcs.length + base;
  funcs.push({
    typeIdx: T_RUN, locals: [I32, I32], body: [
      Op.i32Const(0), Op.localSet(2),
      Op.i32Const(0), Op.localSet(3),
      ...countedLoop(3, [Op.localGet(1)], [
        Op.localGet(2),
        Op.i32Const(0),
        Op.call(targetIdx),
        Op.i32Xor(), Op.localSet(2),
      ]),
      Op.localGet(2),
    ],
  });

  const spec = { types, imports, funcs, exports: [{ name: 'run', kind: 'func', index: runIdx }] };
  if (mmioSource === 'inner') spec.memory = { min: 1 };
  return buildModule(spec);
}

// ---- S1 - S7 ----------------------------------------------------------

export async function runS1() {
  const targetBytes = 6 * 1024 * 1024;
  const { bytes, count } = buildBigModule(targetBytes);
  let syncOk = true, syncErr = null, mod = null;
  const t0 = performance.now();
  try { mod = new WebAssembly.Module(bytes); } catch (e) { syncOk = false; syncErr = String(e && e.stack || e); }
  const syncMs = performance.now() - t0;

  const t2 = performance.now();
  let asyncOk = true, asyncErr = null;
  try { await WebAssembly.compile(bytes); } catch (e) { asyncOk = false; asyncErr = String(e && e.stack || e); }
  const asyncMs = performance.now() - t2;

  let callOk = null;
  if (syncOk) {
    try { const inst = new WebAssembly.Instance(mod, {}); callOk = typeof inst.exports.probe0(1) === 'number'; }
    catch (e) { callOk = 'error: ' + e; }
  }
  return { bytes: bytes.length, functionCount: count, syncOk, syncErr, syncMs, asyncOk, asyncErr, asyncMs, callOk };
}

export function runS2() {
  const aBytes = buildMemoryOwnerModule({ minPages: 2, maxPages: 8 });
  const bBytes = buildMemoryClientModule();
  const aInst = new WebAssembly.Instance(new WebAssembly.Module(aBytes), {});
  const bInst = new WebAssembly.Instance(new WebAssembly.Module(bBytes), { a: { memory: aInst.exports.memory } });

  const r = {};
  aInst.exports.store(100, 12345);
  r.aWriteVisibleToB = bInst.exports.load(100) === 12345;
  bInst.exports.store(200, 6789);
  r.bWriteVisibleToA = aInst.exports.load(200) === 6789;

  const bytesBefore = aInst.exports.memory.buffer.byteLength;
  aInst.exports.memory.grow(1);
  const bytesAfter = aInst.exports.memory.buffer.byteLength;
  r.grew = bytesAfter > bytesBefore;
  r.bytesBefore = bytesBefore;
  r.bytesAfter = bytesAfter;

  aInst.exports.store(300, 999);
  r.afterGrowAWriteVisibleToB = bInst.exports.load(300) === 999;
  bInst.exports.store(400, 111);
  r.afterGrowBWriteVisibleToA = aInst.exports.load(400) === 111;

  r.ok = r.aWriteVisibleToB && r.bWriteVisibleToA && r.grew && r.afterGrowAWriteVisibleToB && r.afterGrowBWriteVisibleToA;
  return r;
}

export function runS3(iters = 20_000_000) {
  const aBytes = buildEmuModule({ withJitEnterImport: false });
  const aInst = new WebAssembly.Instance(new WebAssembly.Module(aBytes), {});
  aInst.exports.mmio_store(0, 0x1234); // non-zero, so a skipped call would visibly change the result
  const variants = {};
  for (const mmioSource of ['a-import', 'js-shim', 'inner']) {
    const bBytes = buildTranslatedModule({ mmioSource });
    const bMod = new WebAssembly.Module(bBytes);
    let importObj = {};
    if (mmioSource === 'a-import') importObj = { a: { mmio_load: aInst.exports.mmio_load } };
    if (mmioSource === 'js-shim') importObj = { env: { mmio_load_shim: (addr) => aInst.exports.mmio_load(addr) } };
    const bInst = new WebAssembly.Instance(bMod, importObj);
    bInst.exports.run(0, Math.min(iters, 500_000)); // JIT warm-up, excluded from the timed span
    const t0 = performance.now();
    const sum = bInst.exports.run(0, iters);
    const ms = performance.now() - t0;
    variants[mmioSource] = { ms, nsPerCall: (ms * 1e6) / iters, sum };
  }
  return { iters, variants };
}

export function runS4(entries = 5_000_000) {
  let jitEnterTarget = null;
  const importObj = { host: { jit_enter: (_id, pc) => jitEnterTarget.exports.run(pc, 1) } };
  const aBytes = buildEmuModule({ withJitEnterImport: true, tableMin: 2, tableMax: 4 });
  const aInst = new WebAssembly.Instance(new WebAssembly.Module(aBytes), importObj);

  const bBytes = buildTranslatedModule({ mmioSource: 'a-import' });
  const bInst = new WebAssembly.Instance(new WebAssembly.Module(bBytes), { a: { mmio_load: aInst.exports.mmio_load } });
  jitEnterTarget = bInst;
  aInst.exports.mmio_store(0, 0xabcd); // non-zero, so a skipped call would visibly change the result

  const r = { entries };

  // (a) shared function table: grow A's table by one slot and set it to
  // B's exported `run` -- a funcref that came from a *different* instance.
  //
  // Deliberately NOT the fused `table.grow(delta, initValue)` form: JSC (at
  // least the version bundled with bun 1.1.18) accepts that call without
  // throwing and `table.get(idx)` reports the funcref afterward, but
  // `call_indirect` on that same slot traps with "call_indirect to a null
  // table entry" -- the JS-visible reflection and the bytecode-visible
  // table disagree. `grow(1)` then a separate `.set(idx, ref)` works in
  // both V8 and JSC; see the seam contract for the repro.
  let tableIdx = -1, tableOk = true, tableErr = null;
  const table = aInst.exports.__indirect_function_table;
  try {
    tableIdx = table.length;
    table.grow(1);
    table.set(tableIdx, bInst.exports.run);
  } catch (e) { tableOk = false; tableErr = String(e && e.stack || e); }

  if (tableOk) {
    aInst.exports.enter_via_table(tableIdx, Math.min(entries, 50_000)); // warm-up, excluded from the timed span
    const t0 = performance.now();
    const sum = aInst.exports.enter_via_table(tableIdx, entries);
    const ms = performance.now() - t0;
    r.table = { ok: true, ms, nsPerEntry: (ms * 1e6) / entries, sum };
  } else {
    r.table = { ok: false, error: tableErr };
  }

  // (b) one host import per entry.
  let importOk = true, importErr = null, importMs = null, importSum = null;
  try {
    aInst.exports.enter_via_import(0, Math.min(entries, 50_000)); // warm-up, excluded from the timed span
    const t0 = performance.now();
    importSum = aInst.exports.enter_via_import(0, entries);
    importMs = performance.now() - t0;
  } catch (e) { importOk = false; importErr = String(e && e.stack || e); }
  r.hostImport = importOk
    ? { ok: true, ms: importMs, nsPerEntry: (importMs * 1e6) / entries, sum: importSum }
    : { ok: false, error: importErr };

  return r;
}

// A -> entry(table) -> B.run -> [B calls a.mmio_load, an import] -> return
// -> B returns -> A returns, repeated `entries` times inside ONE call to
// enter_via_table. This is the exact chain the spec's S5 describes,
// iterative rather than recursive by construction (the repetition is a
// wasm loop, not stack recursion) -- so if the stack or memory were going
// to grow per round trip, this is where it would show.
export function runS5(entries = 1_000_000) {
  let jitEnterTarget = null;
  const importObj = { host: { jit_enter: (_id, pc) => jitEnterTarget.exports.run(pc, 1) } };
  const aBytes = buildEmuModule({ withJitEnterImport: true, tableMin: 2, tableMax: 4 });
  const aInst = new WebAssembly.Instance(new WebAssembly.Module(aBytes), importObj);
  const bBytes = buildTranslatedModule({ mmioSource: 'a-import' });
  const bInst = new WebAssembly.Instance(new WebAssembly.Module(bBytes), { a: { mmio_load: aInst.exports.mmio_load } });
  jitEnterTarget = bInst;

  const table = aInst.exports.__indirect_function_table;
  const tableIdx = table.length;
  table.grow(1); // see runS4's comment: grow-then-set, never the fused 2-arg grow
  table.set(tableIdx, bInst.exports.run);
  aInst.exports.mmio_store(0, 0x55aa);

  const memBefore = memInfo();
  const t0 = performance.now();
  const warmupSum = aInst.exports.enter_via_table(tableIdx, Math.min(entries, 10_000)); // JIT warm-up, excluded from the timed span
  const t1 = performance.now();
  const sum = aInst.exports.enter_via_table(tableIdx, entries);
  const t2 = performance.now();
  const memAfter = memInfo();

  return {
    entries, warmupSum, sum,
    warmupMs: t1 - t0, ms: t2 - t1, nsPerRoundTrip: ((t2 - t1) * 1e6) / entries,
    memBefore, memAfter,
    ok: Number.isFinite(sum),
  };
}

export function runS6() {
  const bytesTarget = 0x1006_0000; // ~257 MiB, the spike's flat guest map (m7/notes.md §5)
  const pages = Math.ceil(bytesTarget / 65536);
  const memBefore = memInfo();
  let ok = true, err = null, aInst = null;
  const t0 = performance.now();
  try {
    const aBytes = buildMemoryOwnerModule({ minPages: pages, maxPages: pages });
    aInst = new WebAssembly.Instance(new WebAssembly.Module(aBytes), {});
  } catch (e) { ok = false; err = String(e && e.stack || e); }
  const instantiateMs = performance.now() - t0;

  let clientOk = null;
  if (ok) {
    try {
      const bBytes = buildMemoryClientModule();
      const bInst = new WebAssembly.Instance(new WebAssembly.Module(bBytes), { a: { memory: aInst.exports.memory } });
      aInst.exports.store(0, 777);
      clientOk = bInst.exports.load(0) === 777;
    } catch (e) { clientOk = 'error: ' + e; }
  }
  const memAfter = memInfo();
  return { pages, bytesTarget, actualBytes: pages * 65536, ok, err, instantiateMs, clientOk, memBefore, memAfter };
}

export function runS7(counts = [2, 50, 500]) {
  const aBytes = buildMemoryOwnerModule({ minPages: 4, maxPages: 4 });
  const aInst = new WebAssembly.Instance(new WebAssembly.Module(aBytes), {});
  const bBytes = buildMemoryClientModule();
  aInst.exports.store(0, 424242);

  const rows = [];
  let total = 0;
  for (const target of counts) {
    const need = target - total;
    const insts = [];
    const t0 = performance.now();
    for (let i = 0; i < need; i++) {
      // A fresh `Module` (a fresh compile) each time, standing in for "a
      // long Studio session recompiles on every shader publish" -- not
      // one module instantiated many times.
      const mod = new WebAssembly.Module(bBytes);
      insts.push(new WebAssembly.Instance(mod, { a: { memory: aInst.exports.memory } }));
    }
    const addedMs = performance.now() - t0;
    total += need;
    const allCorrect = insts.every((inst) => inst.exports.load(0) === 424242);
    rows.push({ target: total, addedMs, msPerInstance: addedMs / need, allCorrect });
  }
  return { rows };
}

export async function runAll() {
  const out = { at: new Date().toISOString() };
  const steps = [['s1', runS1], ['s2', runS2], ['s3', runS3], ['s4', runS4], ['s5', runS5], ['s6', runS6], ['s7', runS7]];
  for (const [key, fn] of steps) {
    try { out[key] = await fn(); }
    catch (e) { out[key] = { error: String(e && e.stack || e) }; }
  }
  return out;
}
