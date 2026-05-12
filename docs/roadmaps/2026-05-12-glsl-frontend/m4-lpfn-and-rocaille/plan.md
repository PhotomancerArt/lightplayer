# M4 Plan - Demo Frontend Switch

## Goal

Turn the M3 language slice into a real firmware demo path before filling out the rest of the M4
language surface. The key experiment is proving that firmware can be built with `lps-glsl` selected
as the shader frontend and without linking Naga at all.

## Scope

In scope:

- Add an explicit shader frontend selector at the `lp-shader` compile descriptor boundary.
- Keep Naga as the default frontend for existing host and firmware builds.
- Make the Naga frontend optional so a no-default firmware build can omit both `lps-frontend` and
  Naga.
- Thread the selected frontend through `lpc-engine` shader compile options.
- Add a firmware feature and Just recipe for the current vertical slice:
  `just demo-esp32c6-host-lps-glsl`.
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
- `lp-shader` default features include `naga-frontend`.
- `lp-shader --no-default-features` still builds and defaults the frontend enum to `LpsGlsl`.
- `lpc-engine` exposes `lps-glsl-default` to select the native frontend for normal shader nodes.
- `fw-esp32` exposes `server-lps-glsl`, which enables the server dependency set and selects
  `lps-glsl` without enabling `naga-frontend`.

## Validation

- Check that the no-default firmware dependency graph contains no `naga` or `lps-frontend`.
- Check the `fw-esp32` no-default `server-lps-glsl` RV32 build.
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
