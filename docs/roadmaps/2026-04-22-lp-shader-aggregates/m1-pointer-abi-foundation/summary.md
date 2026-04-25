### What was built

- LPIR gained explicit `IrFunction::sret_arg: Option<VReg>` and `ImportDecl::sret: bool` markers; printer/parser/validator updated; sret VReg fixed at `vmctx + 1`.
- Frontend (`lps-frontend`) emits a unified pass-by-pointer ABI for aggregates (arrays today; structs in M2): `in` array params lower to `IrType::Pointer` + entry `Memcpy` into a stack slot; aggregate returns use sret with caller-allocated buffer.
- Single layout authority: `lps_shared::layout::std430` is now the only source of truth for aggregate size/align/stride. New `lower_aggregate_layout.rs` funnels Naga types through it.
- New `lps-frontend/src/lower_call.rs` handles call lowering (extracted from `lower_expr.rs`).
- `ArrayInfo` / `ArraySlot` generalised to `AggregateInfo` / `AggregateSlot`.
- All four backends drive sret behaviour off the LPIR marker, no heuristics:
  - `lpvm-cranelift`: `signature_uses_struct_return = func.sret_arg.is_some()`; param order `[vmctx, sret?, user…]`; matching block-param binding and call emission.
  - `lpvm-native` (RV32): `func_abi_rv32` selects `ReturnMethod::Sret` from `sret_arg`; sret pointer in `a0`, vmctx in `a1`, then user args.
  - `lpvm-emu`: new `host_marshal.rs` packs aggregate `in` args into `EmuSharedArena`-backed guest memory; sret buffer allocated in guest RAM and read back; respects `sret_arg` / import `sret`.
  - `lpvm-wasm`: new `aggregate_abi.rs` bumps the exported `$sp` shadow stack and writes/reads aggregate bytes in linear memory; mirrored across `rt_wasmtime` and `rt_browser`.
- `lp-riscv-emu`: `call_function_with_struct_return` now splices the sret pointer at the real `StructReturn` index and runs the full `place_arguments` path (fixes vmctx/sret register layout under the new ABI).
- `lpvm/src/lpvm_abi.rs`: aggregate flatten/decode logic refactored into named helpers (`dense_q32_flatten_array` / `dense_q32_decode_array`) with module docs explaining the dense-Q32 word-stream vs `LpvmDataQ32` / per-runtime pointer-ABI split.
- New filetests: `function/param-array-pointer.glsl`, `function/return-array-sret.glsl`, `function/call-aggregate-roundtrip.glsl`. Existing array-param filetests updated for the new ABI (`param-array.glsl`, `edge-array-size-match.glsl`, `edge-const-out-error.glsl`).
- New round-trip integration test: `lpvm-emu/tests/aggregate_m1_sret_roundtrip.rs`.
- New LPIR test: `lpir/src/tests/sret_roundtrip.rs`.
- `lp-cli/src/commands/shader_debug/collect.rs` updated for the new `func_abi_rv32(fn_sig, Some(func))` signature.

### Decisions for future reference

#### Pass-by-pointer ABI for all aggregates (no scalarization)

- **Decision:** Arrays and structs are always passed/returned by pointer. `in` params copy into a local stack slot on entry; aggregate returns use a caller-allocated sret buffer.
- **Why:** Avoids two divergent representations (scalarized vs memory) and the size-class explosions they cause; matches `LpvmDataQ32`'s std430 layout 1:1 so host marshalling is "just write the bytes."
- **Rejected alternatives:** Scalarize small aggregates into VReg packs (`Particle[4]`, `int[6]`) — got too complex and asymmetric across struct depths; flatten-only ABI in `lpvm_abi.rs` — duplicated work each backend needed to undo.
- **Revisit when:** Profiling shows pass-by-pointer copies dominate hot loops for tiny aggregates and a read-only `in` optimisation (M5) doesn't fully close the gap.

#### Explicit `sret_arg` marker in LPIR (no backend heuristics)

- **Decision:** `IrFunction::sret_arg: Option<VReg>` and `ImportDecl::sret: bool` are the single trigger for sret behaviour. Backends never guess from return-type shape or RV32 ABI rules.
- **Why:** Cranelift's `ArgumentPurpose::StructReturn` heuristic and RV32's "large return" classification disagreed in subtle ways, and the frontend already knows when it's emitting sret. One bit, one truth.
- **Rejected alternatives:** Per-backend size-based heuristics; deriving sret from `return_types.is_empty() && writes-to-pointer-arg` patterns.

#### `lps_shared::layout::std430` as single layout authority

- **Decision:** All aggregate size / align / stride decisions go through `lps_shared::layout::std430`, including frontend slot allocation, host marshalling, and backend buffer sizing.
- **Why:** Three independent layout calculators (frontend, host, backend) had already drifted once. std430 is what `LpvmDataQ32` uses; making it canonical means host data and guest data are byte-identical without translation tables.
- **Revisit when:** A target requires a non-std430 layout (e.g. a real GPU backend with std140 uniforms).

#### `lpvm-emu` host marshalling kept dense Q32 word stream for arrays

- **Decision:** `lpvm-emu`'s `host_marshal.rs` packs aggregate `in` array data as raw `i32` LE words (one per Q32 lane) into `EmuSharedArena`, not via `LpvmDataQ32::from_value`.
- **Why:** Float arrays in Q32 mode must arrive in guest memory as fixed-point Q32 lanes. Going through `LpvmDataQ32::from_value` re-encodes via IEEE float, which would round-trip incorrectly for the emulator's expected lane format. Aggregate `Struct in` is rejected in `host_marshal` until M2 settles a Q32 packing story for nested types.
- **Rejected alternatives:** Always go through `LpvmDataQ32` (broke Q32 fidelity); always pad to std430 in the host (legacy emu paths require dense word stream).
- **Revisit when:** M2 lands struct support and the emu host path is migrated to std430-padded `LpvmDataQ32` end-to-end.
