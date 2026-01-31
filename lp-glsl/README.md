# Light Player GLSL

Light Player GLSL (`lp-glsl`) is a GLSL compiler and runtime for embedded systems, built using a
fork
of Cranelift. It provides a complete GLSL compilation pipeline that targets RISC-V 32-bit embedded
platforms.

## Project Structure

- **`apps/`** - Executable applications
    - `esp32-glsl-jit/` - ESP32 GLSL JIT compiler and runtime
    - `embive-program/` - Demo program for the embive VM
    - `lp-glsl-filetests-app/` - Command-line tool for running GLSL filetests

- **`crates/`** - Core library components
    - `lp-glsl-compiler/` - GLSL compiler and runtime
    - `lp-glsl-filetests/` - Filetest infrastructure for GLSL
    - `lp-glsl-jit-util/` - JIT compilation utilities
    - `lp-riscv-tools/` - RISC-V instruction encoding and utilities

## Running Filetests

Lightplayer includes a comprehensive test suite using Cranelift-style filetests to validate GLSL
compilation and execution.

### Quick Start

Run all GLSL filetests:

```bash
./scripts/glsl-filetests.sh
```

### Test Selection

The filetest runner supports flexible test selection:

```bash
# Run specific test file (searched recursively)
./scripts/glsl-filetests.sh postinc-scalar-int.glsl

# Run tests in a directory
./scripts/glsl-filetests.sh math/*

# Run tests matching a pattern
./scripts/glsl-filetests.sh "*add*"

# Run specific test case by line number
./scripts/glsl-filetests.sh postinc-scalar-int.glsl:10

# Run multiple patterns
./scripts/glsl-filetests.sh math/* operators/postinc*
```

### Test Categories

- **`math/`** - Arithmetic operations (add, subtract, multiply, divide)
- **`operators/`** - Increment/decrement operators and compound assignments
- **`type_errors/`** - Type checking and error handling

### Test File Format

Test files use GLSL syntax with embedded directives:

```glsl
// test run
// target riscv32.q32

float add_float(float a, float b) {
    return a + b;
}

// run: add_float(1.5, 2.5) ~= 4.0
// run: add_float(-1.0, 1.0) ~= 0.0
```

### Advanced Usage

Run tests from the lp-glsl-filetests-app binary directly:

```bash
cargo run -p lp-glsl-filetests-app --bin lp-glsl-filetests-app -- test "*add*"
```

Run tests via cargo (preserves environment variable support):

```bash
cargo test -p lp-glsl-filetests --test filetests
```

## Development

### Building

Build all components:

```bash
cargo build
```

Build for ESP32:

```bash
cargo build --target riscv32imac-unknown-none-elf -p esp32-glsl-jit --release
```

### Testing

Run the full test suite:

```bash
./scripts/lp-build.sh
```

This script runs:

- GLSL filetests
- 32-bit Cranelift filetests
- ESP32 build verification

## Architecture

Lightplayer compiles GLSL to an intermediate representation, then generates optimized RISC-V machine
code using Cranelift. The runtime includes:

- **Fixed-point arithmetic** for embedded systems
- **Vector/matrix operations** optimized for RISC-V SIMD
- **RISC-V emulator** for testing and development
- **JIT compilation** for dynamic code generation

For more details, see the individual crate READMEs and the `plans/` directory.





