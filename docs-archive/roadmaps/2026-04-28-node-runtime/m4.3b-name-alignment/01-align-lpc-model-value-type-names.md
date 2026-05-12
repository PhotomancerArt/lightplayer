# Phase 1 — Align `lpc-model` Value/Type Names

sub-agent: yes
parallel: -

# Scope of phase

Rename the portable structural value/type representations owned by
`lpc-model`:

- `WireValue` -> `ModelValue`
- `WireType` -> `ModelType`
- `WireStructMember` -> `ModelStructMember`

Rename their modules/files accordingly and update call sites across the
workspace.

Out of scope:

- Removing `NodeProps`, `NodeSpecifier`, or other model aliases.
- Renaming source, wire, or view-owned types except where imports/call
  sites need the new `Model*` names.
- Behavior changes to value/type conversion.

# Code organization reminders

- Prefer one concept per file.
- Keep public types and entry points near the top, helper functions lower
  in the file, and tests at the bottom.
- Do not add compatibility aliases for the old `Wire*` names.
- Do not add broad `#[allow(...)]` attributes or suppress warnings.

# Sub-agent reminders

- Do not commit.
- Stay within this phase's naming scope.
- Do not weaken, delete, or ignore tests to make validation pass.
- If a rename reveals a design mismatch rather than mechanical churn,
  stop and report it.
- Report changed files, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md` and `00-design.md` in this directory first.

In `lp-core/lpc-model/src/prop/`:

- Rename `wire_value.rs` to `model_value.rs`.
- Rename `wire_type.rs` to `model_type.rs`.
- Update declarations/re-exports in `prop/mod.rs`.
- Update crate-root re-exports in `lp-core/lpc-model/src/lib.rs`.
- Rename type definitions and all internal references:
  - `WireValue` -> `ModelValue`
  - `WireType` -> `ModelType`
  - `WireStructMember` -> `ModelStructMember`

Update all downstream imports/usages. Known likely areas:

- `lp-core/lpc-source/src/**`
- `lp-core/lpc-wire/src/**`
- `lp-core/lpc-view/src/**`
- `lp-core/lpc-engine/src/wire_bridge/**`
- `lp-core/lpc-engine/src/prop/runtime_prop_access.rs`
- `lp-app/**`
- `lpv-*` crates and tests if they import the old names
- roadmap/design docs only where current text claims the new canonical name

In `lp-core/lpc-engine/src/wire_bridge/`:

- Rename `lps_value_to_wire_value.rs` to
  `lps_value_to_model_value.rs`.
- Rename `wire_type_to_lps_type.rs` to
  `model_type_to_lps_type.rs`.
- Rename functions accordingly:
  - `lps_value_f32_to_wire_value` -> `lps_value_f32_to_model_value`
  - `wire_type_to_lps_type` -> `model_type_to_lps_type`
- Update `mod.rs` exports and all call sites.

Expected result:

- `rg "WireValue|WireType|WireStructMember|wire_value|wire_type|lps_value_f32_to_wire_value|wire_type_to_lps_type" lp-core lp-app lp-visualizer lp-shader` should only show historical roadmap text if anything. Prefer updating active docs too if they are part of the current node runtime roadmap.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-model -p lpc-source -p lpc-wire -p lpc-view -p lpc-engine
cargo test -p lpc-model
```
