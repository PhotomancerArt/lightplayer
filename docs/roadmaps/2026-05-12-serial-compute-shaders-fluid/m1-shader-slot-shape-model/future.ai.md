## Receiver Merge Policies

- **Idea:** Consumed slots should own merge policy such as `by_key`, `latest`,
  and `error`.
- **Why not now:** M1 only models shader slot shapes; resolver execution and
  conflict handling belong to a later dataflow milestone.
- **Useful context:** Produced slots own shader ABI mapping, not merge behavior.

## Aggregate Bindings

- **Idea:** Bindings must be able to target aggregate slots such as a whole
  `MapSlot<u32, FluidEmitter>`, not only value leaves.
- **Why not now:** Current resolver work is separate from compute shader shape
  modeling.
- **Useful context:** Fluid emitters need map-level binding and later merging.

## Resolver Explain For Merges

- **Idea:** Slot probes should explain merge policy, contributors, and conflicts.
- **Why not now:** Explain/probe machinery is not part of M1.
- **Useful context:** This will matter once multiple emitter producers feed one
  fluid node or bus slot.

## Runtime Shader ABI Conversion

- **Idea:** Shader nodes should convert shader-visible ABI storage such as a
  sentinel array into semantic slot data such as `MapSlot<u32, FluidEmitter>`.
- **Why not now:** M1 only records mapping in the model and proves header
  generation.
- **Useful context:** `mapping = { kind = "sentinel", len = 4, key = "id",
  empty_key = 0 }` is the first authored strategy.

## Native Shape Bootstrap

- **Idea:** Static shape bootstrap should eventually include native value shapes
  such as `lp::fluid::Emitter`, not only `SlotRecord` roots discovered by
  codegen.
- **Why not now:** M1 can explicitly register native value shapes where needed.
- **Useful context:** `SlotShapeRegistry` now supports lookup by root name.

## LpValue Naming

- **Idea:** Rename `LpValue` to `LpsValue` or another system-level value name.
- **Why not now:** The rename is broad and orthogonal to compute shader slot
  shape modeling.
- **Useful context:** The current work sharpened the distinction between slot
  data and opaque value data.

