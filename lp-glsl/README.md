# Light Player GLSL

Light Player GLSL (`lp-glsl`) is the shader compiler stack for embedded LightPlayer: **GLSL →
LPIR → Cranelift → RISC-V**, with `no_std` + alloc on device.

## Project Structure

- **Frontend and IR**
    - `lp-glsl-naga/` — GLSL parsing (naga) and lowering to LPIR
    - `lpir/` — LightPlayer IR
    - `lp-glsl-core/`, `lp-glsl-diagnostics/`, `lp-glsl-values/` — shared types and errors for tests/exec

- **Codegen**
    - `lpir-cranelift/` — LPIR → Cranelift → machine code (used by `lp-engine` / firmware)

- **Builtins and IDs**
    - `lp-glsl-builtin-ids/` — Builtin function ID enum (generated)
    - `lp-glsl-builtins/` — Builtin implementations (Q32 / f32)
    - `lp-glsl-builtins-gen-app/` — Generator for builtin boilerplate (`generated_builtin_abi.rs`, etc.)
    - `lp-glsl-builtins-emu-app/` — RISC-V guest for builtin tests
    - `lpfx-impl-macro/` — Macros for LPFX builtin implementations

- **Testing and WASM**
    - `lp-glsl-filetests/` — Cranelift-style GLSL filetests (JIT, RV32, WASM targets)
    - `lp-glsl-filetests-gen-app/` — Generator for repetitive filetests
    - `lp-glsl-filetests-app/` — Filetest runner CLI
    - `lp-glsl-exec/` — Executable trait for filetests
    - `lp-glsl-wasm/` — GLSL → WASM path (host / playground)

- **Browser demo** (workspace root `lp-app/web-demo/`)
    - In-browser GLSL → WASM demo (`just web-demo` from repo root)

## Running Filetests

Run all GLSL filetests:

```bash
./scripts/glsl-filetests.sh
```

See `scripts/glsl-filetests.sh --help` for targets and filters.

### Advanced Usage

```bash
cargo run -p lp-glsl-filetests-app --bin lp-glsl-filetests-app -- test "*add*"
cargo test -p lp-glsl-filetests --test filetests
```

## Development

### Building

```bash
cargo build
```

Firmware (includes on-device compiler):

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server
```

### Testing

```bash
./scripts/lp-build.sh
```

Runs GLSL filetests and a firmware `cargo check` for the ESP32 pipeline.
