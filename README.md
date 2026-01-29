# LightPlayer Application

LightPlayer is a framework for building and deploying interactive visual effects on microcontrollers and other embedded systems.

Effects are written as GLSL shaders a custom native compiler is used to allow running them on hardware without OpenGL, including microcontrollers.

## Quick Start

To run the demo project:

1. **Initialize the development environment:**

   ```bash
   scripts/dev-init.sh
   ```

2. **Run the demo:**

   ```bash
   just demo
   ```

   This will start the lp-cli development server with the `basic` example project. To run a different example:

   ```bash
   just demo -- <example-name>
   ```

## Development Setup

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
   - `just build` - Build all packages (host and RISC-V targets)
   - `just check` - Run formatting and linting checks
   - `just test` - Run all tests
   - `just fmt` - Format code
   - `just fix` - Format code and auto-fix clippy issues

See `just --list` for all available commands.
