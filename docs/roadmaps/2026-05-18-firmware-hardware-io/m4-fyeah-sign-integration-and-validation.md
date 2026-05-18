# Milestone 4: Fyeah Sign Integration And Validation

## Title And Goal

Validate the hardware IO spine against the first wireless button/sign project and clean up temporary
scaffolding.

## Suggested Plan Location

`docs/roadmaps/2026-05-18-firmware-hardware-io/m4-fyeah-sign-integration-and-validation/`

## Scope

In scope:

- Exercise one sign board with LED output and one button board with GPIO input.
- Exercise ESP-NOW button messages between the two boards.
- Confirm output resource errors remain clear with multiple output nodes.
- Update docs and examples for output pin selection, button wiring, and the ESP-NOW event path.
- Remove or rename temporary smoke-test-only scaffolding.
- Run firmware, host, and relevant emulator validation.

Out of scope:

- Full playlist system.
- Final event-driven switching UX or LightPlayer event semantics.
- Production enclosure or physical build instructions beyond basic wiring notes.

## Key Decisions

- This milestone validates infrastructure, not the complete art/show feature set.
- Temporary diagnostics are acceptable during earlier milestones but should be cleaned or documented
  here.
- Validation should include both host behavior and RV32 firmware checks.

## Deliverables

- Updated `docs/use-cases/2025-05-08-fyeah-sign.md` with implemented IO assumptions.
- Wiring/diagnostic notes for LED data pin and button input pin.
- Final cleanup of test-only module boundaries.
- Validation log or summary.

## Dependencies

- Milestones 1-3.
- Two ESP32-C6 boards for the full physical validation path.

## Execution Strategy

Small plan. The integration target is concrete, but final validation will benefit from a short
checklist and clear command list.
