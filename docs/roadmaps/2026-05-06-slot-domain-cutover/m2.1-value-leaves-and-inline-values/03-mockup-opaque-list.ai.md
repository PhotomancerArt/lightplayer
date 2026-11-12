# Phase 3: Mockup Opaque List

## Goal

Prove that inline authored lists can stay concise and still sync generically.

The pressure case is `ring_lamp_counts`: it is one logical value, not a map of
independently versioned slot children.

## Work

- Add a mockup `RingLampCounts` value object.
- Store it as `ValueSlot<RingLampCounts>` in the mockup source mapping model.
- Serialize it as concise inline TOML:

  ```toml
  ring_lamp_counts = [1, 8, 12, 16, 24, 32, 40, 48, 60]
  ```

- Give it a `SlotValueShape` whose `LpType` is `List(U32)`.
- Ensure server/client tree-walk evidence shows the slot tree stops at
  `ring_lamp_counts` while value inspection can still see the array payload.

## Validation

- `cargo test -p lpc-slot-mockup -- --nocapture`
