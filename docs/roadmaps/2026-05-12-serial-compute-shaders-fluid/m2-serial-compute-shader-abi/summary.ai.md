# M2 Summary: Serial Compute Shader ABI

## What Was Built

- Added `lp_shader::CompileComputeDesc`, `ComputeAbi`, `ComputeOutputAbi`, and `LpsComputeShader`.
- Added VMContext private-global read/write access to `LpvmInstance` and all current backends.
- Added compute `tick()` validation: shaders must author `void tick()` with no parameters.
- Added ABI validation for consumed uniforms and produced private globals.
- Added sentinel-array output validation for map-shaped produced slots.
- Updated compute shader header generation so consumed value slots become `layout(binding = N) uniform <ty> <name>;` and produced slots become ordinary private globals.
- Added `lpc_engine::gfx::compute_desc_from_model_def` as the bridge from `ComputeShaderDef` to shader runtime ABI.
- Fixed VMContext global offset metadata so globals after uniforms honor alignment padding.
- Extended frontend lowering so array-of-struct private globals can be written from shader code.
- Kept RGBA point-sampling synthesis scoped to RGBA16 pixel shaders, so R16/RGB16 pixel shaders still compile.

## Decisions For Future Reference

#### Serial CPU Data Program

- **Decision:** Compute shaders are serial LightPlayer CPU/JIT data programs, not GPU compute shaders in miniature.
- **Why:** The immediate use case is small control/data generation on ESP32, and serial `tick()` maps cleanly to the existing on-device GLSL compiler.
- **Rejected alternatives:** Workgroups, dispatch grids, barriers, atomics, and WGSL-style storage bindings.
- **Revisit when:** A workload actually needs parallel GPU or CPU dispatch semantics.

#### Persistent Globals

- **Decision:** Compute shader globals are not reset before each tick.
- **Why:** Plain globals are useful persistent runtime state, while TOML decides which globals are externally visible consumed/produced slots.
- **Rejected alternatives:** Resetting all globals each tick or requiring explicit state annotations in GLSL.
- **Revisit when:** We need lifecycle controls for reset, reload, or per-instance initialization.

#### Authored Slot Direction

- **Decision:** TOML owns consumed/produced direction; GLSL uses ordinary uniforms and private globals.
- **Why:** This keeps the shader language simple and lets the slot model remain the source of truth.
- **Rejected alternatives:** `LP_IN`/`LP_OUT` macros or GLSL `in`/`out` globals.
- **Revisit when:** Generated headers need stronger source-level diagnostics or editor tooling.

#### Sentinel Arrays

- **Decision:** Produced map-shaped slots use fixed private-global arrays with a key sentinel for now.
- **Why:** GLSL does not have maps, but fixed arrays are easy to typecheck and execute on the current VM.
- **Rejected alternatives:** Solving general map merge/materialization in the shader ABI layer.
- **Revisit when:** M3 materializes sentinel arrays into slot map data and defines merge behavior.

## Validation

- `cargo fmt --check`
- `cargo test -p lps-shared`
- `cargo test -p lps-frontend`
- `cargo test -p lpvm`
- `cargo test -p lp-shader`
- `cargo test -p lpc-model`
- `cargo test -p lpc-engine compute_def_header_and_runtime_descriptor_execute -- --nocapture`
- `cargo check -p lpc-engine`
- `cargo check -p lpvm-emu -p lpvm-native -p lpvm-cranelift -p lpvm-wasm`
- `cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server`

## Next Steps

- Build `ComputeShaderNode` on top of this ABI.
- Convert sentinel arrays into slot map data.
- Define receiver-side merge semantics.
- Support non-leaf binding resolution for map-shaped slots.
- Integrate the fluid node with compute-driven emitters.
