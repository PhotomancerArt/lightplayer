# Phase 2: Demand Roots For Output And Radio

## Scope Of Phase

Make demand roots explicit runtime opt-ins and migrate `OutputNode` and `ControlRadioNode` onto `consume()`.

Out of scope:

- Radio ack/TTL/retransmit/mesh behavior.
- Removing all compatibility fallback paths.

## Code Organization Reminders

- Keep output-specific sink flushing in `EngineServices`.
- Keep radio hardware behavior inside `control_radio_node.rs`.
- Avoid introducing a separate "radio root" mechanism.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-engine/src/engine/engine.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`
- `lp-core/lpc-engine/src/nodes/output/output_node.rs`
- `lp-core/lpc-engine/src/nodes/radio/control_radio_node.rs`
- `lp-core/lpc-engine/src/engine/output_flush_tests.rs`

Expected changes:

- Add an opt-in hook such as `NodeRuntime::is_demand_root()` or keep loader-owned `add_demand_root` but make it explicit for radio too.
- Remove output's dummy binding to the conventional `in` slot if the new root loop no longer needs it.
- Change `Engine::tick_nodes` to call `consume()` for each demand root.
- Move `OutputNode::tick()` body into `consume()`.
- Add `ControlRadioNode::consume()`:
  - read config
  - ensure radio
  - resolve `input`
  - dedupe/enqueue local messages
  - transmit pending
  - drain remote messages
  - publish `output`
- Register `ControlRadioNode` as a demand root in `ProjectLoader`.

Tests to add or update:

- Output remains a demand root and flushes exactly as before.
- Radio is registered as a demand root when loading a `ControlRadio` project.
- A radio demand root can send/drain without any output node demanding `radio.output`.

## Validate

```bash
cargo fmt --package lpc-engine
cargo test -p lpc-engine output_demand_marks_output_buffer_dirty_same_frame_before_flush
cargo test -p lpc-engine engine_output_sink_flush_writes_expected_rgb_via_memory_provider
cargo test -p lpc-engine control_radio_bidirectional_bus_binding_broadcasts_button_event
cargo test -p lpc-engine button_sign_example_loads_with_control_radio_node
```

