# M4b — Host Backend Swap (Cranelift → Wasmtime) — Design

Roadmap milestone:
[`docs/roadmaps/2026-04-16-lp-shader-textures/m4b-host-backend-swap.md`](../../roadmaps/2026-04-16-lp-shader-textures/m4b-host-backend-swap.md)

Question iteration: [`./00-notes.md`](./00-notes.md).

## Scope of work

Replace `lp-engine`'s host shader backend with the Wasmtime-backed
`WasmLpvmEngine` from `lpvm-wasm`, and switch the entire backend
selection model from Cargo features to `cfg(target_arch = …)` so
each target has exactly one auto-selected `LpvmEngine`.

Three deliverables ride along because they're prerequisites or
direct consequences:

1. **Wasmtime memory pre-reservation.** `WasmtimeLpvmMemory`'s
   bump-then-grow allocator is unsafe for long-lived production
   use (cached `LpvmBuffer.native` invalidated by `Memory::grow`).
   Replace with bump-only over a pre-grown budget set on
   `WasmOptions::host_memory_pages` (default 64 MiB).
2. **`compile_with_config` parity for `WasmLpvmEngine`.** Today the
   trait's default impl runs and silently drops the per-call
   `lpir::CompilerConfig`. Add the override so `q32_options` etc.
   reach the WASM emitter.
3. **Target-arch backend selection** end-to-end. Drop every
   `cranelift` / `native-jit` Cargo feature from `lp-engine`,
   `lp-server`, `fw-emu`, `fw-esp32`. `lp-engine::Graphics` becomes
   a single unqualified type; per-target file in `gfx/` selects the
   `LpvmEngine`.

A small perf report covers cargo-check time, host binary size, and a
multi-shader stress run (the Cranelift JIT-state-leakage flake — one
of the named motivators — should not reproduce).

Out of scope:

- Removing `lpvm-cranelift` from the workspace (still used by
  `lp-cli shader-debug` AOT and, until M4c, by `lpfx-cpu`).
- `gfx/native_jit.rs`'s underlying behaviour. Only the cfg gating
  changes (`feature = "native-jit"` → `target_arch = "riscv32"`)
  and the type renames to `Graphics`.
- `lpfx-cpu` migration (M4c). User confirmed lpfx-cpu will adopt
  the same target-arch selection pattern this plan establishes.
- Wasmtime perf tuning (separate later milestone).
- `BrowserLpvmEngine`'s own `compile_with_config` override —
  wasm32 path lands but its consumers don't yet set non-default
  `CompilerConfig`. Add later if needed.

## File structure

```
lp-shader/
└── lpvm-wasm/
    ├── src/
    │   ├── options.rs                       # UPDATE: add host_memory_pages field on WasmOptions
    │   └── rt_wasmtime/
    │       ├── engine.rs                    # UPDATE: compile_with_config override + pre-grow + deferred-knobs comment
    │       └── shared_runtime.rs            # UPDATE: drop grow loop; OOM past the pre-grown cap
    └── tests/
        └── compile_with_config.rs           # NEW: q32.mul_mode flows through (regression)

lp-core/
├── lp-engine/
│   ├── Cargo.toml                           # UPDATE: target-arch-gated deps; drop cranelift/native-jit features
│   └── src/
│       ├── lib.rs                           # UPDATE: pub use Graphics; drop CraneliftGraphics export
│       ├── nodes/shader/runtime.rs          # UPDATE: drop #[cfg(feature="cranelift")] on test
│       └── gfx/
│           ├── mod.rs                       # UPDATE: target-arch dispatch into Graphics
│           ├── cranelift.rs                 # DELETE
│           ├── host.rs                      # NEW: Graphics over WasmLpvmEngine (catchall)
│           ├── wasm_guest.rs                # NEW: Graphics over BrowserLpvmEngine (wasm32)
│           └── native_jit.rs                # UPDATE: drop feature gate; rename type to Graphics
└── lp-server/
    ├── Cargo.toml                           # UPDATE: drop cranelift / cranelift-optimizer / cranelift-verifier features
    └── src/
        ├── lib.rs                           # UPDATE: pub use lp_engine::Graphics
        └── server.rs                        # UPDATE: doc-comment examples

lp-cli/
├── src/server/create_server.rs              # UPDATE: Graphics::new()
└── tests/integration.rs                     # UPDATE: Graphics::new()

lp-fw/
├── fw-emu/
│   ├── Cargo.toml                           # UPDATE: drop cranelift + native-jit features (always-on for RV32)
│   └── src/main.rs                          # UPDATE: drop all backend cfgs; just Graphics::new()
└── fw-esp32/
    ├── Cargo.toml                           # UPDATE: drop cranelift feature
    └── src/main.rs                          # UPDATE: drop all backend cfgs; just Graphics::new()

docs/
└── design/native/perf-report/
    └── 2026-04-19-m4b-wasmtime-swap.md      # NEW: cargo check time, binary size, multi-shader stress
```

## Conceptual architecture

```
                              lp_engine::Graphics  (single unqualified type)
                                       ▲
        ┌──────────────────────────────┼──────────────────────────────┐
        │                              │                              │
cfg(target_arch="riscv32")     catchall (host)             cfg(target_arch="wasm32")
        ▼                              ▼                              ▼
  gfx/native_jit.rs               gfx/host.rs                  gfx/wasm_guest.rs
  ┌────────────────────┐    ┌──────────────────────┐    ┌───────────────────────┐
  │ Graphics {         │    │ Graphics {           │    │ Graphics {            │
  │   engine:          │    │   engine:            │    │   engine:             │
  │   LpsEngine<       │    │   LpsEngine<         │    │   LpsEngine<          │
  │   NativeJitEngine> │    │   WasmLpvmEngine>    │    │   BrowserLpvmEngine>  │
  │ }                  │    │ }                    │    │ }                     │
  └────────────────────┘    └──────────────────────┘    └───────────────────────┘
        │                              │                              │
        ▼                              ▼                              ▼
  lpvm-native::rt_jit         lpvm-wasm::rt_wasmtime           lpvm-wasm::rt_browser
        │                              │                              │
        └──────────────────────────────┼──────────────────────────────┘
                                       ▼
                            lp_shader::LpsEngine<E>
                            ::compile_px(glsl, fmt, &cfg)
                            ::alloc_texture(w, h, fmt)
                                       │
                                       ▼
                       LpsPxShader / LpsTextureBuf
                       (shader.render_frame writes pixels;
                        consumers see &dyn TextureBuffer)

backend_name() values: "lpvm-native::rt_jit"
                       "lpvm-wasm::rt_wasmtime"
                       "lpvm-wasm::rt_browser"
```

## Key invariants

- **One backend per target.** `cfg(target_arch = …)` is the only
  selector. No backend-selection Cargo features anywhere in
  `lp-engine`, `lp-server`, `fw-emu`, or `fw-esp32`.
- **Single `lp_engine::Graphics` type.** Same constructor on every
  target (`Graphics::new()`). All call sites unconditional.
- **Wasmtime memory is pre-reserved.** `host_memory_pages` budget
  set once at engine construction; never grown after init. Default
  64 MiB (1024 wasm pages); configurable on `WasmOptions`.
- **`compile_with_config` parity.** `CraneliftEngine`,
  `NativeJitEngine`, `WasmLpvmEngine` all honor per-call
  `lpir::CompilerConfig`. `BrowserLpvmEngine` keeps the default
  (delegate to `compile()`); no current consumer needs more.
- **`lpvm-cranelift` stays in the workspace** for `lp-cli
  shader-debug` AOT and (until M4c) `lpfx-cpu`. `lp-engine` no
  longer depends on it.
