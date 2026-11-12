# Milestone 5: Fluid Compute Example

## Title And Goal

Build the canonical example showing compute-generated emitters driving a fluid
visual into fixture/output.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-serial-compute-shaders-fluid/m5-fluid-compute-example/`

## Scope

In scope:

- Add a project example with:
  - compute shader emitter source;
  - fluid node;
  - fixture;
  - output.
- Exercise bus bindings:
  - compute output -> fluid emitters;
  - fluid output -> fixture visual input;
  - fixture output -> output input.
- Ensure debug/project read can show compute output and fluid state.
- Profile on emulator and ESP32.

Out of scope:

- Final user-facing UI.
- Multiple emitter algorithms.
- Touch/audio live input.

## Key Decisions

- This is the first proof that compute shaders are useful in the domain.
- The example should use realistic embedded defaults, not desktop-only settings.

## Deliverables

- `examples/fluid-basic` or similar.
- Profile report for ESP32 and/or emulator.
- Debug UI evidence that non-visual shader output is inspectable.
- Documentation update under `docs/lp-core` if concepts changed.

## Dependencies

- Milestone 3 compute shader node.
- Milestone 4 fluid node.
- Current project read/debug UI path.

## Execution Strategy

Full plan. This is an integration milestone across source, engine, wire/view,
example, profiling, and docs.

