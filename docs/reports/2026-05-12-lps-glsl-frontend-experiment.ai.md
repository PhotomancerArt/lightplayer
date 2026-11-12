# LPS GLSL Frontend Experiment

Date: 2026-05-12

## Summary

The `lps-glsl` vertical slice proved the core idea: replacing the Naga-backed
GLSL frontend with a small LightPlayer-native GLSL frontend can give a large
firmware-size win and a meaningful on-device compile-time win.

For the same `examples/basic` shader (`3922` bytes), the ESP32-C6 demo build
went from:

- firmware app size: `2,681,296` bytes (`85.24%` of app partition)
- shader compile time: `578ms`

to:

- firmware app size: `1,862,848` bytes (`59.22%` of app partition)
- shader compile time: `165ms`

For this vertical slice, that is:

- `818,448` fewer firmware bytes, about `30.5%` smaller than the Naga build
- `413ms` less shader compile latency, about `3.5x` faster
- `26.02` percentage points of app partition headroom recovered

After the parity push, the closer-to-final `lps-glsl` firmware build measured:

- firmware app size: `2,071,568` bytes (`65.85%` of app partition)
- shader compile time: `195ms`

That makes the current comparison against the original Naga-backed build:

- `609,728` fewer firmware bytes, about `22.7%` smaller than the Naga build
- `383ms` less shader compile latency, about `3.0x` faster
- `19.39` percentage points of app partition headroom recovered

The binary-size comparison is now much more representative than the first
vertical slice, but it is still not a claim of general GLSL completeness.
Intentional exclusions remain: bit reinterpret builtins, pack/unpack, broad GPU
stage metadata, and NaN/inf/domain edge behavior outside the current
LightPlayer shader surface.

## Measurement

Both runs used `examples/basic`, so this is an apples-to-apples comparison for
the current rainbow demo shader.

| Build | App size | Partition use | Shader bytes | Compile time |
| --- | ---: | ---: | ---: | ---: |
| Naga frontend | `2,681,296` | `85.24%` | `3922` | `578ms` |
| `lps-glsl` vertical slice | `1,862,848` | `59.22%` | `3922` | `165ms` |
| `lps-glsl` parity closure | `2,071,568` | `65.85%` | `3922` | `195ms` |

The earlier `57ms` result was real but came from `examples/basic2`, a smaller
`1171` byte shader. It was useful as an initial vertical-slice smoke test, but
`examples/basic` is the better comparison point.

## What Worked

- The default firmware build now excludes Naga and shows the expected
  binary-size benefit. The Naga-backed path remains available through the
  explicit `naga` feature.
- The frontend now compiles the current `examples/basic` rainbow shader:
  vector math, palette helpers, `lpfn_fbm`, and `lpfn_psrdnoise` are enough for
  this demo slice.
- The direct fixture sampling bug found during demo testing was fixed:
  fixture-authored normalized points are now scaled to shader pixel-space, and
  direct sampling passes the intended `outputSize`.
- A compile-only filetest gate now covers the rainbow shader for `rv32lpn.q32`
  before flashing the ESP32 demo.

## Gates Added

- `just test-native-rainbow`
  - Runs `lps-glsl/rainbow.glsl` as a compile-only filetest on `rv32lpn.q32`.
  - `just demo-esp32c6-host` depends on this gate.

- `lp-cli` example-project validation test
  - Recursively loads checked-in `examples/**/project.toml` through
    `lpc_engine::ProjectLoader`.
  - Catches stale authored TOML schema before a project is pushed to firmware.

- Direct sampling regressions in `lpc-engine`
  - Assert direct sampling uses requested `outputSize`.
  - Assert fixture direct sampling sends pixel-space sample points, not raw
    normalized fixture coordinates.

## Caveats

This is not full desktop GLSL compatibility. The goal is the LightPlayer shader
surface, not every GPU compiler feature. The native frontend now covers the
practical examples and filetests needed for the current runtime path, with a
small set of intentional exclusions.

Known exclusions:

- bit reinterpret builtins: `floatBitsToInt`, `intBitsToFloat`
- pack/unpack builtins
- broad NaN/inf/domain propagation edge behavior
- shader-stage IO, buffers, and `shared` globals
- `frexp` and `modf`, pending an explicit Q32 semantics decision

## Interpretation

The size result is the main product signal. Recovering about `610KB` of flash
headroom while keeping the current LightPlayer shader surface makes the
on-device compiler path much more comfortable on ESP32-C6.

The compile-time result is also important: `195ms` is still visible, but it is
comfortably in "interactive reload" territory for the current demo. The old
`578ms` path worked, but it felt much closer to a heavy compile step.

## Next Steps

1. Make the default firmware path use `lps-glsl`; keep Naga behind the explicit
   `naga` feature as a reference.
2. Run a short CPU and allocation profile pass on `examples/basic`.
3. Clean up stale diagnostics and milestone labels in frontend errors.
4. Decide whether `frexp` and `modf` are worth adding with Q32 semantics.
