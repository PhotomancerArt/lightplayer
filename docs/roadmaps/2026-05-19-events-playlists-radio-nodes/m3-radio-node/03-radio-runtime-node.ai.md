# Phase 3: Radio Control Runtime Node

## Scope Of Phase

Implement `ControlRadioNode` and project-loader support. This phase makes authored ControlRadio nodes actually
send, drain, dedupe, and publish graph events.

In scope:

- `lp-core/lpc-engine/src/nodes/radio/control_radio_node.rs`
- project loader attachment and binding registration.
- runtime tests using `VirtualRadioDriver`.
- payload encode/decode helpers for `ControlMessage`.

Out of scope:

- Playlist example polish.
- ESP32 hardware flashing.
- Ack, TTL, ownership, mesh, or rebroadcasting remote messages.

## Code Organization Reminders

- Keep the runtime under `nodes/radio/`.
- Use small private helper types for pending sends and recent rings.
- Put payload codec helpers near `ControlRadioNode` unless a better model-free shared location appears.
- Put tests at the bottom of `control_radio_node.rs`.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Add:

- `lp-core/lpc-engine/src/nodes/radio/mod.rs`
- `lp-core/lpc-engine/src/nodes/radio/control_radio_node.rs`

Update:

- `lp-core/lpc-engine/src/nodes/mod.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`

Runtime behavior:

- Read `ControlRadioDefView` from authored def slots.
- Open radio by endpoint spec using `ctx.radio_service()`.
- Reopen when endpoint or Wi-Fi channel changes.
- Subscribe to `RadioChannelId::new(channel)`.
- Set `state.output` empty and publish it before resolving consumed slot `input`.
- Resolve consumed slot `input`.
- For each new local `ControlMessage`, enqueue a pending repeated send if it is not already pending
  or recently sent.
- Clamp `repeat_count` to a small maximum, suggested max `8`.
- Each tick, send one copy of each pending event and decrement its remaining count.
- Drain the configured channel.
- Decode only `RadioMessageKind::ControlMessage`.
- Drop payloads whose `(id, seq)` are in recent sent or recent received rings.
- Publish accepted local and remote messages in `ControlRadioState::output` for this tick only.
- Clear `output` to an empty map when no new local or remote messages are accepted.
- Publish `state.output` again after updating it.

Avoid an accidental echo loop:

- The node may consume and publish `bus#trigger`, but it must publish an empty `output` snapshot
  before resolving `input` to avoid executing-node re-entry through its own bus provider.
- The node must not rebroadcast drained remote messages.
- The recommended example topology uses only `bus#trigger`.

Tests should cover:

- local input sends `repeat_count` physical radio messages;
- repeated physical sends carry identical decoded `ControlMessage`;
- duplicate remote payloads publish once;
- output is one tick only;
- local self-echo is suppressed;
- resolving `radio.input <- bus#trigger` while `radio.output -> bus#trigger` exists does not re-enter
  the executing radio node;
- project loader attaches `kind = "ControlRadio"` and registers `input`/`output` bindings.

## Validate

```bash
cargo fmt --check
cargo test -p lpc-engine control_radio
cargo test -p lpc-engine project_loader
cargo test -p lpc-shared virtual_radio
cargo check -p lpc-model --no-default-features
cargo check -p lpa-server
```
