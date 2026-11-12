# Architecture

LightPlayer is written primarily in Rust and is made of high-level **components** each of which is made up of several crates.

The components form a rough dependency graph and are the highest order of application organization.

## Components

- [lp-base](./lp-base/_lp-base.md) - Non-domain specific shared code used throughout the system.
- [lp-core](./lp-core/_lp-core.md) - The LightPlayer domain core. Models, logic, engine. Everything that isn't specific to hardware or environment.
- [lp-cli](./lp-cli/_lp-cli.md) - The CLI, designed to run in the codebase and aid with development.
- [lp-shader](./lp-shader/_lp-shader.md) - The shader engine, the compilers, JIT, utilities, and tests that enable LightPlayer to execute shaders.
- [lp-fw](./lp-fw/_lp-fw.md) - The hardware specific binaries that run the core.
- [lp-app](./lp-app/_lp-app.md) - The frontend application at [lightplayer.app](https://lightplayer.app).
- [lp-cloud](./lp-cloud/_lp-cloud.md) - The backend cloud app that supports the frontend.
- [lp-emu](./lp-emu/_lp-emu.md) - The emulators and utilities that allow running and testing firmware on a host machine.
- [lp-riscv](./lp-riscv/_lp-riscv.md) - Common RISC-V libraries and tools used by the rest of the system.
- [lp-xt](./lp-xt/_lp-xt.md) - Common Xtensa libraries and tools used by the rest of the system. 