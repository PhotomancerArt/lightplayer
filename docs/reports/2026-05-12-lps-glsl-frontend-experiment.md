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

The binary-size comparison is not a final apples-to-apples product comparison
yet. `lps-glsl` does not have full GLSL/filetest parity with the Naga frontend,
so some of the size delta is missing functionality. Treat the size result as a
promising upper-bound signal from the vertical slice, not as the final expected
parity savings.

## Measurement

Both runs used `examples/basic`, so this is an apples-to-apples comparison for
the current rainbow demo shader.

| Build | App size | Partition use | Shader bytes | Compile time |
| --- | ---: | ---: | ---: | ---: |
| Naga frontend | `2,681,296` | `85.24%` | `3922` | `578ms` |
| `lps-glsl` frontend | `1,862,848` | `59.22%` | `3922` | `165ms` |

The earlier `57ms` result was real but came from `examples/basic2`, a smaller
`1171` byte shader. It was useful as an initial vertical-slice smoke test, but
`examples/basic` is the better comparison point.

## What Worked

- The `server-lps-glsl` firmware build excludes Naga and shows the expected
  binary-size benefit.
- The frontend now compiles the current `examples/basic` rainbow shader:
  vector math, palette helpers, `lpfn_fbm`, and `lpfn_psrdnoise` are enough for
  this demo slice.
- The direct fixture sampling bug found during demo testing was fixed:
  fixture-authored normalized points are now scaled to shader pixel-space, and
  direct sampling passes the intended `outputSize`.
- A compile-only filetest gate now covers the rainbow shader for `rv32lpn.q32`
  before flashing the ESP32 demo.

## Gates Added

- `just test-lps-glsl-rainbow`
  - Runs `lps-glsl/rainbow.glsl` as a compile-only filetest on `rv32lpn.q32`.
  - `just demo-esp32c6-host-lps-glsl` depends on this gate.

- `lp-cli` example-project validation test
  - Recursively loads checked-in `examples/**/project.toml` through
    `lpc_engine::ProjectLoader`.
  - Catches stale authored TOML schema before a project is pushed to firmware.

- Direct sampling regressions in `lpc-engine`
  - Assert direct sampling uses requested `outputSize`.
  - Assert fixture direct sampling sends pixel-space sample points, not raw
    normalized fixture coordinates.

## Caveats

This is not yet full GLSL compatibility. The goal of this experiment was a
vertical slice that can run an existing demo and show the size/latency trade.
The result is strong enough to justify continuing, but compatibility work is
still ahead.

Important remaining language areas:

- broader control-flow coverage, especially `for` loops
- more complete overload/builtin coverage
- arrays and structs beyond the current slice
- better error recovery, not just good first-error diagnostics
- broader filetest compatibility against the existing GLSL corpus

## Interpretation

The size result is the main product signal, with the caveat above. Recovering
about `818KB` of flash headroom at this stage suggests the native frontend can
make the on-device compiler path much more comfortable on ESP32-C6, but the
number will almost certainly move as language coverage grows.

The compile-time result is also important: `165ms` is still visible, but it is
comfortably in "interactive reload" territory for the current demo. The old
`578ms` path worked, but it felt much closer to a heavy compile step.

## Next Steps

1. Expand `lps-glsl` toward the existing filetest language surface while keeping
   the implementation `no_std + alloc`.
2. Keep `rv32lpn.q32` beside the existing Naga targets until compatibility is
   boring.
3. Add focused filetests as each language feature lands, preferring small
   compile/run fixtures over one huge compatibility jump.
4. Continue measuring both firmware size and compile time after major language
   features, because parser/HIR convenience can quietly become binary bloat.
