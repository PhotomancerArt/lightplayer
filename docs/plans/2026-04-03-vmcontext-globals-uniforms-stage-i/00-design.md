# Design: LPIR `ptr` / VMContext (stage i)

**Notes:** `00-notes.md`  
**Roadmap:** `docs/roadmaps/2026-04-02-vmcontext-globals-uniforms/`

## Problem

On 64-bit host JIT, **VMContext** and other **native addresses** are currently forced through *
*LPIR `i32` and Cranelift `I32`**, so invoke paths truncate (`as i32`). Cranelift already uses
`pointer_type` for StructReturn, stack addresses, and many builtins; LPIR and the vmctx parameter
are the mismatched piece.

## Goals

1. **LPIR** gains **`IrType::Pointer`** with text keyword **`ptr`** (target-width semantics at
   codegen; interp stores **i32** abstract values — offsets / opaque tokens, not host `usize` — see
   Q4 in notes).
2. **Cranelift JIT (host ISA):** user-function and import signatures use **`pointer_type`** for
   vmctx and for any LPIR `ptr` lowered to CLIF; **`invoke` / `JitModule::call` / `direct_call`**
   pass full-width pointers (no truncation on 64-bit).
3. **WASM32:** module boundary stays **i32**; emitter **lowers `ptr` → i32** where needed (minimal
   churn).
4. **RV32 object / `emu_run`:** guest addresses remain 32-bit; vmctx is a **guest** pointer — may
   need an explicit sub-phase or flag until `ElfLoadInfo` / alloc story is unified (see
   shared-memory / rv32 notes).

## Architecture (data plane)

```text
                    ┌─────────────────────────────────────────┐
                    │              LPIR module                 │
                    │  v0 : ptr (vmctx)  |  SlotAddr → ptr     │
                    │  imports: ptr where ABI needs vmctx     │
                    └───────────────┬─────────────────────────┘
                                    │
           ┌────────────────────────┼────────────────────────┐
           ▼                        ▼                        ▼
   ┌───────────────┐       ┌───────────────┐       ┌─────────────────┐
   │  lpir interp  │       │ lpir-cranelift│       │  lps-wasm   │
   │  ptr as i32   │       │ JIT: ptr →    │       │  ptr → i32      │
   │  (slot_mem    │       │ pointer_type  │       │  (wasm32)       │
   │   offsets)    │       │ invoke native │       │                 │
   └───────────────┘       └───────┬───────┘       └─────────────────┘
                                   │
                           ┌───────┴───────┐
                           ▼               ▼
                    host JIT (x64/…)   object/emu (RV32)
                    full ptr width     I32 / guest ptr
```

**Invariant:** Interp correctness does not require `usize` for `ptr`; **native JIT** does.

## Crate / file map

```text
lp-shader/lpir/
  src/types.rs          IrType::Pointer; ordering / hash if needed
  src/builder.rs        v0 default IrType::Pointer; alloc helpers
  src/parse.rs          "ptr" token in type lists
  src/print.rs          ptr formatting
  src/validate.rs       vmctx vreg type; import callee param injection (ptr);
                        SlotAddr / ptr arithmetic rules (phased)
  src/interp.rs         ptr values as i32 bits (same as I32 path or thin alias)
  src/op.rs             (only if op payloads need type-aware docs)
  src/tests/*.rs        roundtrip + validate + interp

lp-shader/legacy/lpir-cranelift/
  src/emit/mod.rs       signature_for_ir_func: vmctx = pointer_type (JIT host);
                        cranelift_ty_for_vreg for Pointer
  src/emit/call.rs      import / local calls, result pointers
  src/emit/memory.rs    SlotAddr / Load / Store address operands vs Pointer
  src/invoke.rs         arg marshalling: ptr-sized DataValue / registers
  src/call.rs           public call entry (no i32 truncation for vmctx)
  src/direct_call.rs    same
  src/emu_run.rs        guest vmctx width / DataValue (phase or follow-up)
  src/generated_builtin_abi.rs  stay consistent with LPIR import types
  src/builtins.rs
  src/lib.rs            tests: jit_test_vmctx full width on 64-bit host

lp-shader/lps-naga/   vreg_types[0] and call lowering → ptr where required

lp-shader/lps-wasm/   lower LPIR ptr to wasm i32 (comments + mapping)
```

## Phases (tree stays green between phases)

| Phase | Doc                                        | Focus                                                                                     |
|-------|--------------------------------------------|-------------------------------------------------------------------------------------------|
| 1     | `01-phase-lpir-ptr-vmctx.md`               | `IrType::Pointer`, `ptr` text, `v0`, validate/import injection, interp + tests            |
| 2     | `02-phase-cranelift-jit-vmctx-invoke.md`   | Host JIT signatures, emit, invoke/call/direct_call, `lib.rs` tests                        |
| 3     | `03-phase-slotaddr-and-downstream.md`      | SlotAddr (+ address chains) as `ptr` in LPIR; memory emit; `lps-naga`; wasm `ptr`→i32 |
| 4     | `04-phase-rv32-emu-boundary.md` (optional) | Object / `emu_run` guest vmctx, flags or dual path per rv32 notes                         |

Phases 1–2 should land **together** or in **immediate succession** if a split would leave
`cargo check` broken (LPIR says `ptr` but cranelift still assumes `I32` for `v0`). Prefer **one PR
or stacked PRs** with no intermediate broken trunk.

## Validation (any change touching `lp-core/`, `lp-shader/`, `lp-fw/`)

Per `AGENTS.md` / `.cursorrules`:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

**Crate-local:**

```bash
cargo test -p lpir
cargo test -p lpir-cranelift
cargo check -p lps-naga
```

Host server smoke (when engine/server touched):

```bash
cargo check -p lp-server
```

## Out of scope (this stage)

- Changing WASM module to wasm64.
- Full guest/host unified allocator without reading `docs/plans/2026-04-03-shared-memory/`
  decisions.

## Sign-off

Design matches resolved Q1–Q4 in `00-notes.md`. Implementation proceeds via phase docs above.
