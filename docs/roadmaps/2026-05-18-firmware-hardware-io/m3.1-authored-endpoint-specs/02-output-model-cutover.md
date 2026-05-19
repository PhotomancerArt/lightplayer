# Phase 2: Output Model Cutover

## Scope Of Phase

In scope:

- Replace `OutputDef.pin` with required `OutputDef.endpoint`.
- Remove output `pin = ...` TOML compatibility.
- Update default templates and project builders.
- Update output model tests and generated slot view expectations.

Out of scope:

- Changing output provider internals.
- Changing fixture/output dataflow products.

## Code Organization Reminders

- Keep `OutputDef` focused on authored source data.
- Tests belong at the bottom of the file.
- Do not add a hidden migration layer for `pin`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/nodes/output/output_def.rs`
- `lp-core/lpc-model/src/lib.rs`
- generated slot-shape exports if required by build output
- `lp-app/lpa-server/src/template.rs`
- `lp-core/lpc-shared/src/project/builder.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`
- tests that serialize/parse output TOML.

Expected changes:

- `OutputDef` stores `endpoint: ValueSlot<HardwareEndpointSpec>`.
- `OutputDef::new(...)` should accept an endpoint spec, not a pin.
- Add a convenience constructor for the default endpoint if useful, but do not
  hide legacy pin support behind it.
- Output TOML examples/templates use:

  ```toml
  endpoint = "ws281x:rmt:D10"
  ```

- Tests should assert `pin = 18` is rejected for `kind = "Output"`.

## Validate

```bash
cargo test -p lpc-model output
cargo check -p lpa-server
```

