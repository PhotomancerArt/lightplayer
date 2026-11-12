# Future Work

## Render-Local Shader Overrides

- **Idea:** Let a playlist pass render-local uniform overrides into child visuals without authored
  graph bindings.
- **Why not now:** The first design should use ordinary produced slot publication for `entry_time`;
  render-local overrides are only needed if that proves too limiting.
- **Useful context:** The active shader can bind its `time` input to `..playlist#entry_time` once the
  resolver supports early-published produced slots.

## Rich Control Vocabulary

- **Idea:** Add standard control messages for `next`, `prev`, `pause`, `resume`, `brightness`, and
  `speed`, plus a generic playlist-level `next` trigger.
- **Why not now:** The fyeah sign needs entry-local restart triggers, and the roadmap intentionally
  keeps generic transport and OSC-style address/args out of this first event slice.
- **Useful context:** Existing notes favor OSC-compatible control messages later while keeping
  `ControlMessage { id, seq }` small now.
