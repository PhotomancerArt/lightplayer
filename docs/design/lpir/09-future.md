# Future Extensions

This chapter lists reserved or planned directions for LPIR beyond the current scalar v1 design. Nothing here is required for conformance to v1.

## Relational ops (`any`, `all`)

These are currently decomposed during scalarization (for example to chains of comparisons and `iand` / `ior`). Dedicated `std.math` entries or small core ops could reduce opcode count for common patterns; any addition would be backward-compatible at the IR level.

## Additional `std.math` and import modules

As GLSL coverage grows, more functions can be added under `@std.math::…` (and other module prefixes) without changing the core opcode set. New modules follow the same `import` / `call` mechanism described in `05-calls.md` and `06-import-modules.md`.

## Vector types and ops

A possible additive extension introduces SIMD-oriented types such as `v2f32`, `v4f32`, and `v4i32` and vector ALU ops for backends that expose them (for example WebAssembly `v128`, ESP32-P4 PIE). Lowering would stop scalarizing for those targets when profitable. Scalarization (vectors to scalars) remains the straightforward direction; re-vectorization from arbitrary scalar IR is not a planned requirement, so the intended path is to retain vector form earlier for SIMD-capable backends rather than to recover vectors from fully scalar LPIR.

## 64-bit types

`i64` and `f64` may be added if a target or language subset requires them. They are out of scope for v1.

## Diagnostic / safe mode

A validation pass or interpreter flag could report suspicious situations (division by zero, NaN inputs, out-of-range casts, out-of-bounds memory) without altering observable results. Such a mode would be diagnostic only.

## Cranelift `StructReturn`

For large multi-return ABIs (for example sixteen `f32` values for a `mat4`), the Cranelift emitter may use `StructReturn` or an equivalent pointer-based convention when the native ABI cannot carry all scalars in registers. This is a backend detail; LPIR retains multi-return in the text form.

## LPIR optimizations

Passes such as dead VReg elimination, constant folding, and liveness-based local reuse are optional middle-end improvements. They do not change the language definition and are not required for correctness of lowering or emission.
