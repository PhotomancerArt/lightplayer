# M2 Future Work

## Output Subtypes

- **Idea:** Model concrete output implementations as distinct output node kinds
  or artifact families such as `output/gpio`, `output/e131`, and
  `output/artnet`.
- **Why not now:** M2 only has one real output shape and should not invent a
  subtype system before the source slot roots are real.
- **Useful context:** M2 simplifies current `OutputDef` to a struct.

## Production Source Mutation

- **Idea:** Use the source slot model as the basis for server-side artifact
  mutation through the message API.
- **Why not now:** M2 proves source shape/access/sync only. Production mutation
  needs engine API cleanup and persistence rules.
- **Useful context:** The mockup has `WireSlotMutationRequest` and
  `SlotMirrorView::prepare_set_value` as prior art.

## Real Fixture Example Corpus

- **Idea:** Add richer fixture examples based on real hardware data once keyed
  mapping shape lands.
- **Why not now:** M2 should use `examples/basic` as the canonical fixture and
  avoid expanding example coverage before the model is stable.
- **Useful context:** The user has real examples they want to support soon.
