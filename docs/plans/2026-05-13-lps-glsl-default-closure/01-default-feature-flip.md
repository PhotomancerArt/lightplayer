# 01 Default Feature Flip

## Scope

Thread a default-off `naga` feature through the runtime crate stack and make
`lps-glsl` the default shader frontend.

## Implementation

- `lp-shader/lp-shader`
  - Set default features to empty.
  - Add `naga = ["dep:lps-frontend"]`.
  - Use `cfg(feature = "naga")` for Naga-only code.
- `lp-core/lpc-engine`
  - Set default features to `["std"]`.
  - Add `naga = ["lp-shader/naga"]`.
- `lp-app/lpa-server`
  - Set default features to `["std"]`.
  - Add `naga = ["lpc-engine/naga"]`.
- `lp-fw/fw-esp32`
  - Set default features to `["esp32c6", "server"]`.
  - Add `naga = ["lpa-server/naga"]`.
- `lp-fw/fw-emu`
  - Set default features to empty.
  - Add `naga = ["lpa-server/naga"]`.

## Validate

```bash
cargo check -p lpa-server
cargo check -p lpc-engine --no-default-features
cargo check -p lp-shader --no-default-features
cargo tree -p fw-esp32 --target riscv32imac-unknown-none-elf --features esp32c6,server | rg "naga|lps-frontend" || true
```
