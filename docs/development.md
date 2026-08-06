# Development guide

How to work on LightPlayer itself. For the product-level quick start, see the
[top-level README](../README.md); for what each directory contains, see the
per-directory READMEs linked below.

## Setup

```bash
scripts/dev-init.sh
```

This checks required tools (Rust 1.90+, `just`, `oxipng`), installs the RISC-V
target if needed, and sets up the pre-commit hook (which runs `just check`).

## Everyday commands

- `just fci` — fix, check, build, and test the whole project. Run before
  submitting a PR. Scoped variants: `just fci-app`, `just fci-glsl`.
- `just check` — lint, schema drift, and firmware-manifest checks (what the
  pre-commit hook runs).
- `just test` — the main test suite. `just test-all` adds the GPU (`wgpu`)
  tests.
- `just ci-prereqs` — build the emulator/builtin artifacts some test suites
  load at runtime. If an oracle or filetest suite fails strangely (for
  example, shaders rendering black), run this first.
- `just studio-dev` — the browser Studio with the built-in simulator.
- `just --list` — everything else.

Note that `just check` alone is lighter than CI: it does not build wasm32 or
feature-gated (server/stories) call sites. `just check test` is closer to CI
parity.

## Deeper workflows

- **Firmware (ESP32 targets, manifests, on-hardware tests)** — see
  [`lp-fw/README.md`](../lp-fw/README.md). Firmware artifacts embed a manifest
  core that CI verifies against each crate's `manifest-core.expected.json`.
  On-hardware harnesses run via the `fwtest-*` recipes (`just --list | grep
  fwtest`).
- **Board manifests** — board profiles live in
  [`lp-core/lpc-hardware/boards/`](../lp-core/lpc-hardware/boards/), edited and
  validated with `lp-cli` (`cargo run -p lp-cli -- hardware --help`). Use
  `just hardware-list` (optionally `--probe`) to identify attached boards.
- **GPIO calibration** — host-driven square-wave calibration that maps board
  silkscreen labels to HAL GPIO addresses; runs through `lp-cli` against a
  firmware build with the `test_gpio_calibrate` harness feature.
- **Schema generation** — JSON Schemas under [`schemas/`](../schemas/) are
  generated from the Rust types: `just schema-gen` to regenerate,
  `just schema-check` to verify drift (part of `just check`).
- **Shader compiler and filetests** — see
  [`lp-shader/README.md`](../lp-shader/README.md). Filetests run with
  `just test-filetests`.
- **Studio story baselines** — Studio UI stories capture PNG baselines that CI
  refreshes automatically; `oxipng` is required locally for baseline work.

## Architecture

[`architecture.md`](architecture.md) is the system overview. Decision records
live in [`adr/`](adr/), known-condition registries in [`debt/`](debt/) and
[`defects/`](defects/), and the glossary in [`glossary.md`](glossary.md).
