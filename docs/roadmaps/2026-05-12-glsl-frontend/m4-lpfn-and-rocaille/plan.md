# M4 Plan - Demo Frontend Switch

## Goal

Turn the M3 language slice into a real firmware demo path before filling out the rest of the M4
language surface. The key experiment is proving that firmware can be built with `lps-glsl` selected
as the shader frontend and without linking Naga at all.

## Scope

In scope:

- Add an explicit shader frontend selector at the `lp-shader` compile descriptor boundary.
- Keep Naga available as an explicit reference frontend.
- Make the Naga frontend optional so a no-default firmware build can omit both `lps-frontend` and
  Naga.
- Thread the selected frontend through `lpc-engine` shader compile options.
- Add a firmware Just recipe for the current vertical slice:
  `just demo-esp32c6-host`.
- Use `examples/basic2` for the first demo because it is already covered by the M3 native frontend
  surface.

Out of scope for this slice:

- Rocaille compatibility.
- `for` loops, component assignment, and compound assignment.
- `lpfn_psrdnoise` out/inout lowering.
- Textures in `lps-glsl`.

## Design

- `lp-shader::ShaderFrontend` is the public switch:
  - `Naga`
  - `LpsGlsl`
- `lp-shader` exposes a short default-off `naga` feature.
- `lp-shader --no-default-features` still builds and defaults the frontend enum to `LpsGlsl`.
- `lpc-engine` defaults normal shader nodes to the native frontend when `naga` is absent.
- `fw-esp32` defaults the server build to `lps-glsl`; `naga` opts into the reference path.

## Validation

- Check that the no-default firmware dependency graph contains no `naga` or `lps-frontend`.
- Check the default `fw-esp32` RV32 server build.
- Check the normal `fw-esp32` server RV32 build.
- Re-run focused `lps-glsl` filetests and unit tests.

## Follow-Up

The original M4 language work remains next:

- `for` loops.
- compound assignment.
- component assignment.
- `length` and `tanh`.
- `lpfn_psrdnoise` out/inout ABI.
- Rocaille as a full-example target.
