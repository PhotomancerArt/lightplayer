# Cranelift RV32 Backend Review

**Date:** 2026-04-05
**Repo:** `light-player/lp-cranelift` (fork of `bytecodealliance/wasmtime`)
**Branch:** `main`
**Upstream issue:** [bytecodealliance/wasmtime#5572](https://github.com/bytecodealliance/wasmtime/issues/5572) — "32 bit targets in Cranelift"

## Executive Summary

The `lp-cranelift` fork adds a full `riscv32` ISA backend to Cranelift, forked
from the existing `riscv64` backend. It is the only known working 32-bit
Cranelift backend. The work is functional for the LightPlayer use case
(RV32IMAC, integer + fixed-point math, no 64-bit division), but has
significant structural issues that would need addressing before upstream
contribution.

**Scope of what was attempted:**
- 32-bit integer arithmetic: full support (working)
- 64-bit integer data type: basic support — loads, stores, register pairs,
  add/sub/shift/compare, some bitwise ops (working, with caveats)
- 64-bit mul/div: explicitly not attempted for full support
- 128-bit: zero work done, dead code copied from RV64 I128 patterns
- Vector: explicitly deferred, commented out
- Floating point: wired through from RV64 (F/D extensions), untested on RV32

**Overall assessment:** The approach is sound — forking RV64 and adapting it to
RV32 is the right strategy. The execution is uneven: the ABI and basic
lowering work, but the codebase carries a lot of cargo-culted RV64 code that
doesn't make sense on RV32, and the I128 ISLE rules are structurally
impossible given the current `ValueRegs` capacity. A methodical cleanup pass
is needed.

---

## 1. Architecture & Approach

### 1.1 What was done

The backend was created by copying `cranelift/codegen/src/isa/riscv64/` to
`cranelift/codegen/src/isa/riscv32/` and making targeted adaptations:

- **New ISA registration** in `cranelift/codegen/src/isa/mod.rs` and meta
  (`cranelift/codegen/meta/src/isa/riscv32.rs`)
- **Separate ISLE compilation** — `isle_riscv32.rs` generated from its own
  `inst.isle`, `inst_vector.isle`, and `lower.isle`
- **ABI adapted to ILP32** — `word_bits()` → 32, `word_bytes()` → 4, stores
  use `SW`/`LW` instead of `SD`/`LD` for frame pointer and return address
- **I64 as register pair** — `rc_for_type(I64)` returns
  `[RegClass::Int, RegClass::Int]` / `[I32, I32]`
- **Extensive ISLE lowering rules** for I64 arithmetic using the same
  two-register-pair pattern that RV64 uses for I128
- **Pre-compilation validator** that rejects unsupported operations early
  (i64 div/rem, bmask, etc.)
- **124 filetests** in `cranelift/filetests/filetests/32bit/`

### 1.2 The RV64 I128 → RV32 I64 analogy

The core architectural idea is correct: on RV64, `I128` is represented as a
pair of 64-bit registers and lowered with carry/borrow propagation. On RV32,
`I64` should be represented as a pair of 32-bit registers using the same
pattern. The ISLE rules in `lower.isle` follow this approach for:

- `iadd` / `isub` (with carry/borrow)
- `band` / `bor` / `bxor` / `bnot`
- `ishl` / `ushr` / `sshr` (with cross-register shifting)
- `rotl` / `rotr`
- `icmp` (multi-register comparison)
- `uextend` / `sextend`
- `select`
- `uadd_overflow` / `usub_overflow` / `sadd_overflow` / `ssub_overflow`

This is the bulk of the work and it appears largely correct in structure.

---

## 2. Bugs and Correctness Issues

### 2.1 CRITICAL: `LD` instruction still emitted on RV32

The `LD` (load doubleword) instruction is RV64-only. Despite commit
`53ff2444ee` ("prevent LD instruction generation on RV32"), `LD` is still
present in multiple code paths:

- **`LoadOP::from_type(I64)` returns `LoadOP::Ld`** (`inst/args.rs:1153`).
  This is called from `Inst::gen_load` whenever an I64 load is requested.
  On RV32, an I64 load should decompose into two `LW` instructions.

- **`LoadInlineConst` emit** uses `ld` in its pretty-print and emits an
  8-byte load (`inst/mod.rs:1159`), but on RV32 there is no `LD` instruction.
  The `LoadInlineConst` for I64 should use `lw` twice, or the code should
  only use I32-typed inline constants.

- **`LoadExtNameFar`** emits `LoadOP::Ld` for loading external name addresses
  (`inst/emit.rs:2027-2030`). On RV32, addresses are 32-bit, so this should
  use `LoadOP::Lw`.

- **Compressed `c.ld` / `c.ldsp`** patterns in the emit path
  (`inst/emit.rs:574-666`) — these are RV64C instructions that don't exist
  on RV32C.

- **`BrTable` emit** (`inst/emit.rs:1430-1438`) extends the index to 64 bits
  using `Extend { from_bits: 32, to_bits: 64 }`, which then does a 64-bit
  shift (meaningless on RV32). The table entry load also uses `LD`.

### 2.2 `Extend` instruction uses 64-bit shift width

In `inst/emit.rs:1149`, the `Extend` instruction uses `(64 - from_bits)` for
the shift amount. On RV32, registers are 32 bits wide, so this should be
`(32 - from_bits)`. The RV64 version correctly uses 64 here.

This means sign/zero extension of types narrower than 32 bits (I8, I16)
produces incorrect shift amounts: instead of `slli rd, rs, 24; srai rd, rd, 24`
for sign-extending I8, it would do `slli rd, rs, 56; srai rd, rd, 56`. Since
RV32 shift amounts are masked to 5 bits (mod 32), `56 mod 32 = 24`, so this
*accidentally works* for I8 (56 mod 32 = 24) and I16 (48 mod 32 = 16). But
it's wrong by coincidence, not by design, and the pretty-print output is
misleading.

### 2.3 `load_constant_u64` used where `load_constant_u32` should be

Several places in the backend call `Inst::load_constant_u64` when they should
use `Inst::load_constant_u32`:

- `abi.rs:329` — `gen_sp_reg_adjust` uses `load_constant_u64` for stack
  adjustment. The `load_constant_u64` path may emit a `LoadInlineConst` with
  `ty: I64`, which (per 2.1) tries to use `LD`.

- `abi.rs:1189` — `gen_probestack_unroll` uses `load_constant_u64`.

- Various emit paths use `load_constant_u64` for address calculations.

### 2.4 I128 `rc_for_type` returns 4 registers, but `ValueRegs` holds max 2

`rc_for_type(I128)` returns 4 register classes (`[Int, Int, Int, Int]`), but
`VALUE_REGS_PARTS` is hardcoded to 2 in `machinst/valueregs.rs:9`. This means
any I128 value would overflow the `ValueRegs` array. The I128 ISLE lowering
rules in `lower.isle` reference `value_regs_get x 0` and `value_regs_get x 1`
(only 2 parts), which means they're treating I128 as a 2-register pair — but
each "register" would be 32-bit, giving only 64 bits total.

This is a known non-goal (128-bit was not attempted), but the dead code is
misleading. The `rc_for_type` entry should either be removed or gated behind
an error.

### 2.5 `Addw`/`Subw`/other `*w` instructions removed but still referenced

The RV64 `*w` (word-width) instructions (`addw`, `subw`, `sllw`, etc.) were
removed from the RV32 `args.rs` (they don't exist on RV32 — all operations
are already 32-bit). However, the ISLE-generated enum variants still exist in
the generated code because `inst.isle` is shared infrastructure. Comments like
"Note: RV64 Addw removed" appear throughout `args.rs`, which is fine, but
the emit path has some inconsistencies where it references removed variants.

---

## 3. Structural / Design Issues

### 3.1 Copy-paste from RV64 without full adaptation

The backend was created by copying RV64 wholesale. Many areas were adapted
correctly, but the copy-paste approach means:

- **Comments reference RV64 concepts.** For example, `isle.rs:41` defines
  `RV64IsleContext` — the struct is named `RV64` even though it's in the
  riscv32 backend.

- **`load_constant_u64`** exists alongside `load_constant_u32` in
  `inst/mod.rs`. On RV64, `load_constant_u64` is the primary function. On
  RV32, `load_constant_u32` should be primary. The `u64` variant should be
  either removed or reserved for constructing 64-bit values across register
  pairs.

- **Rounding mode / FPU / Vector boilerplate** is copied verbatim. The FPU
  paths reference `F128` storage in 4 integer registers, but no floating
  point operations on F128 are actually supported. Vector support is
  commented out but the boilerplate remains.

- **`dynamic_vector_bytes` returns 16** — copied from RV64, probably never
  tested on RV32.

### 3.2 The pre-compilation validator is a workaround, not a solution

The `validator/` module (`instruction.rs`, `supported.rs`, `types.rs`) runs
before lowering to reject unsupported operations. This is a defensive measure
that catches operations the ISLE rules can't lower, but:

- It duplicates information that should be expressed in the ISLE rules
  themselves (if an operation isn't supported, the ISLE rules should simply
  not match it, and the lowering will fail with a proper error).

- It's incomplete — it checks some operations but not all problem areas.

- It allocates a `HashMap` on every function validation call
  (`extension_requirements()` in `supported.rs`).

- The upstream Cranelift approach is to let ISLE matching failures produce
  errors, not to pre-validate. A pre-validator could make sense for better
  error messages, but the current implementation is more of a band-aid.

### 3.3 `no_std` initialization for `MachineEnv`

`abi.rs:637-657` implements a `no_std` version of `OnceLock` using raw
`static mut` with `AtomicBool` guards. This is technically unsound
(data race on `MACHINE_ENV` between the write in one thread and read in
another — `Acquire`/`Release` on the bool does not establish happens-before
for the `MaybeUninit` write). In practice it works because the ESP32-C6 is
single-core for the RV32 use case, but it would be UB under Miri or on
multi-core.

### 3.4 The `Extend` instruction's 64-bit assumption

The `Extend` machine instruction takes `from_bits` and `to_bits` parameters.
On RV64, `to_bits` is always 64. On RV32, `to_bits` should be at most 32
for single-register values. The ABI's `gen_extend` doesn't enforce this.
Combined with 2.2, this means `Extend` is conceptually carrying RV64
semantics.

---

## 4. I64 Support Assessment

The I64 register-pair support is the most interesting part of the work. It
follows the correct pattern from RV64's I128 support.

### What works

| Operation | Status | Notes |
|-----------|--------|-------|
| `iconst` I64 | Works | Materializes into two registers |
| `iadd` I64 | Works | With carry propagation |
| `isub` I64 | Works | With borrow propagation |
| `band`/`bor`/`bxor` I64 | Works | Pair-wise operations |
| `bnot` I64 | Works | |
| `ishl`/`ushr`/`sshr` I64 | Works | Cross-register shift logic |
| `rotl`/`rotr` I64 | Works | |
| `icmp` I64 | Works | Multi-register comparison |
| `uextend` to I64 | Works | Zero-extends into pair |
| `sextend` to I64 | Works | Sign-extends into pair |
| `select` I64 | Works | Multi-register select |
| `uadd_overflow` I64 | Works | Returns pair + overflow flag |

### What's missing or broken

| Operation | Status | Notes |
|-----------|--------|-------|
| `imul` I64 | Partial | Uses `mulhu`/`mul` for cross-products; likely works for basic cases but the algorithm wasn't fully verified |
| `udiv`/`sdiv` I64 | Missing | Explicitly rejected by validator. Would need a software division routine (libcall or inline). |
| `urem`/`srem` I64 | Missing | Same as div |
| `smulhi`/`umulhi` I64 | Present | Rules exist but correctness unclear |
| `load` I64 | Broken | Uses `LD` instruction (see 2.1) |
| `store` I64 | Likely broken | Needs two `SW` instructions |
| `bswap` I64 | Missing | Rejected by validator |
| `bitrev` I64 | Missing | Rejected by validator |
| `popcnt` I64 | Unclear | Rule exists but may have issues |
| `clz`/`ctz` I64 | Unclear | Rules exist |
| `cls` I64 | Unclear | Rule exists |

### The load/store gap

This is the most serious I64 correctness issue. On RV32, loading/storing an
I64 value requires two 32-bit memory operations. The current code calls
`LoadOP::from_type(I64)` which returns `LoadOP::Ld` — an instruction that
doesn't exist on RV32. The ISLE rules for I64 loads/stores need to decompose
into pairs of `LW`/`SW` with appropriate offset arithmetic.

---

## 5. What Would Be Needed for Upstream

If you wanted to post about this on the upstream issue (#5572) or contribute
the work, here's what would need to happen:

### 5.1 Minimum viable upstream PR

1. **Fix `ValueRegs` capacity.** The upstream comment says "we cap the
   capacity at four (when any 32-bit target is enabled)". Change
   `VALUE_REGS_PARTS` to 4 when `riscv32` feature is enabled, add
   `ValueRegs::three()` and `ValueRegs::four()` constructors.

2. **Fix I64 loads/stores.** Decompose into paired `LW`/`SW`. This is the
   single biggest correctness gap.

3. **Remove all `LD`/`SD`/`LWU` references.** These are RV64-only. Audit
   every `LoadOP` and `StoreOP` usage.

4. **Remove dead I128 code or gate it properly.** Either remove the I128
   patterns from `lower.isle` and `rc_for_type`, or implement them properly
   (requires ValueRegs capacity = 4).

5. **Rename `RV64IsleContext` to `RV32IsleContext`.**

6. **Fix `Extend` shift widths.** Use 32, not 64.

7. **Audit `load_constant_u64` vs `load_constant_u32` usage.**

8. **Remove the pre-compilation validator** (or convert to better error
   messages) — upstream would want ISLE to handle this natively.

### 5.2 For complete 64-bit support

- I64 load/store decomposition
- I64 mul (verify the cross-product algorithm)
- I64 div/rem via libcall (`__divdi3`, `__udivdi3`, `__moddi3`, `__umoddi3`
  from compiler-rt/libgcc)
- I64 `bswap`, `bitrev`, `popcnt`, `clz`, `ctz`
- Thorough filetest coverage for all I64 operations

### 5.3 For complete 128-bit support

- `ValueRegs` capacity = 4
- All I128 ISLE rules rewritten for 4-register tuples
- I128 load/store as 4x `LW`/`SW`
- I128 arithmetic with multi-level carry propagation
- This is a large amount of work and probably not worth doing until the I64
  story is solid

---

## 6. Code Quality Notes

- **3833 lines of ISLE** in `lower.isle` (vs 3155 for RV64). The RV32 version
  is *larger* because it has both the I64 pair rules and many I128 rules
  copied verbatim.

- **124 filetests** is a good start. Many are debug/regression tests for
  specific bugs encountered during development, which suggests iterative
  debugging rather than systematic coverage.

- **Comments are generally helpful** — the RV32-specific adaptations are
  well-commented (e.g., "On RV32, return addresses are 32 bits, so use LW
  instead of LD").

- **The validator** is over-engineered for what it does (full extension
  registry, HashMap, etc.) but the idea of catching unsupported ops early
  has merit for user-facing error messages.

- **`no_std` support** is present throughout, which is important for the
  LightPlayer use case.

---

## 7. Recommendations

### For the GitHub post (issue #5572)

The work demonstrates that a 32-bit Cranelift backend is feasible with the
current architecture. Key points worth sharing:

1. The RV64 I128 pattern generalizes cleanly to RV32 I64 (register pairs with
   carry/borrow propagation).
2. `ValueRegs` capacity of 2 is sufficient for I64-on-RV32, but would need
   to be 4 for I128-on-RV32.
3. The main pain points are: (a) many RV64 assumptions baked into the shared
   `machinst` layer (64-bit pointers, `LD`/`SD` availability), (b) the ISLE
   shared prelude assumes `fits_in_64` means "fits in a register", (c) the
   `*w` instruction variants need to be conditionally available.
4. A clean 32-bit backend would benefit from an abstraction like
   `XLEN`-parameterized types in the shared infrastructure.

### For improving the code

Priority order:
1. Fix I64 loads/stores (decompose to paired LW/SW)
2. Audit and remove all LD/SD/LWU usage
3. Fix `Extend` shift widths
4. Clean up naming (RV64IsleContext → RV32IsleContext)
5. Remove dead I128 code
6. Add I64 div/rem via libcalls
7. Remove or simplify the validator
