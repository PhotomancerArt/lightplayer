# 02 Recipes, Docs, And Profiling

## Scope

Rename the happy-path recipes, preserve explicit Naga recipes, update docs with
final measurements, and run a quick profile pass.

## Implementation

- `just demo-esp32c6-host` should build the default `lps-glsl` path.
- Add or keep an explicit Naga recipe such as `demo-esp32c6-host-naga`.
- Use `test-native-rainbow` as the native frontend smoke gate.
- Update the experiment report with:
  - app size `2,071,568/3,145,728 bytes`, `65.85%`
  - compile time `195ms`
  - caveat that feature parity is now much closer, but unsupported categories
    remain intentionally out of scope.

## Profiling

Run a short profiler sweep on `examples/basic`:

```bash
just profile examples/basic --collect alloc --collect cpu --frames 120 --note "lps-glsl default closure"
```

If the exact collector names differ, use `just profile --help` or
`cargo run -p lp-cli -- profile --help` and record the closest alloc and CPU
profiles.

## Validate

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server,naga
```
