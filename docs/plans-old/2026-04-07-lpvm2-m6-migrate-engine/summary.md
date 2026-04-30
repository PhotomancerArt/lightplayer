# M6: Migrate Engine — Summary

This milestone decouples `lp-engine` from `lpvm-cranelift` using a backend abstraction layer.

## Goal

Introduce `LpGraphics` and `LpShader` traits to allow `lp-engine` to work with multiple shader backends (Cranelift, WASM, future GPU) without hard coupling.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│ Firmware (fw-esp32, fw-emu, fw-wasm)                              │
│                                                                 │
│  ┌─────────────────┐    ┌─────────────────────────────────────┐   │
│  │ CraneliftGraphics │    │ Future: WasmGraphics (M8)           │   │
│  │ (RISC-V JIT)    │    │ Future: GpuGraphics (M9)              │   │
│  └────────┬────────┘    └─────────────────────────────────────┘   │
│           │                                                       │
│           ▼                                                       │
│  ┌─────────────────┐                                              │
│  │ Rc<dyn LpGraphics> │ ──► injected into LpServer                  │
│  └─────────────────┘                                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│ lp-server                                                       │
│  │                                                              │
│  └──► ProjectRuntime ──► ShaderRuntime ──► Box<dyn LpShader>   │
│                                                      │           │
└──────────────────────────────────────────────────────┼───────────┘
                                                       │
                                                       ▼
┌─────────────────────────────────────────────────────────────────┐
│ lp-engine/gfx/                                                  │
│                                                                 │
│  LpGraphics trait: compile_shader() ──► Box<dyn LpShader>      │
│                                                                 │
│  LpShader trait: render(texture, time) ──► backend-specific    │
│                                                                 │
│  Concrete impls:                                                │
│    - cranelift.rs: CraneliftGraphics / CraneliftShader         │
│    - future: wasm.rs, gpu.rs                                     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## File Structure

```
lp-core/lp-engine/src/
├── gfx/
│   ├── mod.rs              # Module doc, re-exports
│   ├── lp_gfx.rs           # LpGraphics trait
│   ├── lp_shader.rs        # LpShader trait, ShaderCompileOptions
│   └── cranelift.rs        # CraneliftGraphics implementation (feature-gated)
├── nodes/shader/runtime.rs # Uses LpShader, removed JitModule/DirectCall deps
├── project/runtime.rs      # Holds Rc<dyn LpGraphics>, passes to ShaderRuntime
└── lib.rs                  # Re-exports gfx module

lp-core/lp-server/src/
└── lib.rs                  # Holds Rc<dyn LpGraphics>, passes to ProjectRuntime

lp-fw/fw-esp32/src/main.rs  # Creates CraneliftGraphics, passes to LpServer::new()
lp-fw/fw-emu/src/main.rs   # Creates CraneliftGraphics, passes to LpServer::new()
```

## Design Decisions

1. **Separate trait files**: `lp_gfx.rs` and `lp_shader.rs` keep mod.rs clean
2. **Per-backend modules**: `cranelift.rs` is the first concrete implementation
3. **Rc<dyn LpGraphics>**: Injected at firmware level, passed through to ShaderRuntime
4. **Box<dyn LpShader>**: One per shader node, contains backend-specific compiled artifact
5. **Pixel loop in LpShader**: `CraneliftShader::render()` contains the DirectCall pixel loop, avoiding per-pixel dynamic dispatch
6. **Feature-gated backends**: `lpvm-cranelift` becomes optional dependency

## Phases

| Phase | Description                          | Risk                              |
| ----- | ------------------------------------ | --------------------------------- |
| 1     | Define LpGraphics + LpShader traits  | Low (additive)                    |
| 2     | Implement CraneliftGraphics          | Low (new code)                    |
| 3     | Migrate ShaderRuntime to LpShader    | **High** (changes rendering core) |
| 4     | Wire graphics through ProjectRuntime | Medium (constructor changes)      |
| 5     | Wire graphics through LpServer       | Medium (constructor changes)      |
| 6     | Update firmware crates               | Medium (main.rs changes)          |
| 7     | Make lpvm-cranelift optional         | Low (Cargo.toml only)             |
| 8     | Cleanup, validation, plan closure    | Low                               |

## Key Trade-offs

- **Single dyn per frame**: `LpGraphics` uses `dyn`, but `LpShader::render()` is monomorphized for the specific backend — no per-pixel overhead
- **No generics on LpServer**: `Rc<dyn LpGraphics>` keeps LpServer clean, no type parameter pollution
- **Constructor injection**: Graphics backend chosen at firmware startup, not compile-time (except via features)

## Dependencies

- `lp-engine` optionally depends on `lpvm-cranelift`
- `lp-engine` re-exports `gfx` module for firmware use
- `lp-server` depends on `lp-engine` for `LpGraphics` trait

## Validation

After all phases:

```bash
# File tests across all targets
./scripts/filetests.sh --target rv32.q32c
./scripts/filetests.sh --target wasm.q32

# Firmware tests
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu

# ESP32 build
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server

# Host tests
cargo test -p lp-engine --features test-cranelift
cargo test -p lpa-server --lib
```
