# 00 — Notes: lpvm-native ISA decoupling

## Scope of work

Leave `lpvm-native` in a clean architectural state. The crate is new and the
RV32 leakage into supposedly-generic layers (regalloc, link, ModuleAbi,
FuncAbi) is the kind of cruft worth straightening out while it's small and
fresh. A clean ISA boundary also happens to unblock a future ARM port if/when
that becomes interesting (see `docs/reports/2026-04-17-arm-rp2350-effort.md`),
but the goal of *this* plan is hygiene, not ARM prep.

This plan does not write any ARM code, and does not commit `lpvm-native` to
ever supporting more than one ISA. It just stops `crate::rv32::*` from
reaching into modules that have no business knowing the target ISA.

In scope:

- `Alloc::Reg(PReg)` uses `crate::rv32::gpr::PReg` (a `u8`) — should use the
  canonical `crate::abi::PReg` instead.
- `regalloc/{mod,walk,pool,verify,render}.rs` directly import from
  `crate::rv32::gpr` and `crate::rv32::abi` for ABI-shape info that should live
  on `FuncAbi`.
- `ModuleAbi::from_ir_and_sig` hardcodes `func_abi_rv32`; needs an ISA
  dispatch.
- `lower.rs` and `compile.rs` and `emit.rs` reach into `crate::rv32::abi` for
  `SRET_SCALAR_THRESHOLD`, `S1`, `ARG_REGS`.
- `link.rs::patch_call_plt` is hardcoded for RV32 `auipc+jalr` /
  `R_RISCV_CALL_PLT`; needs to become arch-pluggable.
- `FrameLayout` stack alignment is hardcoded to 16; `FuncAbi::stack_alignment`
  always returns 16.
- Two `EmittedCode` types (one in `crate::emit`, one in `crate::rv32::emit`) —
  consolidate.
- `emit_vinsts` is marked DEPRECATED in a doc comment but still present.

Out of scope (deferred to the actual ARM port):

- Writing any ARM emitter / encoder / ABI / emulator.
- Factoring `lp-riscv-emu` into `lp-emu-core` + `lp-riscv-emu`. On closer
  inspection, several "generic" pieces in there (`abi_helper.rs`,
  `function_call.rs`) are Cranelift- and RV32-coupled in non-trivial ways.
  Doing this factor without a second consumer in hand will produce a
  one-target abstraction that creaks immediately. Defer until ARM emulator
  skeleton exists.
- Renaming `crate::rv32` → `crate::isa::rv32` (open question, see Q2).
- Any GLSL / LPIR / frontend / `lp-shader` changes.

## Current state of the codebase

### `crate::rv32::*` leakage outside its own subtree

(Found via `rg 'rv32::' lp-shader/lpvm-native/src` excluding `rv32/` itself.)

**Type leak (most invasive):**

- `regalloc/mod.rs:32` — `Alloc::Reg(crate::rv32::gpr::PReg)`. This is the
  canonical leak: every `AllocOutput` carries an RV32-flavored register type.
- `regalloc/mod.rs:48` — `Alloc::reg() -> Option<PReg>` exposes it.

**ABI-shape leaks in regalloc:**

- `regalloc/mod.rs:202` — `used_callee_saved_from_output` calls
  `gpr::is_callee_saved_pool_gpr` and converts via `abi::PReg::int(r)`.
- `regalloc/pool.rs:3` — imports `ALLOC_POOL`, `PReg` from `rv32::gpr`;
  `RegPool::new()` initializes from RV32's pool order.
- `regalloc/walk.rs:15` — `use crate::rv32::gpr::{self, PReg}`.
- `regalloc/walk.rs:681` — `crate::rv32::abi::SRET_SCALAR_THRESHOLD` hardcoded
  to gate the sret-Ret stack-coercion path.
- `regalloc/verify.rs:8` — uses `gpr::ALLOC_POOL` to verify alloc-within-pool.
- `regalloc/render.rs:9` — uses `gpr::reg_name` for human-readable register
  names.

**Compile-time arch dispatch hardcoded:**

- `compile.rs:89` — `crate::rv32::abi::func_abi_rv32(fn_sig, ...)`.
- `rt_jit/module.rs:64` — same.
- `rt_jit/instance.rs:15` — imports `func_abi_rv32`.
- `emit.rs:9` — `use crate::rv32::emit::emit_function`.
- `emit.rs:73` — `crate::rv32::abi::S1` for sret preservation.
- `emit.rs:158` — `use crate::rv32::abi::ARG_REGS` in
  `max_outgoing_stack_bytes`.
- `lower.rs:16` — `use crate::rv32::abi::SRET_SCALAR_THRESHOLD`, used in
  `callee_return_uses_sret`.
- `abi/func_abi.rs:127` — `ModuleAbi::from_ir_and_sig` hardcodes
  `func_abi_rv32`.

**Tests use rv32 helpers (acceptable):**

- `abi/func_abi.rs`, `abi/frame.rs`, `regalloc/walk.rs`, `regalloc/mod.rs`,
  `regalloc/render.rs`, `regalloc/test/builder.rs`, `emit.rs`,
  `rv32/emit.rs` — tests construct ABIs via `func_abi_rv32`. Fine to leave;
  RV32 is still the only ISA, and the eventual ARM tests will mirror them.

**Acceptable internal use (target leaves):**

- `rv32/gpr.rs`, `rv32/encode.rs`, `rv32/emit.rs`, `rv32/abi.rs`,
  `rv32/debug/*` — these *are* the RV32 leaf, internal references are fine.
- `debug_asm.rs` — RV32-flavored assembly disassembly; lives at the top level
  but is inherently target-specific. Leave alone for now (or split per-ISA
  later when ARM exists).

### `link.rs`

- `patch_call_plt` hardcoded for `auipc+jalr` and `R_RISCV_CALL_PLT` (r_type 17).
- `link_elf` hardcoded for `Architecture::Riscv32` and
  `EF_RISCV_FLOAT_ABI_SOFT`.
- `link_jit` orchestration is generic; only the per-call patching is RV32.

### `FuncAbi` API surface (what regalloc needs that isn't there yet)

`FuncAbi` already exposes: `allocatable`, `precolors`, `call_clobbers`
(== caller-saved), `callee_saved`, `is_sret`, `sret_preservation_reg`,
`param_locs`, `return_method`, `precolor_of`, `sret_word_count`,
`stack_alignment`.

Missing (currently reached around via `crate::rv32::*`):

- `arg_regs() -> &[PReg]` — for `max_outgoing_stack_bytes` and ABI-aware
  walks.
- `allocatable_pool_order() -> &[PReg]` — for `RegPool::new()` LRU init.
- `is_in_allocatable_pool(p) -> bool` — for `verify_allocs_within_pool`.
- `is_caller_saved_pool(p) -> bool` — for the call-clobber path in walk.
- `reg_name(p) -> &'static str` — for `render.rs`.
- `sret_uses_buffer_for(n_scalars) -> bool` — replaces `SRET_SCALAR_THRESHOLD`.
- `stack_alignment` — already exists but hardcoded to 16; needs to come from
  the underlying ISA.

### Duplicated / deprecated bits

- `EmittedCode` exists in both `crate::emit` and `crate::rv32::emit::EmittedCode`.
  The crate-level one has an extra `alloc_output` field; the rv32 one is the
  raw output of `emit_function`. The orchestration converts between them.
- `emit::emit_vinsts` — marked DEPRECATED in a doc comment, still present.
  Looks like there are no callers of it outside its own test.

## Questions

### Q1 — Does the scope match what you want?

**Resolved.** Scope is `lpvm-native` cleanup only. lp-riscv-emu factoring is a
separate concern, deferred. Reframed the scope language: this is hygiene on a
new crate, not "ARM prep" — though it happens to unblock ARM if that's ever
revisited.

### Q2 — Rename `crate::rv32` to `crate::isa::rv32`?

**Resolved.** Already done by user out-of-band. `lp-shader/lpvm-native/src/isa/`
now contains `mod.rs` and `rv32/`. One stray `crate::rv32::` reference
survived inside a `matches!` macro at `regalloc/walk.rs:681`
(`SRET_SCALAR_THRESHOLD`); will be fixed during cleanup since that constant
is going away anyway.

All path references in this notes file should now read `crate::isa::rv32::*`.

Today everything lives at `lp-shader/lpvm-native/src/rv32/`. Adding ARM later
means either a sibling `arm/` next to it or moving both under an `isa/`
parent.

Options:

- (a) Leave as `crate::rv32::*`. ARM later becomes `crate::arm::*`. Slightly
  inconsistent with how typical compiler backends name things, but minimal
  churn.
- (b) Move now to `crate::isa::rv32::*`. ARM later is `crate::isa::arm::*`.
  Signals intent, ~50 file-import touches, mostly mechanical.

Suggested: (b). It's a one-time mechanical rename, the diff is small, and it
makes the eventual second ISA an obvious peer rather than an afterthought.

### Q3 — How to dispatch on target?

**Resolved.** Plain enum, variant named after the actual hardware target:

```rust
pub enum IsaTarget {
    Rv32imac,
    // future: Rv32imc, Thumbv8mMain, etc.
}
```

Carried through `CompileSession` / `ModuleAbi` / `link_*`, dispatched via
`match`. No_std friendly, zero-cost, compiler-enforced exhaustiveness.

The variant name `Rv32imac` describes the **target hardware**
(ESP32-C6 = `riscv32imac-unknown-none-elf`), not the codegen output. The
current emitter only produces base RV32IM instructions; A and C appear in the
target name because the firmware runtime uses them, not because we emit them.
Worth a doc comment near the enum to capture this.

No extension flags today (no dispatch site needs them). If/when extension
awareness becomes useful (compressed codegen pass, runtime FPU detection,
vector-extension target), refactor to a struct-with-flags
(`{ arch, extensions: Bitset }`) at that point. YAGNI applies: zero callers
need it now, refactoring later is mechanical.

### Q4 — How to expose ABI info to regalloc?

**Resolved.** Split by semantic category:

**Category 1 — Per-function ABI shape (call-site dependent):** methods on
`FuncAbi`. Already partly there (`call_clobbers`, `callee_saved`); regalloc
just needs to *use* the FuncAbi accessors instead of asking `gpr::*` directly.
Add:

- `arg_regs() -> &[PReg]`
- `is_caller_saved_pool(p) -> bool` (derived from existing `call_clobbers`)

**Category 2 — Per-target invariants (function-independent):** methods on
`IsaTarget`. These don't depend on signature; they're properties of the ISA.

```rust
impl IsaTarget {
    pub fn allocatable_pool_order(self) -> &'static [PReg] { … }
    pub fn is_in_allocatable_pool(self, p: PReg) -> bool { … }
    pub fn reg_name(self, p: PReg) -> &'static str { … }
    pub fn sret_uses_buffer_for(self, scalars: u32) -> bool { … }
    pub fn stack_alignment(self) -> u32 { … }
}
```

`FuncAbi` carries an `isa: IsaTarget` field; regalloc reaches target-invariant
methods via `func_abi.isa().reg_name(p)` — no extra parameter to thread.

Implementation pattern for `IsaTarget` methods is an internal `match` that
delegates to `crate::isa::rv32::*` constants/functions:

```rust
impl IsaTarget {
    pub fn allocatable_pool_order(self) -> &'static [PReg] {
        match self {
            IsaTarget::Rv32imac => crate::isa::rv32::gpr::ALLOC_POOL_PREG,
        }
    }
}
```

After this, regalloc imports nothing from `crate::isa::rv32`. The rv32 leaf
stays internal to the dispatch layer.

### Q5 — `Alloc::Reg(PReg)` size impact

**Resolved.** Keep `Alloc::Reg(u8)` as a raw hardware-encoding index. Drop
the `crate::isa::rv32::gpr::PReg` import from `regalloc/mod.rs`. The
interpretation of "what does this u8 mean?" lives on `FuncAbi::isa()`.

```rust
pub enum Alloc {
    Reg(u8),       // raw hw encoding; meaning comes from FuncAbi::isa()
    Spill(SpillSlot),
    None,
}
```

Rationale (per `docs/design/native/perf-report/perf-notes.md`):

- The JIT runs on-device. ESP32-C6 has 320 KB heap total with peak ~136 KB
  used during compilation; only ~184 KB headroom.
- Compile is hot-path: 565 ms for a 3877-byte shader on 40 MHz. The team
  already killed per-call allocations to gain FPS (P2 fix); the same
  allocation discipline applies to the compiler itself.
- `RawVecInner::finish_grow` is the #1 alloc hotspot at 416 KB across the
  run. `AllocOutput::allocs` is one of those vecs.
- Cortex-M / RV32 D-caches are small. Cache density of regalloc data
  structures matters in a way it doesn't on the host.
- `RegClass` is pure overhead today: RV32IM has no FPU, every `PReg.class`
  is `Int`. We'd be paying 2× memory and worse cache density for an enum
  field that's structurally always one value.

Conversion to the canonical `crate::abi::PReg` (2 bytes: `hw` + `class`)
happens only at module boundaries — emitter input, debug rendering, link.
Those are out of the hot path. The type fiction in `Alloc::Reg` goes away
by being honest about what `u8` is (a hardware encoding interpreted under a
known ISA), not by widening it.

When float regs eventually land, two paths open up:

1. Reserve a high bit in the `u8` (e.g. `0x80` = float bank). Costs one bit,
   stays `u8`. RV32F, ARM AAPCS, and most ISAs have <128 regs per class.
2. Switch to `(u8, RegClass)` *then*, with measurements in hand and knowing
   the actual cost.

YAGNI. Decide with data when a second register class actually exists.

### Q6 — `link.rs` split now or defer?

**Resolved.** Do it now. The remaining `link.rs` after the split contains
only generic orchestration; everything ISA-flavored moves under
`crate::isa::rv32::link`.

Plan:

1. Add `isa: IsaTarget` parameter to `link_jit(...)` and `link_elf(...)`.
2. Move `patch_call_plt` from `link.rs` into a new
   `crate::isa::rv32::link` module. It's RV32-specific instruction emission
   misfiled in a "generic" file.
3. In `link_jit`, dispatch the per-relocation patcher via a small `match`
   on `isa`.
4. Add `IsaTarget` methods for ELF metadata:
   - `elf_architecture() -> object::Architecture`
   - `elf_e_flags() -> u32`
   `link_elf` calls these instead of hardcoding `Architecture::Riscv32` and
   `EF_RISCV_FLOAT_ABI_SOFT`.
5. Expected relocation r_types (e.g. `R_RISCV_CALL_PLT = 17`) live in the
   per-ISA link module.

Cost is small (~100–200 lines moved + dispatch points). No behavior change
for the existing RV32 path. The ARM patcher (BL/BLX encoding, `R_ARM_CALL`)
slots in as a sibling `crate::isa::arm::link` whenever ARM happens.

Rationale for not deferring: if ARM ever happens, the split has to happen
*anyway* before the first ARM relocation patch — there's no real "wait until
needed" payoff. And `link.rs` is the most architecturally embarrassing leak
in the crate (a file named "link" with hardcoded RV32 instruction bytes
inside it); leaving it would make the cleanup look half-finished.

### Q7 — Include the small consolidations?

**Resolved.** Yes to all three.

**(a) Dedupe `EmittedCode`.** Today there are two structs with the same
name: `crate::emit::EmittedCode` (the canonical, with `alloc_output`) and
`crate::isa::rv32::emit::EmittedCode` (raw output of `emit_function`). The
orchestrator wraps the inner into the outer.

Plan: keep the crate-level `EmittedCode`. Have `rv32::emit::emit_function`
return a small private `Rv32EmitOutput` struct (or a tuple) with just
code+relocs+metadata — no `EmittedCode` name. Orchestrator builds the
canonical `EmittedCode` directly and attaches `alloc_output`.

**(b) Remove `emit::emit_vinsts`.** Verified: no callers anywhere in the
workspace outside its own definition and a re-export in `lib.rs`. It's a
thin compatibility shim that builds a fake `LoweredFunction` and calls
`emit_lowered`. Just delete it and the `lib.rs` re-export.

**(c) Replace `SRET_SCALAR_THRESHOLD` with a `FuncAbi`-derived check.**
Already covered by Q4's Category-2 method
`IsaTarget::sret_uses_buffer_for(scalar_count: u32) -> bool`. Update the two
non-rv32 callers (`lower.rs::callee_return_uses_sret` and the
`regalloc/walk.rs:681` `matches!`) to use `func_abi.isa().sret_uses_buffer_for(n)`.
The constant stays as a private impl detail in `crate::isa::rv32::abi`,
consumed only by the rv32 implementation of that method.

### Q8 — Stack alignment

**Resolved.** Include in the FuncAbi enrichment phase.

The hardcoding is bigger than "one line": `FuncAbi::stack_alignment` returns
literal 16, *and* `crate::abi::frame.rs` has three round-up sites
(`frame.rs:69`, `:75`, `:89`) all hardcoding `& !15u32`. Plus a test named
`total_size_aligned_16`.

Plan:

1. Add `IsaTarget::stack_alignment() -> u32` (already in Q4 Category 2 list).
2. `FuncAbi::stack_alignment()` becomes `self.isa.stack_alignment()`.
3. Replace the three `& !15u32` round-ups in `frame.rs` with a small helper
   that takes alignment from the `FuncAbi` already in scope:
   ```rust
   fn align_up(n: u32, align: u32) -> u32 {
       (n.saturating_add(align - 1)) & !(align - 1)
   }
   ```
4. RV32 returns 16. ARM AAPCS would return 8 if/when added.
5. Rename `total_size_aligned_16` test to `total_size_aligned` and assert
   against `func_abi.stack_alignment()` instead of literal 16.

Cost: ~20-line diff. No behavior change for RV32. Removes the architectural
lie that `crate::abi::frame` is "generic" while secretly assuming one
alignment.

# Notes
