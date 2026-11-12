# Phase 4: Example And Fyeah Validation

## Scope Of Phase

Add or update an example that demonstrates the first symmetric button/sign radio behavior.

In scope:

- A project containing button, radio, playlist, shader/fixture/output nodes.
- Bindings that use one shared `bus#trigger` for button, radio, and playlist.
- Host validation with virtual radio where practical.
- ESP32/firmware checks with compiler included.

Out of scope:

- Hardware two-board manual validation unless the executor has boards attached and the user asks for
  it.
- Mesh/TTL/ack protocol.
- New playlist semantics beyond what M2 already defined.

## Code Organization Reminders

- Prefer extending the M2 playlist example if it exists.
- Keep example artifacts small and explicit.
- Avoid duplicating shader assets if an existing example can be reused cleanly.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Expected binding shape:

```toml
# button.toml
kind = "Button"
endpoint = "button:gpio:D9"
id = 1

[bindings.down]
target = "bus#trigger"
```

The same bus is used for local button triggers, radio input, radio event output, and playlist
input.

```toml
# radio.toml
kind = "ControlRadio"
endpoint = "radio:espnow:0"
channel = 1
repeat_count = 3

[bindings.input]
source = "bus#trigger"

[bindings.output]
target = "bus#trigger"
```

Playlist should consume `bus#trigger`, so local and remote events both restart the active visual.

If a host example needs virtual hardware, use `endpoint = "radio:virtual:0"` in a test fixture rather
than changing the product default.

Tests:

- example parses through `lp-cli` example validation;
- virtual-radio integration can inject a remote radio message and observe playlist trigger state if
  M2 exposes a convenient assertion point;
- otherwise keep the example parse/loader validation and rely on `ControlRadioNode` unit tests for radio
  behavior.

## Validate

```bash
cargo fmt --check
cargo test -p lp-cli --test examples_valid
cargo test -p lpc-engine control_radio
cargo test -p lpc-engine --test runtime_spine
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```
