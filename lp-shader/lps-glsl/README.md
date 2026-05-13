# lps-glsl

`lps-glsl` is LightPlayer's native GLSL frontend. It parses the GLSL used by
LightPlayer shader projects, checks and lowers it into LPIR, and hands that IR
to the runtime compiler backend.

LightPlayer is an embedded shader JIT for LEDs. A shader is loaded from a
project, compiled on the ESP32-C6 at runtime, emitted as native RISC-V machine
code, and then called directly by the renderer. This crate is the first stage of
that runtime compiler pipeline.

```text
GLSL source
  -> lps-glsl
  -> LPIR
  -> lpvm-native
  -> RV32 machine code
```

The crate is `no_std + alloc` so the same frontend can run inside firmware.

## What It Does

`lps-glsl` implements the compiler frontend pieces needed before LPIR codegen:

- source mapping and byte spans
- source-spanned diagnostics
- tokenization
- top-level declaration indexing
- function body parsing
- typed HIR construction
- overload resolution and coercions
- constant folding
- readable/writable place modeling
- LPIR lowering
- budgeted, resumable compilation through `CompileJob`

The one-shot API is `compile`. The resumable API is `CompileJob`, which steps
through the same pipeline under a caller-provided compile budget.

```text
lex -> index -> body/HIR -> lower -> done
```

## Language Scope

This is not intended to be a complete desktop GLSL implementation. It is the
frontend for the LightPlayer shader language, which is GLSL-shaped and tested
through the repository's filetests and checked-in examples.

The intended supported surface includes normal shader code used by LightPlayer:
scalars, vectors, matrices where supported, structs, arrays, functions,
overloads, nested control flow, `inout`/`out` parameters, uniforms, textures,
and builtins used by existing shader tests and examples.

The frontend intentionally does not start with features that are not part of the
product runtime surface:

- GLSL preprocessing
- general GPU pipeline validation
- full desktop GLSL compatibility
- shader-stage metadata that LightPlayer does not consume
- host-only compiler behavior

## Design Background

`lps-glsl` is a purpose-built frontend, but it is not being designed in a
vacuum. The previous LightPlayer GLSL path used `lps-frontend`, which parsed
GLSL through Naga. Naga is a Rust shader translation and validation library used
by many graphics projects. It can parse and validate shader languages and move
them through a general GPU-oriented intermediate representation.

That older path is important prior art for this crate:

- it proved the GLSL-to-LPIR product shape
- it provides a behavior reference while `lps-glsl` reaches parity
- it informs the language edge cases covered by filetests
- it gives a concrete size and compile-time baseline

The reason for writing `lps-glsl` is specialization. Naga is broad, mature GPU
compiler infrastructure. LightPlayer's firmware path needs a smaller runtime
frontend that runs on ESP32-C6, targets only LPIR, preserves authored-source
diagnostics, supports resumable compilation, and avoids pulling unused GPU
compiler machinery into the app image.

The initial vertical slice compared the existing Naga-backed path with
`lps-glsl` on the same ESP32-C6 native backend and the same `examples/basic`
shader.

| Path | App image | App partition | Shader bytes | Compile time |
| --- | ---: | ---: | ---: | ---: |
| Naga-backed frontend | `2,681,296` bytes | `85.24%` | `3922` | `578ms` |
| `lps-glsl` parity closure | `2,071,568` bytes | `65.85%` | `3922` | `195ms` |

This is not a claim of full desktop GLSL compatibility, but it is a much closer
comparison for the current LightPlayer shader surface. The result shows why this
crate is worth building: the frontend shape has a large effect on firmware size
and shader reload latency.

See:

- [`docs/reports/2026-05-12-lps-glsl-frontend-experiment.md`](../../docs/reports/2026-05-12-lps-glsl-frontend-experiment.md)
- [`docs/design/lps-glsl-native-frontend.md`](../../docs/design/lps-glsl-native-frontend.md)

## Filetest Targets

The native frontend has its own filetest target:

```text
rv32lpn.q32: GLSL -> lps-glsl -> LPIR -> lpvm-native
```

The previous Naga-backed native target remains available as a reference:

```text
rv32n.q32: GLSL -> lps-frontend/Naga -> LPIR -> lpvm-native
```

Keeping both targets visible lets the new frontend move toward parity while
still comparing against the older behavior.

## Running Checks

Useful crate checks:

```bash
cargo test -p lps-glsl
cargo clippy -p lps-glsl -- -D warnings
```

Useful filetest checks:

```bash
cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise
cargo run -p lps-filetests-app -- test --target rv32n.q32,rv32lpn.q32 --concise path/to/file.glsl
```

Firmware check for the Naga-free path:

```bash
cargo check -p fw-esp32 \
  --target riscv32imac-unknown-none-elf \
  --profile release-esp32 \
  --features esp32c6,server
```

## Development Notes

- Keep the runtime compiler embedded-capable. Frontend code in the product path
  should remain `no_std + alloc`.
- Keep the default firmware path independent of Naga so size and compile-time
  measurements remain meaningful.
- Prefer small modules named after compiler concepts.
- Reuse `lps_shared::layout` and LPVM data/path concepts for aggregate layout.
- Treat source spans as required infrastructure, even when halt-on-first-error
  diagnostics are enough.
- Add or update filetests when language behavior changes.
