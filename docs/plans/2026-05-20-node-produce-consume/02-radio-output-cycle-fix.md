# Phase 2: Radio Output Cycle Fix

## Scope Of Phase

Make `ControlRadioNode` use producer-specific output production and demand-root consumption. This phase must fix the `button-sign` serial cycle.

Out of scope:

- Ack/TTL/retransmit/mesh radio protocol design.
- Broad node migration beyond what is required for radio/output.

## Code Organization Reminders

- Keep radio-specific helpers in `control_radio_node.rs`.
- Split helpers by behavior: config/open, receive output, accept/send input.
- Avoid resolving `input` from any helper used by `produce(output)`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/nodes/radio/control_radio_node.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`
- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/dataflow/resolver/resolve_session.rs`

Required behavior:

- Implement `ControlRadioNode::produce(output)` as receive-only:
  - read config
  - ensure/open radio
  - drain received radio messages
  - publish `output`
  - never call `resolve_input_messages`
  - never resolve `control_radio_input_path()`
- Implement `ControlRadioNode::consume()` for root work:
  - read config
  - ensure/open radio
  - resolve `input`
  - dedupe/enqueue local messages
  - transmit pending messages
  - drain received radio messages
  - publish `output`
- Register `ControlRadioNode` as a demand root in `ProjectLoader`.
- Add a regression test proving resolving `radio.output` while `bus#trigger` is active does not resolve `radio.input` and does not produce `SessionResolveError::Cycle`.
- Remove or bypass any need for the temporary same-node bus-provider skip if possible.

Acceptance criterion:

The following serial warning must be gone:

```text
resolve control radio input: ResolveError { message: "resolve cycle at Bus(ChannelName(\"trigger\"))" }
```

## Validate

```bash
cargo fmt --package lpc-engine
cargo test -p lpc-engine control_radio_bidirectional_bus_binding_broadcasts_button_event
cargo test -p lpc-engine button_sign_example_loads_with_control_radio_node
cargo test -p lpc-engine radio_output_production_does_not_resolve_radio_input
timeout -s INT 10s just demo button-sign
```

If hardware is attached and the branch is ready for device validation, also run the ESP32 demo path that produced the serial warning:

```bash
just demo-esp32c6-host button-sign
```
