# lpvm-native ISA Decoupling — Design

## Scope of Work

Leave `lpvm-native` in a clean architectural state. The crate is new and
the RV32 leakage into supposedly-generic layers (`regalloc`, `link.rs`,
`ModuleAbi`, `FuncAbi`, `crate::abi::frame`) is the kind of cruft worth
straightening out while it's small and fresh. A clean ISA boundary also
happens to unblock a future ARM port if/when that becomes interesting (see
`docs/reports/2026-04-17-arm-rp2350-effort.md`), but the goal of *this*
plan is hygiene, not ARM prep.

This plan does not write any ARM code, and does not commit `lpvm-native`
to ever supporting more than one ISA. It just stops `crate::isa::rv32::*`
from reaching into modules that have no business knowing the target ISA.

For the full design rationale and Q&A that produced this plan, see
[`00-notes.md`](./00-notes.md).

## File Structure (added/touched)

```
lp-shader/lpvm-native/src/
├── isa/
│   ├── mod.rs                          # UPDATE: add IsaTarget enum + impl
│   └── rv32/
│       ├── abi.rs                      # UPDATE: expose pool order, name fns; SRET_SCALAR_THRESHOLD becomes private impl detail
│       ├── gpr.rs                      # (unchanged)
│       ├── emit.rs                     # UPDATE: emit_function returns Rv32EmitOutput, not EmittedCode
│       └── link.rs                     # NEW: patch_call_plt, R_RISCV_CALL_PLT, ELF arch/flags
├── abi/
│   ├── func_abi.rs                     # UPDATE: add isa: IsaTarget field, accessors; ModuleAbi dispatches via IsaTarget
│   ├── frame.rs                        # UPDATE: round-ups use func_abi.stack_alignment(); align_up helper
│   └── mod.rs                          # (unchanged)
├── regalloc/
│   ├── mod.rs                          # UPDATE: Alloc::Reg(u8); use FuncAbi/IsaTarget accessors
│   ├── walk.rs                         # UPDATE: drop crate::isa::rv32 imports; SRET via FuncAbi.isa()
│   ├── pool.rs                         # UPDATE: init from IsaTarget::allocatable_pool_order()
│   ├── verify.rs                       # UPDATE: use IsaTarget::is_in_allocatable_pool()
│   └── render.rs                       # UPDATE: use IsaTarget::reg_name()
├── lower.rs                            # UPDATE: SRET via FuncAbi.isa()
├── compile.rs                          # UPDATE: dispatch FuncAbi construction via IsaTarget
├── emit.rs                             # UPDATE: ARG_REGS / S1 via FuncAbi accessors; one EmittedCode type
├── link.rs                             # UPDATE: orchestration only; patcher dispatch via IsaTarget
├── rt_jit/
│   ├── module.rs                       # UPDATE: dispatch FuncAbi via IsaTarget
│   └── instance.rs                     # UPDATE: drop func_abi_rv32 import
└── lib.rs                              # UPDATE: drop emit_vinsts re-export
```

## Conceptual Architecture

```
        IsaTarget (enum)
             │
             ├── per-target invariants (ISA-only):
             │     allocatable_pool_order, is_in_allocatable_pool,
             │     reg_name, sret_uses_buffer_for, stack_alignment,
             │     elf_architecture, elf_e_flags
             │
             └── delegates via match → crate::isa::rv32::* (private leaf)


        FuncAbi
        ├── isa: IsaTarget
        ├── per-function ABI shape:
        │     allocatable, precolors, call_clobbers, callee_saved,
        │     param_locs, return_method, arg_regs (NEW),
        │     is_caller_saved_pool (NEW)
        └── stack_alignment() → self.isa.stack_alignment()


   regalloc / lower / compile / emit / frame
        │
        └── reach ABI/ISA info via FuncAbi (+ FuncAbi.isa())
            zero direct imports of crate::isa::rv32::*


   link.rs (generic orchestration)
        │
        └── dispatch per-relocation patch via match isa { ... }
                                                    │
                                                    └─→ crate::isa::rv32::link
```

## Main Components

### `IsaTarget`

```rust
pub enum IsaTarget {
    Rv32imac,
    // future: Rv32imc, Thumbv8mMain, etc.
}
```

Variant name describes the **target hardware** (ESP32-C6 =
`riscv32imac-unknown-none-elf`), not the codegen output. The current emitter
produces only base RV32IM instructions; A and C appear in the target name
because the firmware runtime uses them. Doc comment captures this.

All ISA-specific knowledge funnels through methods on `IsaTarget`. Each
method is a `match` that delegates to `crate::isa::rv32::*` constants/fns.
A new ISA = a new variant + a new arm in each method. No hidden coupling.

### `FuncAbi`

Gains an `isa: IsaTarget` field. Constructors take `IsaTarget`. Adds two
new per-function accessors so regalloc no longer reaches into `crate::isa::rv32::gpr`:

- `arg_regs() -> &[PReg]`
- `is_caller_saved_pool(p: PReg) -> bool` (derived from existing `call_clobbers`)

Existing `stack_alignment()` becomes `self.isa.stack_alignment()` instead
of returning literal 16.

### `ModuleAbi`

`from_ir_and_sig` takes `IsaTarget` and dispatches `FuncAbi` construction
via `match` instead of hardcoding `func_abi_rv32`.

### `Alloc::Reg`

Stays `u8`. The hardware-encoding semantics live on `FuncAbi::isa()`. No
memory growth in the regalloc hot path.

`crate::abi::PReg` (`{ hw, class }`) remains the canonical type at module
boundaries (emitter input, debug rendering, link). Conversion happens only
out of the hot path.

### `link.rs`

Becomes generic orchestration only. RV32 instruction emission and
relocation r_types move to `crate::isa::rv32::link`. `link_jit` /
`link_elf` take an `IsaTarget` parameter and dispatch via `match`.

## Key Decisions

- **`enum IsaTarget` with hardware-named variants.** No traits, no generics.
  `no_std` friendly, zero-cost, compiler-enforced exhaustiveness, only ever
  2-3 ISAs. Future extension flags (float / compressed / vector) are YAGNI
  until a dispatch site needs them.
- **Two-tier ABI surface on `FuncAbi`:** per-function shape stays on
  `FuncAbi`; per-target invariants go on `IsaTarget`, accessible via
  `func_abi.isa()`. Regalloc imports nothing from `crate::isa::rv32::*`.
- **`Alloc::Reg(u8)` not `Alloc::Reg(PReg)`.** On-device JIT, 320 KB heap,
  hot-path data structure, `RegClass` would be pure overhead today.
- **`link.rs` split now, not later.** Most architecturally embarrassing
  leak in the crate; ARM port would need this anyway.
- **No behavior change for RV32.** Every phase compiles, tests pass.

# Phases

## Phase 1: IsaTarget + FuncAbi Plumbing
Pure additive. Introduce `IsaTarget`, plumb through constructors. No
existing call sites change behavior.

## Phase 2: Decouple Regalloc
Remove all `crate::isa::rv32::*` imports from `regalloc/`. Switch to
`FuncAbi`/`IsaTarget` accessors.

## Phase 3: Decouple Lower / Compile / Emit / Frame
Remove `crate::isa::rv32::*` imports from `lower.rs`, `compile.rs`,
`emit.rs`, `rt_jit/`, and the alignment hardcoding from `crate::abi::frame`.

## Phase 4: Decouple link.rs
Extract `crate::isa::rv32::link`. `link.rs` becomes generic orchestration.

## Phase 5: Final Consolidations
Dedupe `EmittedCode`, delete `emit::emit_vinsts`.
