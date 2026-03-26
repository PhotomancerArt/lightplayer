# Design: embedded GLSL JIT on `fw-emu` / `fw-esp32`

## Scope of work

- **Product:** GLSL sources from on-flash (or emulated) FS → **compile on device** → **JIT’d code** → run (ESP32-C6 / RISC-V reference, `fw-emu` in CI).
- **Acceptance:** `fw-tests` `scene_render_emu` + `alloc_trace_emu` pass; **`fw-esp32`** **`cargo check`** on **`riscv32imac-unknown-none-elf`** with default **`server`** (compiler in image).
- **Cargo philosophy:** GLSL+JIT are **baseline** for server/engine; **`std`** gates **host-only** pieces (`libstd`, `cranelift-native`, …), not “has a compiler.” Opt-out flags only for stripped / special builds.

## File structure (relevant)

```
lp-glsl/
├── lp-glsl-naga/                 # UPDATE: already no_std; remains front end
├── lpir-cranelift/
│   ├── Cargo.toml                # UPDATE: split `std` vs `glsl`; `lp-glsl-naga` not std-only
│   └── src/
│       ├── lib.rs                # UPDATE: export `jit()` under `glsl`, not `std`
│       ├── compile.rs            # UPDATE: `jit()` cfg + docs
│       ├── jit_module.rs         # existing JIT module
│       └── jit_memory.rs         # embedded allocator path (extend if needed)
lp-core/
├── lp-engine/
│   ├── Cargo.toml                # UPDATE: always enable `glsl` (+ JIT) on `lpir-cranelift` for this crate
│   └── src/nodes/shader/
│       └── runtime.rs            # UPDATE: real `compile_shader` without `std`; stub only for explicit opt-out
└── lp-server/
    └── Cargo.toml                # UPDATE: if needed, forward dep features; default embedded path includes compiler

lp-fw/
├── fw-emu/Cargo.toml             # UPDATE: `lp-server` line ensures engine has compiler (via lp-server → lp-engine)
└── fw-esp32/Cargo.toml           # UPDATE: same; default `server` build includes compiler

lp-fw/fw-tests/
├── src/lib.rs                    # existing shader gate
└── tests/
    ├── scene_render_emu.rs       # acceptance
    └── alloc_trace_emu.rs        # acceptance
```

## Conceptual architecture

```
┌─────────────────────────────────────────────────────────────┐
│  fw-emu / fw-esp32                                           │
│  lp-server (no libstd)                                       │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  lp-engine  ShaderRuntime                                    │
│  load_glsl → compile_shader → JitModule + DirectCall         │
│  (always-on for server builds; opt-out only if minimal)      │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  lpir-cranelift                                              │
│  jit(glsl):  lp-glsl-naga → LPIR → build_jit_module          │
│  `std` branch: cranelift-native, host ISA autodetect         │
│  `!std` branch: explicit RISC-V32 ISA + jit_memory alloc     │
└──────────────────────────┬──────────────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────────────┐
│  Executable machine code in RAM (JIT buffer) → call          │
└─────────────────────────────────────────────────────────────┘
```

## Main components and interactions

1. **`lpir-cranelift`:** Owns **GLSL → IR → machine code**. **`glsl`** (or default dep) pulls **`lp-glsl-naga`**; **`jit()`** runs under **`glsl` + alloc**, without **`std`**. **`std`** adds **host** codegen discovery (`cranelift-native`) and any **`std`-only helpers**.
2. **`lp-engine`:** **`ShaderRuntime`** calls **`lpir_cranelift::jit`** (or thin wrapper). **No** “enable compiler” feature for normal builds; **optional** **`minimal`** / **`no-shader-compile`** only if we need a smaller `lp-engine` for tests/tools.
3. **`lp-server` / firmware:** Depend on **`lp-engine`** with **`default-features = false`** but **dependency feature list must still include `lpir-cranelift`’s `glsl`** (and optimizer/verifier flags as today). No extra “turn compiler on” knob at **`fw-emu`** unless we’re fixing a missing passthrough.
4. **Platform:** ESP32-C6 build uses **`riscv32imac-unknown-none-elf`** (see `justfile`). **`fw-emu`** uses same family for CI parity. If **I-cache flush** or **executable region** rules appear on real silicon, handle in **`jit_memory` / Cranelift JIT finalize** path (phase notes).

## Dependencies

- **`pp-rs` / `lp-glsl-naga` no_std** (prior plan) — prerequisite.
- Roadmap **Stage VI-A** (`stage-vi-a-embedded-readiness.md`) — overlapping goals for **`lpir-cranelift`** embedded profile; this plan closes the loop through **engine + fw-tests**.
