// M7 P2 browser-seam probe -- throwaway wasm byte generator.
//
// This is a hand-rolled, minimal wasm binary assembler: just enough opcodes
// to build the toy modules S1-S7 need. It is deliberately not a general
// compiler -- every module builder in probe-tests.mjs is a few instructions
// long and readable top to bottom. `wabt`/`wat2wasm` may not be installed on
// the measuring machine, so bytes are emitted directly, per the phase spec.
//
// Pure ESM, no dependencies. Runs identically in a browser module Worker, in
// Node's worker_threads, and in Bun.

// ---- LEB128 -----------------------------------------------------------

function uleb(nIn) {
  let n = BigInt(nIn);
  const out = [];
  do {
    let byte = Number(n & 0x7fn);
    n >>= 7n;
    if (n !== 0n) byte |= 0x80;
    out.push(byte);
  } while (n !== 0n);
  return out;
}

function sleb(nIn) {
  let n = BigInt(nIn);
  const out = [];
  let more = true;
  while (more) {
    let byte = Number(n & 0x7fn);
    n >>= 7n;
    const signBitSet = (byte & 0x40) !== 0;
    if ((n === 0n && !signBitSet) || (n === -1n && signBitSet)) {
      more = false;
    } else {
      byte |= 0x80;
    }
    out.push(byte);
  }
  return out;
}

// ---- byte writer --------------------------------------------------------
// Chunks are Uint8Arrays; concatenation happens once at the end so a 6 MB
// data section is one allocation + one copy, never a byte-by-byte push or a
// spread (spreading a multi-million-element array into a function call blows
// the stack).

class W {
  constructor() {
    this.chunks = [];
    this.len = 0;
  }
  raw(bytesLike) {
    const u8 = bytesLike instanceof Uint8Array ? bytesLike : Uint8Array.from(bytesLike);
    this.chunks.push(u8);
    this.len += u8.length;
    return this;
  }
  u8(v) { return this.raw([v & 0xff]); }
  uleb(n) { return this.raw(uleb(n)); }
  sleb(n) { return this.raw(sleb(n)); }
  name(s) {
    const b = new TextEncoder().encode(s);
    this.uleb(b.length);
    return this.raw(b);
  }
  // Writes a vector: count (uleb) then each item via `fn(item, writer)`.
  vec(items, fn) {
    this.uleb(items.length);
    for (const it of items) fn(it, this);
    return this;
  }
  // Writes another writer's bytes as a length-prefixed sub-section body.
  section(id, body) {
    this.u8(id);
    this.uleb(body.len);
    for (const c of body.chunks) this.raw(c);
    return this;
  }
  toBytes() {
    const out = new Uint8Array(this.len);
    let o = 0;
    for (const c of this.chunks) { out.set(c, o); o += c.length; }
    return out;
  }
}

// ---- value types / opcodes ------------------------------------------

const I32 = 0x7f;
const FUNCREF = 0x70;

// Instruction helpers: each returns a plain array of bytes. These are the
// only opcodes the probe's toy modules need.
const Op = {
  end: () => [0x0b],
  unreachable: () => [0x00],
  block: (blockType = 0x40) => [0x02, blockType],
  loop: (blockType = 0x40) => [0x03, blockType],
  br: (depth) => [0x0c, ...uleb(depth)],
  brIf: (depth) => [0x0d, ...uleb(depth)],
  return: () => [0x0f],
  call: (idx) => [0x10, ...uleb(idx)],
  callIndirect: (typeIdx, tableIdx = 0) => [0x11, ...uleb(typeIdx), ...uleb(tableIdx)],
  drop: () => [0x1a],
  localGet: (idx) => [0x20, ...uleb(idx)],
  localSet: (idx) => [0x21, ...uleb(idx)],
  localTee: (idx) => [0x22, ...uleb(idx)],
  globalGet: (idx) => [0x23, ...uleb(idx)],
  globalSet: (idx) => [0x24, ...uleb(idx)],
  i32Load: (align = 2, offset = 0) => [0x28, ...uleb(align), ...uleb(offset)],
  i32Store: (align = 2, offset = 0) => [0x36, ...uleb(align), ...uleb(offset)],
  memorySize: () => [0x3f, 0x00],
  memoryGrow: () => [0x40, 0x00],
  i32Const: (n) => [0x41, ...sleb(n)],
  i32Eqz: () => [0x45],
  i32Eq: () => [0x46],
  i32LtU: () => [0x49],
  i32GeU: () => [0x4f],
  i32Add: () => [0x6a],
  i32Sub: () => [0x6b],
  i32Mul: () => [0x6c],
  i32Xor: () => [0x73],
};

function flatten(instrs) {
  const out = [];
  for (const i of instrs) out.push(...i);
  return out;
}

// ---- module builder --------------------------------------------------
//
// `spec` shape (only the fields a given module needs):
//   types:   [[paramTypes[], resultTypes[]], ...]
//   imports: [{ mod, name, kind: 'func'|'mem'|'table'|'global', typeIdx?, mem?, table?, mutable? }]
//   funcs:   [{ typeIdx, locals: [I32,...], body: instrs[] }]   // defined funcs, in order
//   table:   { min, max? }                                       // one table defined in this module
//   memory:  { min, max? }                                       // one memory defined in this module
//   globals: [{ mutable, init: instrs[] }]
//   exports: [{ name, kind: 'func'|'mem'|'table'|'global', index }]
//   elems:   [{ offset: instrs[], funcIdxs: [] }]                 // active, table 0
//   data:    [{ offset: instrs[], bytes: Uint8Array }]            // active, mem 0
//
// Index spaces (per the wasm spec) are imports-first: e.g. func index 0..k-1
// are imported funcs, k.. are this module's own funcs, in declaration order.
function buildModule(spec) {
  const types = spec.types || [];
  const imports = spec.imports || [];
  const funcs = spec.funcs || [];
  const globals = spec.globals || [];
  const exports = spec.exports || [];
  const elems = spec.elems || [];
  const datas = spec.data || [];

  const mod = new W();
  mod.raw([0x00, 0x61, 0x73, 0x6d]); // magic
  mod.raw([0x01, 0x00, 0x00, 0x00]); // version 1

  // 1. Type
  if (types.length) {
    const b = new W();
    b.vec(types, ([params, results], w) => {
      w.u8(0x60);
      w.vec(params, (t, w2) => w2.u8(t));
      w.vec(results, (t, w2) => w2.u8(t));
    });
    mod.section(1, b);
  }

  // 2. Import
  if (imports.length) {
    const b = new W();
    b.vec(imports, (imp, w) => {
      w.name(imp.mod);
      w.name(imp.name);
      if (imp.kind === 'func') { w.u8(0x00); w.uleb(imp.typeIdx); }
      else if (imp.kind === 'table') { w.u8(0x01); w.u8(FUNCREF); writeLimits(w, imp.table); }
      else if (imp.kind === 'mem') { w.u8(0x02); writeLimits(w, imp.mem); }
      else if (imp.kind === 'global') { w.u8(0x03); w.u8(I32); w.u8(imp.mutable ? 1 : 0); }
      else throw new Error('bad import kind ' + imp.kind);
    });
    mod.section(2, b);
  }

  // 3. Function
  if (funcs.length) {
    const b = new W();
    b.vec(funcs, (f, w) => w.uleb(f.typeIdx));
    mod.section(3, b);
  }

  // 4. Table (at most one defined here, beyond any imported table)
  if (spec.table) {
    const b = new W();
    b.uleb(1);
    b.u8(FUNCREF);
    writeLimits(b, spec.table);
    mod.section(4, b);
  }

  // 5. Memory (at most one defined here, beyond any imported memory)
  if (spec.memory) {
    const b = new W();
    b.uleb(1);
    writeLimits(b, spec.memory);
    mod.section(5, b);
  }

  // 6. Global
  if (globals.length) {
    const b = new W();
    b.vec(globals, (g, w) => {
      w.u8(I32);
      w.u8(g.mutable ? 1 : 0);
      w.raw(flatten(g.init));
      w.raw(Op.end());
    });
    mod.section(6, b);
  }

  // 7. Export
  if (exports.length) {
    const b = new W();
    b.vec(exports, (e, w) => {
      w.name(e.name);
      const kindByte = { func: 0x00, table: 0x01, mem: 0x02, global: 0x03 }[e.kind];
      w.u8(kindByte);
      w.uleb(e.index);
    });
    mod.section(7, b);
  }

  // 9. Element (active, table 0, MVP encoding: flags=0 then direct funcidx vec)
  if (elems.length) {
    const b = new W();
    b.vec(elems, (el, w) => {
      w.uleb(0); // flags: active, table index 0 implied
      w.raw(flatten(el.offset));
      w.raw(Op.end());
      w.vec(el.funcIdxs, (fi, w2) => w2.uleb(fi));
    });
    mod.section(9, b);
  }

  // 10. Code
  if (funcs.length) {
    const b = new W();
    b.vec(funcs, (f, w) => {
      const body = new W();
      // locals: grouped runs of same type; our functions only ever use I32
      // locals, so this is one run (or none).
      const localRuns = (f.locals && f.locals.length) ? [[f.locals.length, I32]] : [];
      body.vec(localRuns, ([count, t], w2) => { w2.uleb(count); w2.u8(t); });
      body.raw(flatten(f.body));
      body.raw(Op.end());
      w.uleb(body.len);
      for (const c of body.chunks) w.raw(c);
    });
    mod.section(10, b);
  }

  // 11. Data (active, mem 0)
  if (datas.length) {
    const b = new W();
    b.vec(datas, (d, w) => {
      w.uleb(0); // flags: active, mem index 0 implied
      w.raw(flatten(d.offset));
      w.raw(Op.end());
      w.uleb(d.bytes.length);
      w.raw(d.bytes);
    });
    mod.section(11, b);
  }

  return mod.toBytes();
}

function writeLimits(w, { min, max }) {
  if (max === undefined || max === null) { w.u8(0x00); w.uleb(min); }
  else { w.u8(0x01); w.uleb(min); w.uleb(max); }
}

export { buildModule, Op, I32, FUNCREF, uleb, sleb, W };
