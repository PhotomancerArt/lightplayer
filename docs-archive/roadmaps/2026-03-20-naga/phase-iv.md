# Phase IV: Cleanup, old frontend removal, validation

## Goal

Remove the old frontend and any migration scaffolding. Final validation across
all targets. Documentation updates.

## Scope

In scope:
- Remove `lps-frontend` crate
- Remove `glsl-parser` fork dependency
- Remove any compatibility shims or dual-path code in filetests
- Update architecture documentation
- Update README
- Final validation: all filetests, ESP32 build, web demo

Out of scope:
- New features

## Deliverables

- Cleaned-up dependency graph (no old frontend)
- Updated documentation
- All targets passing

## Dependencies

- Phase III complete (both backends on Naga)
