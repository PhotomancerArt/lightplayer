# Phase 1: Radio Model And Payload

## Scope Of Phase

Add the authored `ControlRadio` node model and the graph-event radio payload kind. This phase should not
add runtime radio behavior yet.

In scope:

- `ControlRadioDef` and `ControlRadioState`.
- `NodeKind::ControlRadio` and `NodeDef::ControlRadio`.
- model re-exports and slot shape/view generation.
- `RadioMessageKind::ControlMessage`.
- tests for TOML parsing, slot directions, defaults, and radio kind round trip.

Out of scope:

- Engine service plumbing.
- Runtime `ControlRadioNode`.
- Examples.
- Ack, TTL, mesh, ownership, or resend protocols beyond the `repeat_count` config field.

## Code Organization Reminders

- Prefer granular files with one main concept per file.
- Keep `radio/control_radio_def.rs` focused on the model shape.
- Put helpers lower in the file when that improves readability.
- Put `#[cfg(test)] mod tests` at the bottom of each Rust file.
- Mark any temporary code with a clear `TODO`, but avoid temporary code if possible.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add:

- `lp-core/lpc-model/src/nodes/radio/mod.rs`
- `lp-core/lpc-model/src/nodes/radio/control_radio_def.rs`

Expected model shape:

- `ControlRadioDef`:
  - `bindings: BindingDefs`
  - `endpoint: ValueSlot<HardwareEndpointSpec>`, default `radio:espnow:0`
  - `channel: ValueSlot<u32>`, default `1`
  - `repeat_count: ValueSlot<u32>`, default `3`
  - optional Wi-Fi/ESP-NOW channel config if a clean existing optional slot shape is available
- `ControlRadioState`:
  - consumed `input: MapSlot<u32, ControlMessage>`
  - produced `output: MapSlot<u32, ControlMessage>`

Use `ControlRadio` as the authored TOML kind string. Use Rust identifiers such as
`ControlRadioDef`, `ControlRadioState`, `NodeKind::ControlRadio`, and `NodeDef::ControlRadio`.
Use `control_radio` for project-loader tree/display type names.

Update:

- `lp-core/lpc-model/src/nodes/mod.rs`
- `lp-core/lpc-model/src/nodes/node_def.rs`
- `lp-core/lpc-model/src/node/kind.rs`
- `lp-core/lpc-model/src/lib.rs`
- `lp-core/lpc-shared/src/hardware/radio_message.rs`

For `RadioMessageKind`, add an explicit `ControlMessage` variant. Preserve existing numeric values
and avoid breaking `ButtonPress` diagnostics. Use a new stable value, for example `2`.

Tests:

- `kind = "ControlRadio"` TOML parses to `NodeDef::ControlRadio`.
- `ControlRadioDef` defaults match the design.
- `ControlRadioState::input` is consumed and `ControlRadioState::output` is produced.
- `RadioMessageKind::ControlMessage` round-trips through `as_u8` / `from_u8`.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-model radio
cargo test -p lpc-shared radio_message
cargo check -p lpc-model --no-default-features
```
