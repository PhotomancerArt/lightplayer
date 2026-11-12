# Node Produce/Consume Runtime Design

## Scope

Replace the current tick-first runtime shape with explicit `produce(slot)` and `consume()` paths while preserving the existing `demand_roots` concept.

## File Structure

```text
lp-core/lpc-engine/src/
  node/
    node_runtime.rs        # NodeRuntime contract, ProduceResult, demand-root hooks
    contexts.rs            # shared context first; split Produce/Consume contexts later if needed
  engine/
    engine.rs              # produced-slot dispatch, demand-root consume loop
    project_loader.rs      # demand-root registration for Output and ControlRadio
    test_support.rs        # dummy nodes updated to new contract
  nodes/
    output/output_node.rs
    radio/control_radio_node.rs
    button/button_node.rs
    playlist/playlist_node.rs
    fixture/fixture_node.rs
    shader/*.rs
    texture/texture_node.rs
    fluid/fluid_node.rs
```

## Architecture Summary

`produce(slot)` materializes one produced slot. It may resolve only the consumed inputs needed for that produced slot.

`consume()` runs for demand roots once per frame. It is where graph boundary actors pull graph data into side effects: hardware output, radio send/retry/receive service, and future IO nodes.

`demand_roots` remains the engine term. Output and radio both use it.

## Core Contract

The contract should be close to:

```rust
pub enum ProduceResult {
    Produced,
    Unsupported,
}

pub trait NodeRuntime {
    fn produce(
        &mut self,
        slot: &SlotPath,
        ctx: &mut TickContext<'_>,
    ) -> Result<ProduceResult, NodeError>;

    fn consume(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError> {
        Ok(())
    }

    fn is_demand_root(&self) -> bool {
        false
    }
}
```

Names can change during implementation, but the behavior must not: produced-slot demand must dispatch to `produce(slot)`, not to full-node `tick()`.

## Simple Node Ergonomics

Simple nodes may use a helper equivalent to the old `tick()`:

- Run full evaluation once per frame.
- Publish all runtime state slots that old `tick()` published.
- Let engine-produced-slot dispatch read the requested slot afterward.

This helper must be explicit enough that specialized nodes can avoid it. `ControlRadioNode::produce(output)` must not use the full-evaluation helper.

## Demand Root Loop

`Engine::tick_nodes` should:

1. Clear the frame resolver cache and advance frame/revision.
2. Build one `EngineSession`.
3. Call `consume()` for each registered demand root.
4. Refresh and flush output sinks after root consumption.

The old dummy `in` binding can be removed once roots are called directly.

## Radio Behavior

`ControlRadioNode` must split current `tick()` behavior:

- `produce(output)`:
  - read config
  - ensure/open radio and subscription
  - publish empty output if needed for same-frame cache safety
  - drain received radio messages
  - publish `output`
  - must not resolve `input`

- `consume()`:
  - read config
  - ensure/open radio and subscription
  - resolve `input`
  - dedupe/enqueue local events
  - transmit pending events
  - drain received radio messages
  - publish `output`

This is the architectural fix for the serial cycle.

## Output Behavior

`OutputNode` should become a demand root whose `consume()` body is its current `tick()` body:

- resolve `input`
- render control product
- write output buffer
- let `EngineServices` flush dirty output sinks afterward

## Resolver Workaround

Any temporary resolver-specific workaround for same-node radio bus cycles should be removed or proven unnecessary after `ControlRadioNode::produce(output)` is receive-only. The architecture should fix the cycle without relying on bus re-entry exceptions.
