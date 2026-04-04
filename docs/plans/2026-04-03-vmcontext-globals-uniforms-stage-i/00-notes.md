# Plan notes: LPIR pointer / VMContext type (stage i)

Roadmap: `docs/roadmaps/2026-04-02-vmcontext-globals-uniforms/`  
Reference: `docs/roadmaps/2026-04-02-vmcontext-globals-uniforms/milestone-1-jit-notes.md`

## Scope of work

**End-to-end:** Introduce **target-width pointer** semantics in LPIR and thread them through **Cranelift JIT + invoke + direct call paths** so 64-bit hosts do not truncate VMContext or other pointer-sized values. **WASM** stays **wasm32 / i32** at the module boundary (linear memory indices and locals); only light doc or assert updates if LPIR text gains `ptr`.

**Goals**

1. **VMContext** — LPIR `v0` typed as pointer width; Cranelift user-function and import signatures use `pointer_type` for that argument on native JIT ISAs; `invoke` / `JitModule::call` / `direct_call` pass a full-width pointer (no `as i32` truncation on 64-bit).
2. **Other pointer sites (Cranelift path)** — Audit and align anything that is semantically an address in generated **host** code: e.g. **StructReturn** / **result-pointer** hidden args (already often `pointer_type` in CLIF), **stack slot addresses** (`SlotAddr` → `stack_addr`), **Load/Store** address operands, and **future array / out-param** surfaces as they appear in LPIR/lowering. Where LPIR still uses `i32` for an address, either widen to `IrType::Pointer` or document a deliberate “offset in i32” split for 32-bit-only targets.
3. **Emulator / object** — RV32 object + `emu_run` remain **32-bit guest addresses**; vmctx must be a **guest** pointer (see `rv32-notes.md`), not host stack — may land in same plan or adjacent phase if shared `ElfLoadInfo` / alloc work is ready.
4. **`lp-glsl-naga`** — Lowering must produce correct `vreg_types[0]` and any new pointer-typed vregs; update builtin / call lowering as needed.

**Likely crate touch list**

| Area | Crates / files (non-exhaustive) |
|------|----------------------------------|
| LPIR IR | `lpir`: `types`, `module`, `builder`, `parse`, `print`, `validate`, `interp`, tests |
| Cranelift emit | `lpir-cranelift`: `emit/mod.rs` (vmctx param + `cranelift_ty_for_vreg`), `emit/call.rs`, `emit/memory.rs`, `module_lower.rs` |
| Builtins ABI | `lpir-cranelift`: `generated_builtin_abi.rs`, `builtins.rs` (already uses `pointer_type` in places — reconcile with LPIR) |
| JIT call | `lpir-cranelift`: `call.rs`, `direct_call.rs`, `invoke.rs`, `lib.rs` tests using `jit_test_vmctx` |
| Emulator glue | `lpir-cranelift`: `emu_run.rs` (guest vmctx + DataValue width) |
| Frontend | `lp-glsl-naga`: lowering that sets `vreg_types` and call operands |
| WASM | `lp-glsl-wasm`: **minimal** — types stay i32; comments / mapping if LPIR uses `ptr` (emit still lowers ptr → i32 for wasm32) |
| Filetests / engine | Any harness that assumes vmctx is always one i32 word on host |

## Current state of the codebase

### `lpir`

- `IrType` is only `F32` | `I32` (`types.rs`).
- `VMCTX_VREG` is always `v0`; `FunctionBuilder::new` seeds `vreg_types[0] = IrType::I32` with comment “VMContext pointer (32-bit)” (`builder.rs`).
- `ImportDecl::needs_vmctx`: validator injects `IrType::I32` as first logical param for callee matching (`validate.rs` ~347).
- `parse.rs` / `print.rs`: only `i32` / `f32` type tokens.
- `interp.rs`: call setup assumes vmctx + user args; value storage is per `IrType` (F32/I32).
- `emit` in `lpir-cranelift` (outside this crate) forces Cranelift vmctx param to **`types::I32`** intentionally for RISC-V `enable_multi_ret_implicit_sret` (`emit/mod.rs` comment in milestone doc).

### Downstream coupling

- `lp-glsl-naga`, `lpir-cranelift`, filetests, and JIT `call.rs` assume vmctx is a single **i32** word at the **host** invoke boundary today (`call.rs`, `direct_call.rs` push `as i32`).
- Cranelift already uses **`pointer_type`** for StructReturn, result-pointer stack bases, and `stack_addr`; LPIR still types **vmctx** and **SlotAddr** as **I32** and forces vmctx to I32 in the **Cranelift signature** (`emit/mod.rs`), which is the JIT 64-bit bug.
- `generated_builtin_abi.rs` already pushes `AbiParam::new(pointer_type)` for many builtins — must stay consistent with LPIR import typing and vmctx ordering.

### Pointer-related sites to reconcile (Cranelift JIT)

- `emit/mod.rs`: `signature_for_ir_func` vmctx param; `cranelift_ty_for_vreg` vmctx vs `vreg_wide_addr`.
- `emit/call.rs`: local function StructReturn + import **result-pointer** calls (`stack_addr` + args).
- `emit/memory.rs`: `SlotAddr`, `Load`/`Store` address widening (`widen_to_ptr`).
- `direct_call.rs` / `call.rs`: vmctx passed as `i32`.
- `invoke.rs`: all shims assume scalar args are `i32` (vmctx must become native width on 64-bit).
- `lib.rs` tests: `jit_test_vmctx()` cast to `i32` for `call_i32`.

## Questions (to resolve one at a time)

### Q1 — Plan boundary: LPIR-only vs LPIR + codegen in one plan?

**Context:** JIT notes say the real fix needs `pointer_type` in Cranelift signatures *and* invoke shims. LPIR-only change alone can break `cargo check` if lowering still assumes `I32` for `v0` everywhere.

**Suggested answer:** Single plan with **phases**: (1) LPIR + interp + validate + tests, (2) minimal `lpir-cranelift` wiring so `signature_for_ir_func` uses `pointer_type` for vmctx when lowering for **host JIT ISA**, (3) optional: keep **emulator object** path on `I32` vmctx via flag or dual path until unified — *or* one phase that updates both LPIR and cranelift together to avoid a broken intermediate state.

*Answer:* **Full E2E in this plan.** WASM stays **all i32** at the WASM boundary; **Cranelift** path updates **vmctx** and **any other real pointer passes** (arrays, out/result-pointer paths, slot addresses in LPIR if we widen them, etc.). Emulator guest vmctx allocation fixes can ride along or sit in an explicit phase tied to `rv32-notes.md`.

### Q2 — Text syntax for pointer type?

**Context:** LPIR text format uses `i32` / `f32` in function headers and vreg defs.

**Suggested answer:** Keyword **`ptr`** (short, matches “pointer width is target-defined” without spelling `i64` in IR).

*Answer:* **`ptr`** — use in parse/print and docs.

### Q3 — Should `SlotAddr` / address vregs become `ptr` or stay `i32`?

**Context:** Cranelift `EmitCtx` already has `pointer_type` for stack addresses; LPIR today uses `I32` for `SlotAddr` results and `vreg_wide_addr` maps to pointer in cranelift. User asked to cover **array handling, out params**, not only vmctx.

**Suggested answer:** **Plan for widening** any LPIR vreg that is a **native address** on the JIT target: at minimum **vmctx**; then **SlotAddr** (and chains) so **`Iadd` / `Isub` / immediate forms** on addresses are typed `ptr` in LPIR and validate/interp agree. **Arrays / out**: as lowering gains pointer parameters, they use **`ptr`** in LPIR and `pointer_type` in CLIF on JIT; WASM emitter **lowers `ptr` to i32** for wasm32. Exact order can be phased (vmctx first, then slot addresses, then user-visible array/out if separate milestones).

*Answer:* **Single plan**, **multiple phases** — do vmctx, SlotAddr/address chains, and other pointer surfaces in one plan document, ordered by phase so the tree stays green between phases; not split across separate roadmap plans.

### Q4 — Interp storage for `Pointer`

**Context:** `interp` runs on the host; tests use `i32` scalars today.

**Suggested answer:** Store pointer-sized values as **`usize`** in a small enum variant or side table keyed by vreg; call entry copies host `usize` into vmctx slot. Document that interp is host-semantics only (not a guest 32-bit emulator).

*Answer:* **`i32` is sufficient.** The interpreter does not use host virtual addresses: `slot_mem` is a linear buffer, `SlotAddr`/`Load`/`Store` use **i32 offsets** into that buffer (`interp.rs`). `IrType::Pointer` in the interp is still carried as a **32-bit abstract address** (offset or opaque token for `ImportHandler`), matching wasm32-style tests. **`usize`/`u64` is not required** for interp correctness; it matters for **native JIT invoke**, which is separate.

**Implementation note:** Extend `Value` with a `Ptr(i32)` variant **or** treat `ptr` vregs as `Value::I32` in interp (same bits, typed differently only in validate). Prefer one representation to avoid duplicate paths; document that interp `ptr` ≠ host pointer width.

## Notes

- **2026-04-03:** User confirmed plan scope is **end-to-end** (not LPIR-only). WASM: **minimal change**, keep **i32** for module interface. Cranelift: **vmctx + other pointer passes** (arrays, out/result-pointer, etc.). See updated scope and inventory above.
- **2026-04-03:** LPIR text keyword for pointer type: **`ptr`**.
- **2026-04-03:** **One plan** with **phased implementation** (vmctx → SlotAddr/chains → arrays/out as present); not separate plans for each slice.
- **2026-04-03:** Interp: use **`i32`** for `ptr` values (offsets into `slot_mem` / opaque vmctx for imports); no need for `usize` in the interpreter.
