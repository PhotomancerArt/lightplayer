# Node Produce/Consume Runtime Notes

## Scope

Refactor the runtime node contract so produced-slot demand and demand-root side effects have separate, explicit entry points.

This plan must fix the `button-sign` radio cycle:

```text
output.input
  -> fixture.input
    -> playlist entry trigger
      -> bus#trigger
        -> radio.output
          -> old full radio tick
            -> radio.input
              -> bus#trigger  // already active
```

The serial symptom this plan must eliminate is:

```text
resolve control radio input: ResolveError { message: "resolve cycle at Bus(ChannelName(\"trigger\"))" }
```

## Current State

- `NodeRuntime` is centered on `tick(&mut TickContext)`.
- `EngineResolveHost::produce(QueryKey::ProducedSlot)` calls `tick_node_once_for_output`, then reads the requested runtime state slot.
- `tick_node_once_for_output` runs the whole node `tick()` at most once per frame through `producers_ticked`.
- `Engine::demand_roots` exists and currently drives output nodes.
- `ProjectLoader` registers output nodes as demand roots through output-specific loader code and a dummy binding to the conventional `in` slot.
- `ControlRadioNode::tick()` currently does both directions:
  - resolve `input`
  - enqueue/dedupe local messages
  - transmit pending messages
  - drain received radio messages
  - publish `output`
- That combined tick is the cycle source. Producing `radio.output` must not resolve `radio.input`.

## Required Fix

The plan is only complete if these are true:

- `ControlRadioNode::produce(output)` is receive-only.
- `ControlRadioNode::produce(output)` does **not** resolve `input`.
- `ControlRadioNode::consume()` is the path that resolves `input` and sends/retries local messages.
- `ControlRadioNode` is registered as a demand root so radio side effects run even when nobody demands `radio.output`.
- `OutputNode` and `ControlRadioNode` share the same demand-root mechanism.
- The `button-sign` project runs without the serial `resolve cycle at Bus(ChannelName("trigger"))` warning.

## User Notes

- `demand root` is an acceptable name.
- Nodes should actively opt into demand-root behavior.
- We have enough nodes now that this is a good time to refactor.
- Simple nodes should be able to ignore the exact production query and run a full evaluation once per frame.
- The system should make “run this node once per frame if needed” easy.
- Output and radio should share a general system, not separate one-off paths.

## Suggested Answers

### Should `tick()` remain the primary trait method?

No. Keep a compatibility helper if useful, but make `produce(slot)` and `consume()` the trait vocabulary.

### Should demand roots keep the dummy `in` binding?

No. Keep the `demand_roots` list, but call `consume()` directly. The demand root implementation resolves its actual inputs.

### What makes this fix the radio cycle?

The failure path currently says `produce: tick failed`, which means produced-slot demand for `radio.output` runs the full radio tick and resolves `radio.input`. After this refactor, produced-slot demand for `radio.output` calls only `ControlRadioNode::produce(output)`, which drains/publishes received events and never reads `bus#trigger`.
