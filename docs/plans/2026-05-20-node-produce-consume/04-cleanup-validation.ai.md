# Phase 4: Cleanup And Validation

## Scope Of Phase

Remove temporary compatibility pieces where practical, delete obsolete resolver cycle scaffolding, and run final validation.

Out of scope:

- New radio reliability protocol work.
- Firmware flashing unless explicitly requested or available as part of validation.

## Code Organization Reminders

- Update comments from tick-first language to produce/consume language.
- Keep `demand_roots` naming.
- Do not leave stale TODOs or commented experiments.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/dataflow/resolver/resolve_session.rs`
- `lp-core/lpc-engine/src/node/node_runtime.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`
- `lp-core/lpc-engine/src/nodes/radio/control_radio_node.rs`
- `lp-core/lpc-engine/src/nodes/output/output_node.rs`

Expected cleanup:

- Remove resolver-specific radio cycle workaround if producer-specific radio output makes it unnecessary.
- If any generalized merge behavior remains, document it as a general resolver rule and keep tests that are not radio-shaped.
- Ensure no produced-slot demand path for `radio.output` calls full-node evaluation.
- Ensure demand-root `consume()` is the only radio path that resolves `input`.
- Ensure output and radio demand roots use the same engine loop.

Final validation:

```bash
cargo fmt --package lpc-engine --package lp-cli --package lpc-shared
cargo test -p lpc-engine
cargo test -p lpc-shared virtual_radio
cargo check -p lp-cli
timeout -s INT 10s just demo button-sign
```

Serial acceptance:

Run the hardware/serial path when available and confirm the log no longer contains:

```text
resolve cycle at Bus(ChannelName("trigger"))
```

The plan is not complete if that serial warning remains.
