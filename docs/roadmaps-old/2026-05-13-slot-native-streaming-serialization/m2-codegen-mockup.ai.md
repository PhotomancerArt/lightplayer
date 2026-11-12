# Milestone 2: Codegen In The Mockup

## Title and goal

Generate slot-native typed readers and writers, then prove them against the
mockup project model.

## Suggested plan location

`docs/roadmaps/2026-05-13-slot-native-streaming-serialization/m2-codegen-mockup/`

## Scope

In scope:

- Extend `SlotRecord` or adjacent macros to generate read/write support.
- Apply generated code to the mockup source model.
- Keep the mockup aligned with the current real model shape.
- Exercise node defs, invocations, values, maps, options, enums, and bindings.
- Compare generated behavior against manual functions from Milestone 1.
- Change the reader/writer API where codegen reveals friction.

Out of scope:

- Production loader/message replacement.
- Removing Serde from real crates.
- Complex nested wrapper enums beyond the one-level forms required by the
  mockup.

## Key decisions

- Codegen should use the same reader/writer semantics as manual functions.
- The mockup is allowed to change to better mirror the validated target model.
- Any mockup-vs-real deviation must be documented as either intentional or a
  real-domain cleanup candidate.

## Deliverables

- Generated reader/writer support for representative slot records/enums.
- Mockup TOML disk-storage tests.
- Mockup JSON wire-storage tests.
- Round-trip and error tests.
- Updated notes on storage metadata that the derive needs.

## Dependencies

- Milestone 1 foundation.
- Current mockup model and existing slot shape generation.

## Execution strategy

Full plan: macro/codegen work has high leverage and can create subtle behavior
drift, so it should be planned and validated in phases against the mockup.
