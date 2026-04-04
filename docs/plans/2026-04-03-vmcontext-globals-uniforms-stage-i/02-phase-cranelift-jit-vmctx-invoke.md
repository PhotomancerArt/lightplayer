# Phase 2: Cranelift JIT — vmctx and invoke at native pointer width

## Objective

For **host JIT** targets, Cranelift signatures treat **vmctx (`v0`)** and LPIR **`ptr`** values as **`pointer_type`**. Remove **`as i32`** truncation on vmctx (and other `ptr` args) in **`call.rs`**, **`direct_call.rs`**, and **`invoke.rs`**. **`lib.rs`** tests pass a real pointer (e.g. `usize` / `*const ()`) through the JIT boundary on 64-bit hosts.

## Tasks

1. **`emit/mod.rs`**
   - `signature_for_ir_func`: first param (vmctx) uses `pointer_type` when compiling for the **JIT host ISA** (not a hard-coded `types::I32` except where RV32 object path explicitly requires 32-bit — see Phase 4 or cfg split).
   - `cranelift_ty_for_vreg`: map `IrType::Pointer` → `pointer_type`.
2. **`invoke.rs`** — Argument lowering: vmctx and `ptr` operands use pointer-sized **`DataValue`** / register class expected by the ABI (replace “everything scalar is i32” assumptions for these slots).
3. **`call.rs` / `direct_call.rs`** — Pass vmctx through without narrowing to `i32` on 64-bit; keep 32-bit paths only for explicit 32-bit backends if still needed.
4. **`emit/call.rs`** — Re-check import calls: vmctx argument type matches signature; result-pointer paths unchanged except for consistency with `pointer_type`.
5. **`generated_builtin_abi.rs` / `builtins.rs`** — Align with LPIR import types (vmctx + `ptr` params) so there is no mix of `I32` and `pointer_type` for the same logical slot.
6. **`lib.rs` tests** — `jit_test_vmctx` (and similar): use full-width pointer on host; assert call succeeds without truncation bugs.

## RV32 / object module

If **`emit/mod.rs`** today forces `I32` vmctx for **`enable_multi_ret_implicit_sret`**, split by **compilation mode**:

- **JIT host:** `pointer_type` for vmctx.
- **RV32 object / emu:** remain **32-bit** until Phase 4 unifies guest vmctx handling.

Document the cfg or `CompileOptions` field that selects the path.

## Exit criteria

- `cargo test -p lpir-cranelift` passes on **64-bit host**.
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server` (if `lp-glsl` / engine pulls these crates).
- No regression for existing JIT filetests / builtins.

## Validation

```bash
cargo test -p lpir-cranelift
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```
