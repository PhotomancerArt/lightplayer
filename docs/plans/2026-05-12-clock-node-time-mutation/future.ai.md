# Future Work

## Persist Mutations Back To Artifacts

- **Idea:** Add explicit save/writeback for mutated authored defs, skipping transient fields by default.
- **Why not now:** Requires TOML editing/formatting policy and artifact lifecycle decisions.
- **Useful context:** Clock controls introduce `SlotPersistence::Transient` so future save code can make this distinction.

## Config, Params, Controls, State Taxonomy

- **Idea:** Formalize top-level conventions such as `config`, `params`, `controls`, and `state`.
- **Why not now:** The clock only needs `controls`; broader taxonomy should emerge from more real nodes.
- **Useful context:** `controls` are user-editable but not persisted by default, unlike durable authored config.

## Container Mutation

- **Idea:** Support map/record/option/enum mutation operations, not only value-leaf `SetValue`.
- **Why not now:** Clock controls only need scalar leaves. Container mutation has more versioning and UI edge cases.
- **Useful context:** `lpc-slot-mockup` has pressure tests for mutation conflicts and map-like data.

## Time Source Variants

- **Idea:** Add clocks driven by wall time, beat/tempo, external MIDI/OSC, or test timelines.
- **Why not now:** First slice should prove a single normal `ClockNode` with dataflow binding.
- **Useful context:** `rate` and `scrub_offset_seconds` are enough for shader timing debug.
