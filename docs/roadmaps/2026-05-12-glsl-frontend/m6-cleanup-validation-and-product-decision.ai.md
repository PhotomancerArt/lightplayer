# Milestone 6 - Cleanup, Validation, and Product Decision

## Title and Goal

Decide how `lps-glsl` enters the product, clean up experiment scaffolding, and lock down
validation for the supported example-shaped language.

## Suggested Plan Location

`docs/roadmaps/2026-05-12-glsl-frontend/m6-cleanup-validation-and-product-decision/`

## Scope

In scope:

- Document the supported GLSL subset and known unsupported syntax.
- Keep Naga as the host oracle for differential tests.
- Add or promote selected filetests that mirror the example feature surface.
- Remove temporary experiment-only code paths that are no longer needed.
- Decide whether `lps-glsl` becomes default for embedded examples, remains opt-in, or
  requires another milestone before product use.
- Run final validation across host, filetests subset, emulator, and ESP32 checks relevant to the
  shader pipeline.

Out of scope:

- Full filetest compatibility.
- New language features not needed by examples.
- WGSL implementation.

## Key Decisions

- The product decision is evidence-based: example coverage, latency, memory, scheduling, and
  diagnostic quality.
- Unsupported syntax must fail explicitly on embedded builds.
- Any production switch must preserve on-device GLSL compilation.

## Deliverables

- Final roadmap summary.
- Supported-subset documentation.
- Validation command log.
- Product recommendation and follow-up roadmap items if needed.

## Dependencies

- Milestone 5 measurement and scheduling evidence.

## Execution Strategy

Small plan. This is mostly cleanup and validation, but it needs a written checklist so the product
decision is grounded in evidence.
