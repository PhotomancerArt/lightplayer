# Phase 3: Decouple Lower / Compile / Emit / Frame

## Goal

Remove the remaining `crate::isa::rv32::*` references from the
non-rt_jit, non-link orchestration layer:

- `lower.rs` — SRET threshold via `FuncAbi`/`IsaTarget`.
- `compile.rs` — `func_abi_rv32` dispatch via `IsaTarget`.
- `emit.rs` — `S1` / `ARG_REGS` via `FuncAbi` accessors.
- `crate::abi::frame` — stack alignment via `FuncAbi`.
- `rt_jit/{module,instance}.rs` — `func_abi_rv32` dispatch via `IsaTarget`.

After this phase, the only files outside `crate::isa::rv32::*` itself that
reference `crate::isa::rv32` are `link.rs` (Phase 4 handles it) and the
`emit::EmittedCode` consolidation site (Phase 5).

## Inventory of leakage to remove

| File                            | Current usage                                        | Replacement                                        |
| ------------------------------- | ---------------------------------------------------- | -------------------------------------------------- |
| `lower.rs:16`                   | `use crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD`   | `func_abi.isa().sret_uses_buffer_for(n)`           |
| `compile.rs:89`                 | `crate::isa::rv32::abi::func_abi_rv32(...)`          | dispatch via `match isa { IsaTarget::Rv32imac => func_abi_rv32(...) }` |
| `emit.rs:73`                    | `crate::isa::rv32::abi::S1`                          | `func_abi.sret_preservation_reg()` (already on `FuncAbi`) |
| `emit.rs:158`                   | `use crate::isa::rv32::abi::ARG_REGS` (in `max_outgoing_stack_bytes`) | `func_abi.arg_regs()`                              |
| `abi/frame.rs:69,75,89`         | `& !15u32` (16-byte align hardcode)                  | `align_up(n, func_abi.stack_alignment())`          |
| `rt_jit/module.rs:64`           | `crate::isa::rv32::abi::func_abi_rv32(...)`          | dispatch via `match isa`                           |
| `rt_jit/instance.rs:15`         | `use crate::isa::rv32::abi::func_abi_rv32`           | (delete; constructor moved behind `IsaTarget`)     |

## Steps

### 3.1 `lower.rs::callee_return_uses_sret`

Replace the import-and-compare with the FuncAbi-derived check:

```rust
fn callee_return_uses_sret(func_abi: &FuncAbi, n_scalars: u32) -> bool {
    func_abi.isa().sret_uses_buffer_for(n_scalars)
}
```

Delete the `use crate::isa::rv32::abi::SRET_SCALAR_THRESHOLD;` import at
line 16. Audit other call sites in `lower.rs` that compare against the
threshold inline; route them all through this helper (or directly
`func_abi.isa().sret_uses_buffer_for(...)`).

### 3.2 `compile.rs::compile_module` (and friends)

Replace the hardcoded `func_abi_rv32` call with an `IsaTarget` dispatch.
This dispatch should live on `ModuleAbi::from_ir_and_sig` (already touched
in Phase 1.5). If `compile.rs:89` is calling `func_abi_rv32` directly (not
via `ModuleAbi`), refactor it to go through `ModuleAbi::from_ir_and_sig`
or add a small `IsaTarget::build_func_abi(sig, ...)` helper.

Pattern:

```rust
let func_abi = match session.isa {
    IsaTarget::Rv32imac => crate::isa::rv32::abi::func_abi_rv32(fn_sig, ...),
};
```

`session.isa` (or whatever the threading vehicle is from Phase 1) is
`IsaTarget`. There should be exactly one `match` per dispatch site.

### 3.3 `emit.rs::emit_function_*` — replace `S1` and `ARG_REGS`

`emit.rs:73` uses `crate::isa::rv32::abi::S1` for sret preservation. The
ABI already exposes this via `FuncAbi::sret_preservation_reg()` — use it:

```rust
let sret_preserve = func_abi.sret_preservation_reg();
```

`emit.rs:158` (`max_outgoing_stack_bytes`) uses `ARG_REGS.len()`. Replace:

```rust
fn max_outgoing_stack_bytes(vinsts: &[VInst], func_abi: &FuncAbi) -> u32 {
    let arg_regs = func_abi.arg_regs();
    let mut max_bytes = 0u32;
    for inst in vinsts {
        if let VInst::Call { args, callee_uses_sret, .. } = inst {
            let cap = if *callee_uses_sret { arg_regs.len() - 1 } else { arg_regs.len() };
            let n = args.len();
            if n > cap {
                let stack_words = (n - cap) as u32;
                max_bytes = max_bytes.max(stack_words * 4);
            }
        }
    }
    max_bytes
}
```

The function previously didn't take `&FuncAbi`; thread it through from
`emit_lowered`.

### 3.4 `crate::abi::frame.rs` — generic alignment

Add a helper near the top of `frame.rs`:

```rust
#[inline]
fn align_up(n: u32, align: u32) -> u32 {
    (n.saturating_add(align - 1)) & !(align - 1)
}
```

Then update the three round-up sites:

```rust
// frame.rs:69
let caller_arg_stack_size = align_up(caller_outgoing_stack_bytes, alignment);

// frame.rs:75
align_up(caller_sret_bytes, alignment)

// frame.rs:89
let total_size = align_up(raw_total, alignment);
```

`alignment` comes from the `FuncAbi` already passed to `FrameLayout::compute`
(or whatever the entry point is). If `FrameLayout::compute` doesn't take a
`FuncAbi` today, pass `alignment: u32` instead — pulled from
`func_abi.stack_alignment()` at the call site.

Rename the test `total_size_aligned_16` → `total_size_aligned`. The body
should assert `frame.total_size % func_abi.stack_alignment() == 0`
(passing in an `Rv32imac`-shaped `FuncAbi`, so the value is still 16 in
practice).

### 3.5 `rt_jit/module.rs` and `rt_jit/instance.rs`

`rt_jit/module.rs:64` calls `func_abi_rv32` directly. Replace with the
dispatch already centralized in `ModuleAbi::from_ir_and_sig` (Phase 1.5)
or via the helper added in 3.2.

`rt_jit/instance.rs:15` has a stale `use crate::isa::rv32::abi::func_abi_rv32`
that becomes unused once `module.rs` stops calling it directly. Delete the
import.

### 3.6 Verify

```
rg 'use crate::isa::rv32' lp-shader/lpvm-native/src/lower.rs lp-shader/lpvm-native/src/compile.rs lp-shader/lpvm-native/src/emit.rs lp-shader/lpvm-native/src/abi lp-shader/lpvm-native/src/rt_jit
# Should produce ZERO matches.

rg 'crate::isa::rv32' lp-shader/lpvm-native/src/lower.rs lp-shader/lpvm-native/src/compile.rs lp-shader/lpvm-native/src/emit.rs lp-shader/lpvm-native/src/abi lp-shader/lpvm-native/src/rt_jit
# Should produce ZERO matches.

cargo check -p lpvm-native
cargo test -p lpvm-native
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

## Validation

- `rg 'crate::isa::rv32' lp-shader/lpvm-native/src` reports matches **only**
  in `crate::isa::rv32::*` itself, in `link.rs` (Phase 4), in the
  `EmittedCode`-conversion site in `emit.rs` (Phase 5), and in tests
  (acceptable: tests construct ABIs via `func_abi_rv32` directly).
- `cargo check -p lpvm-native` clean
- `cargo test -p lpvm-native` all green
- ESP32 target check clean
- `frame.rs` no longer hardcodes `15u32` anywhere; `align_up` helper used
  uniformly
- `total_size_aligned` test passes; alignment value comes from `FuncAbi`

## Notes

- The `ARG_REGS` and `S1` references in `emit.rs` are the last leaks from
  the orchestration layer into RV32 specifics. Once Phase 3 completes,
  `emit.rs` only knows about ABI-shape APIs.
- `func_abi.sret_preservation_reg()` already returns the right value
  (loaded from `S1` for RV32 in `func_abi_rv32`); no new accessor needed
  for that one.
- The frame-alignment cleanup is small but architecturally important: it
  removes the last "secretly hardcoded for one ISA" lie from the
  `crate::abi::*` modules.
