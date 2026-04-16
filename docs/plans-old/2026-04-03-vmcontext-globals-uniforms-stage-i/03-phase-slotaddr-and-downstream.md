# Phase 3: SlotAddr / address chains and downstream (`naga`, WASM)

## Objective

Treat **native addresses in LPIR** consistently: **`SlotAddr`** (and *
*`Iadd` / `Isub` / `IaddImm` / `IsubImm`** on address chains used for loads and stores) use **`ptr`
** in validation and type maps. **`emit/memory.rs`** maps LPIR `ptr` to **`pointer_type`** without
redundant “always widen from i32” hacks where the IR is already `ptr`. **`lps-frontend`** sets *
*`vreg_types`** and lowering so vmctx and address vregs are **`ptr`**. **`lps-wasm`** lowers *
*`ptr` → i32** for wasm32.

## Tasks

1. **`validate.rs`** — `Op::SlotAddr` result type `ptr`; rules for `Iadd`/`Isub`/`IaddImm`/`IsubImm`
   and `Load`/`Store` address operands: pointer arithmetic and memory ops expect `ptr` where the
   value is a frame/stack address (match `EmitCtx::vreg_wide_addr` intent). **`Isub`** matters for *
   *base − offset** style addresses, not only **`Iadd`**.
2. **`emit/memory.rs`** — `SlotAddr` produces `pointer_type`; `Load`/`Store` address operands typed
   consistently; remove or simplify `widen_to_ptr` when source is already `ptr`.
3. **`lps-frontend`** — Any place that seeds **`vreg_types[0]`** or slot/import lowering: use *
   *`ptr`** for vmctx and for lowered pointer temps as needed.
4. **`lps-wasm`** — If LPIR text or internal types expose `ptr`, WASM emission maps to **i32**;
   add brief comments at mapping sites.
5. **Filetests / harnesses** — Update any harness that assumed vmctx is always one **i32** word on *
   *host** (may now be pointer-sized in JIT tests only; WASM tests stay i32).

## Exit criteria

- `cargo test -p lpir` and `cargo test -p lpvm-cranelift` pass.
- `cargo check -p lps-frontend` passes.
- WASM pipeline still targets wasm32 i32 at the module boundary.

## Validation

```bash
cargo test -p lpir
cargo test -p lpvm-cranelift
cargo check -p lps-frontend
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```
