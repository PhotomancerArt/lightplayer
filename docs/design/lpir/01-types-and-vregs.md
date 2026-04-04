# Types and Virtual Registers

## Type system

LPIR has **three** scalar-level register types. `f32` and `i32` are fixed 32-bit; `ptr` is **pointer-sized** in the emitter’s sense (see below).

| Type  | Width / role | Description |
|-------|----------------|-------------|
| `f32` | 4 bytes | IEEE 754 single-precision float |
| `i32` | 4 bytes | 32-bit integer (signedness per op) |
| `ptr` | Target-defined | Opaque byte address: **32 bits** on RV32 / WASM32, **pointer width of the host ISA** on native JIT (e.g. 64-bit on AArch64 / x86-64) |

In the text format and tables, the spelling is `ptr`. In the Rust crate this is `IrType::Pointer`.

### Rules

- Boolean conditions use `i32`: `0` is false, any nonzero value is true (WebAssembly-style). Comparison operations produce `i32` (`0` or `1`). GLSL `bool` is not a distinct LPIR type.
- There is no separate `u32` type: unsigned behavior is selected per operation (for example `ilt_u`).
- There is no `i64` **as a general data type** in the IR. Q32 widening is backend-internal, not an LPIR type. The `ptr` type is not “`i64`”; it is an address whose machine width follows the ABI.
- There are no vector or matrix types in v1; those forms are scalarized during lowering. Vectors and matrices may be added in a future extension.
- **Addresses** that must round-trip across host JIT, stack slots, and GLSL `out` / `inout` / pointer parameters use `ptr`. Pure 32-bit byte offsets and indices remain `i32`. Lowering may form `ptr` results from `slot_addr` and from `iadd` / `isub` chains rooted at a `ptr` (byte offset added in `i32`, result widened in the emitter as needed).
- Signedness is a property of the operation, not of the type (aligned with WebAssembly for integer ops).

### VM context (`v0`)

Every function has an implicit **VM context** virtual register `v0` with type `ptr`. It is not listed in the text `param_list`; the builder and validator inject it. Callees receive it as the first real ABI argument after any struct-return pointer (emitter-defined). Shader-to-shader and many `call` sites pass `v0` as the first **operand** in the IR pool; the text printer omits that leading `v0` so round-trips stay readable (see `05-calls.md`).

## Virtual register semantics

- Virtual registers are named `v0`, `v1`, `v2`, … with indices monotonic within a function.
- A type appears on the first definition of a register: `v3:f32 = fadd v1, v2`.
- Later uses omit the type: `v3`.
- The IR is not SSA: a virtual register may be reassigned; redefinitions must keep the same type.
- `vreg_count` is a property of `IrFunction` and is fixed before emission.
- Indices are dense: valid indices are `0` through `vreg_count - 1` with no gaps.
- **User** function parameters are virtual registers starting at `v1` in the text form (`v0` reserved for VM context). For example, `func @foo(v1:f32, v2:i32)` defines two user parameters; `v0:ptr` is implicit.
- In internal IR pools, `v0` appears explicitly where the ABI requires it (calls, context use).
