# M2.3 Authored Slot Bindings Summary

## What Was Built

- Added semantic binding types to `lpc-model`:
  - `BindingDefs`
  - `BindingDef`
  - `BindingEndpoint`
  - `BusSlotRef`
  - `NodeSlotRef`
- Added parsed endpoint syntax:
  - `bus#visual.out`
  - `..texture#output`
- Added default-empty `bindings` fields to shader, texture, and fixture defs.
- Removed `texture_loc` from shader and fixture defs.
- Updated `examples/basic` to use bus-first shader -> texture bindings and a
  direct texture -> fixture binding.
- Updated the project loader to interpret the new authored binding fields while
  preserving the current runtime behavior for M2.4.
- Updated project builders and CLI project creation to emit the new binding
  shape.
- Trimmed the old `lpc-source/src/prop` public surface by removing the
  conventional default-bind helper and historical compatibility re-export
  modules.
- Preserved `toml_color` and the old `SrcValueSpec` / `SrcShape` stack because
  they still feed useful color parsing and the remaining legacy resolver path.

## Decisions For Future Reference

#### Bindings Live In `lpc-model`

- **Decision:** Durable binding concepts live in `lpc-model`, not `lpc-source`.
- **Why:** Node defs, source sync, and UI tooling need one semantic model.
- **Rejected alternatives:** A new `lpc-source`-local `BindingDef` layer.
- **Revisit when:** Node defs move crates or `lpc-source` shrinks further.

#### Endpoints Are Parsed Values

- **Decision:** Store parsed `BusSlotRef` / `NodeSlotRef` values, not raw strings.
- **Why:** TOML gets friendly syntax while runtime/loading code avoids repeated
  parsing.
- **Rejected alternatives:** Raw endpoint strings in node defs.

#### Bus-First Is Canonical

- **Decision:** `examples/basic` publishes shader output to `bus#visual.out` and
  has texture consume that bus slot.
- **Why:** Library artifacts should be reusable without hard-coded neighbor refs.
- **Rejected alternatives:** Direct shader -> texture node refs as the default
  authoring pattern.

#### Output Sink Registration Remains Transitional

- **Decision:** `FixtureDef.output_loc` remains for now.
- **Why:** Outputs are special IO sink nodes, and moving that relation into the
  binding model belongs with the M2.4 runtime truth pass.
- **Revisit when:** M2.4 updates output sink registration and fixture/output
  runtime behavior.

#### `lpc-source/src/prop` Is Not Fully Gone Yet

- **Decision:** Remove obvious old public surface now, but keep `SrcValueSpec`,
  `SrcShape`, `SrcSlot`, and `SrcBinding` until the old resolver cascade is
  retired.
- **Why:** The remaining engine resolver tests and source value/color parsing
  still depend on that stack.
- **Rejected alternatives:** Delete the whole prop module in M2.3 and absorb a
  resolver rewrite.
- **Revisit when:** Runtime consumed-slot resolution no longer uses
  `NodeInvocation.overrides` / `SrcBinding`.
