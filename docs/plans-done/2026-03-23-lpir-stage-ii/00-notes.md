# LPIR Stage II — Implementation Notes

## Scope of work

Implement the `lpir` Rust crate: core IR types (`Op`, `IrFunction`, `IrModule`,
`IrType`, `VReg`), a builder API, text format printer, text format parser,
interpreter, and basic validation. Validate with unit tests using hand-built IR.

This corresponds to Stage II of the LPIR roadmap
(`docs/roadmaps/2026-03-21-lpir/stage-ii.md`).

Spec: `docs/lpir/` (chapters 00–09).

## Current state

- **No `lpir` crate exists.** The `lp-shader/` directory has `lp-glsl-naga`,
  `lp-glsl-wasm`, `lp-glsl-cranelift`, and others, but no `lpir/` subdirectory.
- The LPIR spec is complete (10 chapters in `docs/lpir/`).
- Existing crates use `#![no_std]` + `extern crate alloc`; the `lpir` crate
  should follow this pattern.
- The roadmap estimates ~900 lines impl + ~400 lines tests.

## Questions

### 1. Crate naming and path

The roadmap spec overview says `lp-shader/lpir/`. Other crates under `lp-shader/`
follow a `lp-glsl-<name>` convention (e.g. `lp-glsl-naga`, `lp-glsl-wasm`).

Should the crate be:

- `lp-shader/lpir/` with package name `lpir` (as in roadmap)
- `lp-shader/lp-glsl-lpir/` with package name `lp-glsl-lpir` (matching convention)

**Suggested**: `lp-shader/lpir/` with package name `lpir`. The `lpir` crate is the
IR core and will be depended on by `lp-glsl-naga` (for lowering), `lp-glsl-wasm`
(for emission), and `lp-glsl-cranelift`. A shorter name reflects that it is a
foundational module, not just another `lp-glsl-*` plugin.

**Answer**: `lp-shader/lpir/` with package name `lpir`.

---

### 2. Op / Statement representation

The LPIR is "flat" (no expression trees) but has structured control flow
(`if`/`else`, `loop`, `switch`). These nest arbitrarily. Two main options:

**Option A — single `Op` enum**: Every instruction and control construct is an
`Op` variant. Control flow variants contain `Vec<Op>` for bodies.

```rust
enum Op {
    Fadd { dst: VReg, lhs: VReg, rhs: VReg },
    // ...
    If { cond: VReg, then_body: Vec<Op>, else_body: Vec<Op> },
    Loop { body: Vec<Op> },
    Switch { selector: VReg, cases: Vec<SwitchCase>, default: Option<Vec<Op>> },
    Break,
    Continue,
    BrIfNot { cond: VReg },
    Return { values: SmallVec<VReg> },
    // ...
}
```

**Option B — Statement + Op split**: A `Statement` enum wraps control flow and
contains an `Op` variant for value-producing instructions.

```rust
enum Statement {
    Op { op: Op },
    If { ... },
    Loop { ... },
    // ...
}
enum Op {
    Fadd { dst: VReg, lhs: VReg, rhs: VReg },
    // only scalar / value-producing ops
}
```

**Suggested**: Option A (single enum). Simpler, fewer indirections, matches the
spec's model where everything is a "statement line." The `Op` name might be
misleading for control flow; we could name it `Stmt` instead, but the roadmap
uses `Op`.

**Answer**: Single `Op` enum, but **flat encoding with markers** (WASM bytecode
style). One `Vec<Op>` per function; no nested `Vec<Op>` inside control flow
variants. Control flow uses open/close marker ops (`IfStart`/`Else`/`IfEnd`,
`LoopStart`/`LoopEnd`, `SwitchStart`/`CaseStart`/`DefaultStart`/`SwitchEnd`)
with `u32` skip-offsets for O(1) jumps. This minimizes enum size (all variants
small), eliminates fragmentation from nested allocations, and is cache-friendly.
Builder uses stack-based open/close API that patches offsets on close.

---

### 3. IrModule scope

The spec defines modules (function declarations, imports, entry functions). The
roadmap Stage II mentions `IrModule`.

Should Stage II include a full `IrModule` type that holds multiple `IrFunction`s
and import declarations? Or should we start with just `IrFunction` (standalone)
and add `IrModule` later when we have Naga lowering (Stage IV)?

**Suggested**: Include `IrModule` from the start. It's needed for:

- Import declarations (referenced by `call @std.math::fsin(...)` etc.)
- Multiple function definitions
- Entry function marking
- Well-formedness checks (call targets must exist)

The type is small:

```rust
struct IrModule {
    imports: Vec<ImportDecl>,
    functions: Vec<IrFunction>,
}
```

**Answer**: Include `IrModule` from the start. Needed for imports, multi-function
modules, entry marking, and well-formedness checks.

---

### 4. VReg and type tracking

The spec says virtual registers have dense indices (`v0..v{N-1}`) with a fixed
type per register. Options:

**Option A — newtype + parallel type array in IrFunction**:

```rust
struct VReg(u32);
// In IrFunction:
vreg_types: Vec<IrType>,  // indexed by VReg.0
```

**Option B — newtype encodes type (e.g. high bit)**:
Not practical for 2 types; wastes space.

**Suggested**: Option A. Simple, cache-friendly. `vreg_count` is
`vreg_types.len()`.

**Answer**: `VReg(u32)` newtype + parallel `vreg_types: Vec<IrType>` in
`IrFunction`. Simple, cache-friendly, no type encoded in VReg value.

---

### 5. Builder API design for control flow

For scalar ops, the builder pattern is clear:

```rust
let v2 = builder.alloc_vreg(IrType::F32);
builder.push(Op::Fadd { dst: v2, lhs: v0, rhs: v1 });
```

For control flow, options:

**Option A — direct construction**: Build `Vec<Op>` for bodies and pass them:

```rust
let then_body = vec![Op::Fadd { dst: v2, lhs: v0, rhs: v1 }];
let else_body = vec![];
builder.push(Op::If { cond, then_body, else_body });
```

**Option B — nested builder with closures**:

```rust
builder.push_if(cond, |b| {
    b.push(Op::Fadd { dst: v2, lhs: v0, rhs: v1 });
}, |b| {});
```

**Superseded** — see question 2b below (flat encoding changes builder design).

**Answer**: Superseded by flat encoding decision.

---

### 2b. Call operand storage (side pool vs inline)

With flat encoding, every Op variant should be small. Most ops are 12 bytes or
under (3 × `u32`). But `Call` has variable-arity arguments and results.

**Option A — side pool (Cranelift-style ValueList)**:
`IrFunction` holds a shared `vreg_pool: Vec<VReg>`. Each `Call` stores a
`(start: u32, count: u16)` range into the pool for args and results:

```rust
Op::Call {
    callee: CalleeRef,
    args: VRegRange,     // { start: u32, count: u16 }
    results: VRegRange,  // { start: u32, count: u16 }
}
```

Total: ~16-20 bytes. Pool grows monotonically; no per-call allocation.

**Option B — inline small array**:

```rust
Op::Call {
    callee: CalleeRef,
    args: [VReg; 4],
    arg_count: u8,
    results: [VReg; 4],
    result_count: u8,
}
```

Total: ~42 bytes. Blows up the enum size for every Op variant. Bad.

**Suggested**: Option A (side pool). One extra `Vec<VReg>` per function, all
call arg/result VRegs packed contiguously. Keeps `Op::Call` small. Same pattern
as Cranelift `ValueList`.

Also considered inline marker ops (`CallArg`, `CallResult` in the op stream) to
avoid any second allocation, but this makes the op stream noisier and iteration
more protocol-dependent. Two allocations per function (body + pool) is fine; the
fragmentation concern was about dozens of nested `Vec<Op>` per function, not two
predictable monotonic allocations.

**Answer**: VRegPool side allocation. `IrFunction` holds a `vreg_pool: Vec<VReg>`.
`Op::Call` and `Op::Return` store `VRegRange { start: u32, count: u16 }` into the
pool for args, results, and return values.

---

### 6. Multi-return in `return` statement

The text format grammar currently says:

```
return_stmt = "return" [ vreg ]
```

But the spec allows multi-return functions (`func @foo() -> (f32, f32, f32)`)
and the well-formedness rules state: "a parenthesized return type in the
declaration corresponds to multiple parallel returned scalars."

This means `return` needs to support multiple VRegs:

```
return v0, v1, v2
```

The grammar should be: `return_stmt = "return" [ vreg { "," vreg } ]`

This is a minor spec gap. In the `Op` enum, `Return` uses `VRegRange` into
the pool (same as `Call`).

**Suggested**: Fix the grammar in the spec and use `Return { values: VRegRange }`
in the implementation.

**Answer**: Fixed. Grammar in `docs/lpir/07-text-format.md` updated:
`return_stmt = "return" [ vreg { "," vreg } ]`. Also fixed `switch_case` to use
`integer_literal` instead of `uint_literal` (case labels can be negative since
selector is `i32`). `Op::Return` uses `VRegRange` into the pool.

---

### 7. Interpreter scope

Stage II says "Interpreter: execute IrFunction with concrete inputs, return
results." Stage III says "extend the interpreter… with comprehensive coverage."

How much should the Stage II interpreter cover?

**Suggested**: Stage II interpreter covers:

- All arithmetic, comparison, logic, constant, immediate, cast, select, copy ops
- Control flow: if/else, loop, break, continue, br_if_not, switch, return
- Memory: slot, slot_addr, load, store, memcpy
- Calls to local functions (same module)
- Import calls: stub mechanism (caller provides closures/function pointers)
- Multi-return

Stage III then adds: comprehensive edge-case tests, validator tests, round-trip
hardening. The interpreter itself should be complete in Stage II; Stage III is
about test coverage.

**Answer**: Interpreter is functionally complete in Stage II (all ops, control
flow, memory, local calls, import stubs). Stage III adds thorough test coverage
and edge-case verification, not new interpreter features.

---

### 8. Validation scope

The roadmap says "Validation: basic well-formedness checks." What should be
checked vs deferred to Stage III?

**Suggested Stage II checks**:

- VReg defined before use
- VReg type consistency (same type on redefinition)
- `break`/`continue`/`br_if_not` only inside `loop`
- `slot_addr` references a declared slot
- `return` arity matches function signature
- `call` arity matches callee signature
- Switch case uniqueness
- At most one `entry func`

Stage III adds: comprehensive negative tests, edge cases, better error messages.

**Answer**: Stage II includes the full validation checklist above. Stage III adds
comprehensive negative testing and diagnostic hardening.

---

### 9. `SmallVec` and allocation strategy

The crate must be `no_std + alloc`, no external deps. `SmallVec` would be an
external dependency. Options:

- Use `Vec<VReg>` everywhere (simpler, one allocation per Return/Call)
- Use a custom inline-capable small vec (more work)
- Allow `tinyvec` as a dep (it supports no_std)

**Suggested**: Just use `Vec<VReg>`. The overhead of one small heap allocation
per call/return op is negligible. Keeps the "no external deps" rule intact.

**Answer**: `Vec` everywhere. `nom` + `nom_locate` allowed as parser dependencies
(good error messages, span support). Otherwise no external deps.

---

### 10. `switch` case label type

The spec says case labels are integer constants. The grammar says
`switch_case = "case" uint_literal ...`. But should case labels be `i32`
(signed) or `u32` (unsigned)? The selector is `i32`.

Looking at GLSL: switch labels are integer constants (can be negative).

**Suggested**: Case labels should be `i32` (signed), matching the selector type.
The grammar should use `integer_literal` (which allows negative) instead of
`uint_literal` for case labels. Another minor spec gap.

**Answer**: Fixed together with Q6. Grammar updated to `integer_literal` for
case labels. `SwitchCase` in Op uses `i32`.
