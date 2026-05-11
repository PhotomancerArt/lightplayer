# Phase III: Cranelift backend port + lp-engine integration

## Goal

Write a new Cranelift backend that lowers `naga::Module` → Cranelift IR,
replacing `lps-cranelift`'s dependency on `lps-frontend`. Update
`lp-engine` to use the new frontend. Measure ESP32 ROM impact.

## Scope

In scope:

- New Cranelift codegen walking `naga::Module` (replaces current TypedShader walk)
- Update `lp-engine` to call new frontend
- All cranelift.q32 filetests passing
- ESP32 binary size measurement (before/after)
- Host JIT and emulator execution modes

Out of scope:

- Old frontend removal (Phase IV)
- Optimization work

## Key decisions

- Rewrite `lps-cranelift` in place or create new crate (TBD at phase start)
- If ESP32 ROM delta is unacceptable, evaluate Naga fork to strip unused code

## Deliverables

- Cranelift backend consuming `naga::Module`
- `lp-engine` using `lps-frontend` frontend
- All filetests passing on both targets
- ESP32 ROM size report

## Dependencies

- Phase II complete (WASM path validated, confidence in Naga frontend)
