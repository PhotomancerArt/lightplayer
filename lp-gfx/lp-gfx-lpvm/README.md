# lp-gfx-lpvm

The **guaranteed CPU backend** for [`lp-gfx`](../lp-gfx/README.md)
(`no_std + alloc`): one generic `LpvmGraphics<B: LpvmEngine>` implements
`LpGraphics` over any LPVM engine, replacing the three near-identical
per-target implementations that previously lived in
`lpc-engine/src/gfx/{host,wasm_guest,native_jit}.rs`.

## Target selection

Exactly one concrete engine is compiled per target — cfg-selected, never a
Cargo feature (`target_backend.rs`):

| Target                         | Engine                          | Backend name             |
|--------------------------------|---------------------------------|--------------------------|
| `cfg(target_arch = "riscv32")` | `lpvm_native::NativeJitEngine`  | `lpvm-native::rt_jit`    |
| `cfg(target_arch = "wasm32")`  | `lpvm_wasm::rt_browser`         | `lpvm-wasm::rt_browser`  |
| catchall (host)                | `lpvm_wasm::rt_wasmtime`        | `lpvm-wasm::rt_wasmtime` |

Construct with `LpvmGraphics::new()` (per-target inherent constructor), or
name the type as `TargetLpvmGraphics`. `riscv32` is the on-device JIT — the
product path; it is never optional.

Per the lp-gfx doctrine (see
`docs/adr/2026-07-09-preview-fidelity-tiers.md`), this backend is always
present; optional GPU backends are additional and runtime-selected, never a
silent substitute. `LpvmGraphics` compiles the `Q32` semantics tier only and
rejects an explicit `F32Gpu` request with an error.

## Handle backing

`lp_shader::LpsTextureBuf` / `LpsSamplePointBuf` / `LpsSampleRgba16Buf`
(guest-pointer buffers in the engine's shared memory) are the *private*
backing of the opaque `lp-gfx` handles. Handles free their buffers through a
shared `Arc` on drop and keep the engine memory alive while they exist;
guest pointers never cross the `lp-gfx` API.
