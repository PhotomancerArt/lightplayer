# M1 Future Work

## OSC Address Field

- **Idea:** Add OSC-compatible address/path to `ControlMessage`.
- **Why not now:** The first project can route by `bus#trigger`, and the timeline is tight.
- **Useful context:** Keep address syntax compatible with OSC address patterns when this returns.

## OSC Typed Args

- **Idea:** Add typed positional `ControlArg` values inspired by OSC type tags.
- **Why not now:** The first event is a no-args trigger.
- **Useful context:** Start with bool/int/float/string/blob equivalents and map them to `LpValue`.

## Source Identity

- **Idea:** Add semantic/display source identity back to control messages.
- **Why not now:** Strings are not directly shader-ABI-compatible, and the first trigger flow only
  needs `id` plus `seq`.
- **Useful context:** GLSL may eventually see a compact projection such as `u32`, `u8[]`, or no
  source field at all, while host-side tooling can map that projection to a string.

## Bundles And Timetags

- **Idea:** Support grouped/timed messages similar to OSC bundles.
- **Why not now:** The first trigger behavior is immediate and frame-local.
- **Useful context:** Useful later for synced wireless behavior and timeline scheduling.

## Wider Id And Sequence Fields

- **Idea:** Widen `id` and/or `seq` from `u32` to `u64`.
- **Why not now:** The first trigger path can use existing `LpValue::U32` support, which keeps M1
  focused on the message abstraction and bus behavior.
- **Useful context:** Revisit this when bridge semantics need stronger dedupe/ordering guarantees
  across many producers or longer runtimes.

## MIDI And DMX Bridges

- **Idea:** Add bridge nodes/services that translate MIDI/OSC/control messages into DMX or fixture
  control data and vice versa.
- **Why not now:** M1 only introduces the common message envelope.
- **Useful context:** This is part of the long-term control-data broker direction.
