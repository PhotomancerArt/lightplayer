# Phase 3: Clock Node And Time Bus

## Scope Of Phase

Add the actual clock node and wire examples so shader time is normal dataflow.

In scope:

- Add `ClockDef`, `ClockControls`, `ClockState`.
- Add `ClockNode`.
- Add `Clock` to `NodeKind` and `NodeDef`.
- Register clock static shapes.
- Load and attach clock nodes.
- Add binding-priority helpers for default/fallback vs authored bindings.
- Register clock default produced-slot target binding only when there is no explicit binding.
- Bind examples to `bus#time.seconds`.

Out of scope:

- Debug UI mutation controls.
- Mutation request handling.
- Persisting controls to TOML.
- Treating time specially in shader/compute code.

## Code Organization Reminders

- Use `nodes/clock/clock_def.rs`, `clock_controls.rs`, and `clock_state.rs`.
- Use `lpc-engine/src/nodes/clock/clock_node.rs`.
- Keep `mod.rs` files as declarations/re-exports.
- Keep tests at the bottom of the files they exercise.

## Sub-Agent Reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation Details

Relevant files:

- `lp-core/lpc-model/src/node/kind.rs`
- `lp-core/lpc-model/src/nodes/mod.rs`
- `lp-core/lpc-model/src/nodes/node_def.rs`
- `lp-core/lpc-model/src/nodes/clock/*`
- `lp-core/lpc-engine/src/nodes/mod.rs`
- `lp-core/lpc-engine/src/nodes/clock/*`
- `lp-core/lpc-engine/src/dataflow/binding/binding_entry.rs`
- `lp-core/lpc-engine/src/engine/project_loader.rs`
- `lp-core/lpc-model/src/slot_shapes.rs` or generated static-shape registration files, depending on current build output.

Suggested model:

```rust
pub struct ClockDef {
    pub bindings: BindingDefs,
    pub controls: ClockControls,
}

pub struct ClockControls {
    pub running: BoolSlot,
    pub rate: ValueSlot<f32>,
    pub scrub_offset_seconds: ValueSlot<f32>,
}

pub struct ClockState {
    pub seconds: ValueSlot<f32>,
    pub delta_seconds: ValueSlot<f32>,
}
```

Shape metadata:

- `controls` or each control field should be writable + transient.
- `rate` should have number/slider metadata. Suggested visible range: `0.0..4.0`, step `0.05`.
- `scrub_offset_seconds` should have slider metadata. Suggested range: `-10.0..10.0`, step `0.01` or `0.05`.

Runtime behavior:

- Clock node stores accumulated seconds internally.
- If `controls.running == true`, accumulated seconds advances by `delta_seconds * rate`.
- If `controls.running == false`, accumulated seconds does not advance.
- Produced `seconds = accumulated_seconds + scrub_offset_seconds`.
- Produced `delta_seconds` is the last engine delta in seconds when running, else `0.0`.

Project-loader behavior:

- Attach `ClockNode`.
- Register authored target bindings for `seconds` and `delta_seconds` when authored.
- Register default fallback `seconds -> bus#time.seconds` only when `bindings.seconds` is absent.
- Do not serialize this default fallback into `ClockDef.bindings`.
- Add `BindingPriority::authored()` and `BindingPriority::default_fallback()` helpers if useful. Existing authored priority `0` can remain authored-compatible; fallback must sort lower.
- The inline clock in examples should be minimal:

```toml
[nodes.clock]
kind = "clock"
```

Example migration:

- Add inline clock node to `examples/basic/project.toml` and `examples/fluid/project.toml`.
- Add `[bindings.time] source = "bus#time.seconds"` to shader/compute artifacts that use time.
- Rewrite `examples/fluid/compute.glsl` to use `time` directly.
- Tune fluid emitters down:
  - lower intensity,
  - smaller or fewer emitters,
  - smoother position paths,
  - possibly slightly higher fade to prevent saturation.

Tests:

- Clock node produces increasing seconds when running.
- Clock node freezes seconds when not running except scrub offset changes.
- Clock node rate affects seconds.
- Project loader attaches inline clock and bus binding.
- Explicit `bindings.seconds` overrides the clock default fallback binding.
- Compute shader receives time through bus binding in fluid-style test.

## Validate

```bash
cargo fmt
cargo test -p lpc-model clock
cargo test -p lpc-engine clock
cargo test -p lpc-engine project_loader
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```
