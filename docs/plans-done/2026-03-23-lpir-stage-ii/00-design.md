# LPIR Stage II — Design

## Scope of work

Implement the `lpir` Rust crate: core IR types, flat Op encoding, builder API,
text format printer, text format parser (nom), interpreter, and basic
validation. Validate with unit tests using hand-built and parsed IR.

Spec: `docs/lpir/` (chapters 00–09).
Roadmap: `docs/roadmaps/2026-03-21-lpir/stage-ii.md`.

## File structure

```
lp-shader/lpir/
├── Cargo.toml                      # NEW: no_std + alloc; deps: nom, nom_locate
└── src/
    ├── lib.rs                      # NEW: public API, re-exports
    ├── types.rs                    # NEW: IrType, VReg, SlotId, VRegRange, CalleeRef
    ├── op.rs                       # NEW: Op enum (flat, ~50 variants)
    ├── module.rs                   # NEW: IrModule, IrFunction, ImportDecl, SlotDecl
    ├── builder.rs                  # NEW: FunctionBuilder, ModuleBuilder
    ├── print.rs                    # NEW: text format printer (IrModule → String)
    ├── parse.rs                    # NEW: text format parser (String → IrModule)
    ├── validate.rs                 # NEW: well-formedness checks
    └── interp.rs                   # NEW: interpreter

Cargo.toml                          # UPDATE: add "lp-shader/lpir" to members
```

## Architecture

```
                         IrModule
                  ┌─────────┴─────────┐
           imports: Vec<ImportDecl>    functions: Vec<IrFunction>
           (@std.math::fsin, ...)      (@main, @helper, ...)
                                       │
                              ┌────────┼────────────┐
                              │        │            │
                         vreg_types  slots        body: Vec<Op>
                         Vec<IrType>  Vec<SlotDecl>  │
                         (v0:f32,     (ss0: 64,      │  ┌─ vreg_pool: Vec<VReg>
                          v1:i32,      ss1: 16)      │  │  (shared operand storage
                          ...)                        │  │   for Call + Return)
                                                      │  │
                              ┌────────────────────────┘  │
                              ▼                           │
               ┌──────────────────────────────┐           │
               │ Flat op stream (one Vec<Op>) │           │
               │                              │           │
               │  FconstF32 { dst, value }    │           │
               │  Fadd { dst, lhs, rhs }      │           │
               │  IfStart { cond, offsets }    │    VRegRange ──► vreg_pool
               │    Iadd { dst, lhs, rhs }    │           │
               │  Else                        │           │
               │    Isub { dst, lhs, rhs }    │           │
               │  End                         │           │
               │  Call { callee, args, res }──┼───────────┘
               │  Return { values }───────────┘
               └──────────────────────────────┘
```

## Key design decisions

### Flat encoding with markers

Control flow is encoded as marker ops (`IfStart`, `Else`, `LoopStart`,
`SwitchStart`, `CaseStart`, `DefaultStart`, `End`) in a single `Vec<Op>` per
function. No nested `Vec<Op>` inside control flow variants. This minimizes enum
size, eliminates heap fragmentation from nested allocations, and is
cache-friendly. Matches how WebAssembly bytecode encodes structured control
flow.

Marker ops carry `u32` skip-offsets for O(1) jumps (e.g., `IfStart.else_offset`
points to `Else` or `End`; `LoopStart.end_offset` points to `End`). The
builder patches these on block close.

Generic `End` op (not typed per construct) — matches WASM precedent. Consumer
tracks nesting with a small stack.

### VRegPool for call/return operands

`IrFunction` holds a `vreg_pool: Vec<VReg>`. `Op::Call` and `Op::Return` store
`VRegRange { start: u32, count: u16 }` pointing into the pool. This keeps the
Op enum small (max ~16 bytes payload) while supporting variable-arity calls.
Two allocations per function total: `body` + `vreg_pool`.

### Op enum sizing

All variants fit in ~16 bytes of payload (largest: `Select` with 4 VRegs,
`Call` with callee + two VRegRanges). With discriminant and alignment, estimated
~20 bytes per op. Every op in the function pays the same size — no waste from
oversized control flow variants.

### Parser: nom + nom_locate

`nom` provides composable parser combinators with `no_std` support. `nom_locate`
adds span tracking for error messages with line/column. Exception to the "no
external deps" rule; justified by parser quality-of-life.

## Core types

```rust
// type
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum IrType { F32, I32 }

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct VReg(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct SlotId(pub u32);

#[derive(Clone, Copy, Debug)]
pub struct VRegRange {
    pub start: u32,
    pub count: u16
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct CalleeRef(pub u32);
// Index into a combined callee table on IrModule:
//   0..imports.len()                 → import
//   imports.len()..imports.len()+N   → local function

// module.rs
pub struct IrModule {
    pub imports: Vec<ImportDecl>,
    pub functions: Vec<IrFunction>,
}

pub struct ImportDecl {
    pub module_name: String,    // "std.math"
    pub func_name: String,      // "fsin"
    pub param_types: Vec<IrType>,
    pub return_types: Vec<IrType>,
}

pub struct SlotDecl {
    pub size: u32,
}

pub struct IrFunction {
    pub name: String,
    pub is_entry: bool,
    pub param_count: u16,
    pub return_types: Vec<IrType>,
    pub vreg_types: Vec<IrType>,   // indexed by VReg.0
    pub slots: Vec<SlotDecl>,
    pub body: Vec<Op>,             // flat op stream
    pub vreg_pool: Vec<VReg>,      // shared pool for Call/Return operands
}
```

## Op enum (abbreviated)

```rust
pub enum Op {
    // ── Float arithmetic ──
    Fadd { dst: VReg, lhs: VReg, rhs: VReg },
    Fsub { dst: VReg, lhs: VReg, rhs: VReg },
    Fmul { dst: VReg, lhs: VReg, rhs: VReg },
    Fdiv { dst: VReg, lhs: VReg, rhs: VReg },
    Fneg { dst: VReg, src: VReg },

    // ── Integer arithmetic ──
    Iadd { dst: VReg, lhs: VReg, rhs: VReg },
    Isub { dst: VReg, lhs: VReg, rhs: VReg },
    Imul { dst: VReg, lhs: VReg, rhs: VReg },
    IdivS { dst: VReg, lhs: VReg, rhs: VReg },
    IdivU { dst: VReg, lhs: VReg, rhs: VReg },
    IremS { dst: VReg, lhs: VReg, rhs: VReg },
    IremU { dst: VReg, lhs: VReg, rhs: VReg },
    Ineg { dst: VReg, src: VReg },

    // ── Float comparisons ──
    Feq { dst: VReg, lhs: VReg, rhs: VReg },
    Fne { dst: VReg, lhs: VReg, rhs: VReg },
    Flt { dst: VReg, lhs: VReg, rhs: VReg },
    Fle { dst: VReg, lhs: VReg, rhs: VReg },
    Fgt { dst: VReg, lhs: VReg, rhs: VReg },
    Fge { dst: VReg, lhs: VReg, rhs: VReg },

    // ── Integer comparisons (signed) ──
    Ieq { dst: VReg, lhs: VReg, rhs: VReg },
    Ine { dst: VReg, lhs: VReg, rhs: VReg },
    IltS { dst: VReg, lhs: VReg, rhs: VReg },
    IleS { dst: VReg, lhs: VReg, rhs: VReg },
    IgtS { dst: VReg, lhs: VReg, rhs: VReg },
    IgeS { dst: VReg, lhs: VReg, rhs: VReg },

    // ── Integer comparisons (unsigned) ──
    IltU { dst: VReg, lhs: VReg, rhs: VReg },
    IleU { dst: VReg, lhs: VReg, rhs: VReg },
    IgtU { dst: VReg, lhs: VReg, rhs: VReg },
    IgeU { dst: VReg, lhs: VReg, rhs: VReg },

    // ── Logic / bitwise ──
    Iand { dst: VReg, lhs: VReg, rhs: VReg },
    Ior { dst: VReg, lhs: VReg, rhs: VReg },
    Ixor { dst: VReg, lhs: VReg, rhs: VReg },
    Ibnot { dst: VReg, src: VReg },
    Ishl { dst: VReg, lhs: VReg, rhs: VReg },
    IshrS { dst: VReg, lhs: VReg, rhs: VReg },
    IshrU { dst: VReg, lhs: VReg, rhs: VReg },

    // ── Constants ──
    FconstF32 { dst: VReg, value: f32 },
    IconstI32 { dst: VReg, value: i32 },

    // ── Immediate variants ──
    IaddImm { dst: VReg, src: VReg, imm: i32 },
    IsubImm { dst: VReg, src: VReg, imm: i32 },
    ImulImm { dst: VReg, src: VReg, imm: i32 },
    IshlImm { dst: VReg, src: VReg, imm: i32 },
    IshrSImm { dst: VReg, src: VReg, imm: i32 },
    IshrUImm { dst: VReg, src: VReg, imm: i32 },
    IeqImm { dst: VReg, src: VReg, imm: i32 },

    // ── Casts ──
    FtoiSatS { dst: VReg, src: VReg },
    FtoiSatU { dst: VReg, src: VReg },
    ItofS { dst: VReg, src: VReg },
    ItofU { dst: VReg, src: VReg },

    // ── Select / Copy ──
    Select { dst: VReg, cond: VReg, if_true: VReg, if_false: VReg },
    Copy { dst: VReg, src: VReg },

    // ── Memory ──
    SlotAddr { dst: VReg, slot: SlotId },
    Load { dst: VReg, base: VReg, offset: u32 },
    Store { base: VReg, offset: u32, value: VReg },
    Memcpy { dst_addr: VReg, src_addr: VReg, size: u32 },

    // ── Control flow markers ──
    IfStart { cond: VReg, else_offset: u32, end_offset: u32 },
    Else,
    LoopStart { end_offset: u32 },
    SwitchStart { selector: VReg, end_offset: u32 },
    CaseStart { value: i32, end_offset: u32 },
    DefaultStart { end_offset: u32 },
    End,

    // ── Control flow jumps ──
    Break,
    Continue,
    BrIfNot { cond: VReg },

    // ── Call / Return ──
    Call { callee: CalleeRef, args: VRegRange, results: VRegRange },
    Return { values: VRegRange },
}
```

## Builder API

```rust
pub struct FunctionBuilder {
    ...
}

impl FunctionBuilder {
    pub fn new(name: &str, return_types: &[IrType]) -> Self;
    pub fn set_entry(&mut self);
    pub fn add_param(&mut self, ty: IrType) -> VReg;
    pub fn alloc_vreg(&mut self, ty: IrType) -> VReg;
    pub fn alloc_slot(&mut self, size: u32) -> SlotId;

    // Scalar / memory ops
    pub fn push(&mut self, op: Op);

    // Control flow (stack-based, patches offsets on close)
    pub fn push_if(&mut self, cond: VReg);
    pub fn push_else(&mut self);
    pub fn end_if(&mut self);
    pub fn push_loop(&mut self);
    pub fn end_loop(&mut self);
    pub fn push_switch(&mut self, selector: VReg);
    pub fn push_case(&mut self, value: i32);
    pub fn push_default(&mut self);
    pub fn end_switch(&mut self);

    // Call / return (appends to vreg_pool)
    pub fn push_call(&mut self, callee: CalleeRef, args: &[VReg], results: &[VReg]);
    pub fn push_return(&mut self, values: &[VReg]);

    pub fn finish(self) -> IrFunction;
}

pub struct ModuleBuilder {
    ...
}

impl ModuleBuilder {
    pub fn new() -> Self;
    pub fn add_import(&mut self, decl: ImportDecl) -> CalleeRef;
    pub fn add_function(&mut self, func: IrFunction) -> CalleeRef;
    pub fn finish(self) -> IrModule;
}
```

## Interpreter

```rust
pub trait ImportHandler {
    fn call(
        &mut self,
        module_name: &str,
        func_name: &str,
        args: &[Value],
    ) -> Result<Vec<Value>, InterpError>;
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    F32(f32),
    I32(i32),
}

pub fn interpret(
    module: &IrModule,
    func_name: &str,
    args: &[Value],
    imports: &mut dyn ImportHandler,
) -> Result<Vec<Value>, InterpError>;
```

Interpreter state per frame:

- `regs: Vec<Value>` indexed by VReg
- `slot_mem: Vec<u8>` (flattened; slot offsets from cumulative sizes)
- `pc: usize` (index into body)
- Loop context stack for break/continue targeting

## Phases

```
1. Crate scaffold + core types + Op enum
2. FunctionBuilder + text format printer
3. Text format parser (nom) + round-trip tests
4. Interpreter
5. Validator
6. Cleanup & validation
```
