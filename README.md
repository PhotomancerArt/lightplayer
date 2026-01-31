# LightPlayer

LightPlayer is a tool for controlling visual effects on microcontrollers (and larger computers).

GLSL shaders are used to define the visual effects, which are JIT-compiled to native RISC-V
code on the target device.

## Quick Start

To run the demo project:

```bash
# Clone the repo
git clone https://github.com/Yona-Appletree/lp2025.git
cd lp2025

# Initialize your development environment
scripts/dev-init.sh

# Run the demo
just demo

# Run other examples
just demo -- <example-name>
```

## Development

To get started with development:

1. **Initialize the development environment:**

   ```bash
   scripts/dev-init.sh
   ```

   This will:
   - Check for required tools (Rust, Cargo, rustup, just)
   - Verify Rust version meets minimum requirements (1.90.0+)
   - Install the RISC-V target (`riscv32imac-unknown-none-elf`) if needed
   - Set up git hooks (pre-commit hook runs `just check`)

2. **Required tools:**
   - Rust toolchain (1.90.0 or later) - [Install Rust](https://rustup.rs/)
   - `just` - Task runner: `cargo install just` or via package manager

3. **Common development commands:**
   - `just fci` - Fix, check, build, and test the whole project. Do this before you submit a PR.
   - `just fci-app` - Fix, check, build, and test the application.
   - `just fci-glsl` - Fix, check, build, and test the GLSL compiler.

See `just --list` for all available commands.

## Acknowledgments

LightPlayer would not be possible without the amazing work of these projects:

- **[Cranelift](https://cranelift.dev/)** - Fast, secure compiler
  backend ([homepage](https://cranelift.dev/), [GitHub](https://github.com/bytecodealliance/cranelift)) - [lightplayer's RISC-V 32 fork](https://github.com/Yona-Appletree/lp-cranelift)
- **[glsl-parser](https://git.sr.ht/~hadronized/glsl)** - GLSL
  parser - [lightplayer's fork](https://github.com/Yona-Appletree/glsl-parser) (enhanced to support
  spans)
- **[Lygia](https://github.com/patriciogonzalezvivo/lygia)** - Shader library (source for lpfx
  built-in functions)
- **[DirectXShaderCompiler](https://github.com/microsoft/DirectXShaderCompiler)** - HLSL compiler (
  compiler architecture inspiration)
- **[esp-hal](https://github.com/esp-rs/esp-hal)** - Pure Rust ESP32 bare metal HAL (used for ESP32
  firmware)
- **[GLSL Specification](https://github.com/KhronosGroup/GLSL)** - GLSL language reference
- **[RISC-V Instruction Set Manual](https://github.com/msyksphinz-self/riscv-isadoc)** - RISC-V
  architecture documentation
- **[RISC-V ELF psABI Specification](https://github.com/riscv-non-isa/riscv-elf-psabi-doc)** -
  RISC-V ABI documentation

... and many more not listed. Thank you to everyone in the open source community for your work.
