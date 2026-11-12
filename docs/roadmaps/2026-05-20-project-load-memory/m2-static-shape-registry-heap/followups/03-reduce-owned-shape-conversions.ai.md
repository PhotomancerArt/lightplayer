# Reduce Owned Shape Conversions

## Smell

Some runtime paths still convert `SlotShapeView` / static descriptors back into
owned `SlotShape` values with `to_owned_shape()`.

Several of these are acceptable boundary conversions today, but they are easy
places for hidden allocation to creep back into project load, mutation, or sync.

## Better Shape

Move APIs that only need traversal or default construction to borrowed
`SlotShapeView`-style inputs. Keep owned conversion only at explicit wire or
debug boundaries where an owned payload is required.

## Known Hotspots

- mutation default insertion in `slot_mutation.rs`
- dynamic reader default construction
- slot factory ref resolution
- wire snapshot serialization

## Useful Context

- `lp-core/lpc-model/src/slot/slot_mutation.rs`
- `lp-core/lpc-model/src/slot/slot_factory.rs`
- `lp-core/lpc-model/src/slot_codec/dynamic_slot_reader.rs`
- `lp-core/lpc-wire/src/slot/access_sync.rs`
