# Phase 4 (optional): RV32 object / `emu_run` guest vmctx

## Objective

Align **32-bit guest** execution with LPIR **`ptr`** semantics: on **RV32**, **`ptr` is 32 bits**. **`emu_run`** and **object module** paths use a **guest** vmctx value (linear memory / guest RAM notion), not a truncated **host** stack pointer. This phase may depend on **`docs/plans/2026-04-03-shared-memory/`** and **`milestone-1-rv32-notes.md`** for allocation and **`ElfLoadInfo`**.

## Tasks (sketch)

1. **`emu_run.rs`** — `DataValue` width and vmctx argument match **guest** pointer size (32-bit for RV32).
2. **`emit/mod.rs` (object)** — Keep or unify vmctx param type: **I32** or **pointer_type** for RV32 ISA (both are 32-bit on that target); document choice.
3. **Dual path cleanup** — If Phase 2 introduced JIT-vs-object cfg branches, reduce duplication once guest vmctx story is clear.

## Exit criteria

- `cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu`
- Emulator tests that compile and run shaders still pass (e.g. `fw-tests` as listed in `AGENTS.md` when this code path is exercised).

## Validation

```bash
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
```

(Adjust test targets if the repo’s CI names differ.)
