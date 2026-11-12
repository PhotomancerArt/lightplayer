# Stream Static Catalog Export

## Status

Implemented. The direct JSON shape writer now emits generated static
descriptors without first converting the whole static catalog into owned
`SlotShapeEntry` values.

## Smell

The old full-catalog snapshot path materialized the static catalog into owned
`SlotShapeEntry` values before sending shapes to clients.

That preserves client compatibility, but it temporarily allocates the very
static graph we moved out of the registry. On device, client setup should not
need a full owned catalog snapshot.

## Better Shape

Add a streaming static-catalog export path that writes each static descriptor
directly from generated read-only data, then writes dynamic registry entries.

The client can still receive the same descriptor payload shape. The difference
is only server-side allocation behavior.

## Useful Context

- `lp-core/lpc-model/src/slot/slot_shape_registry.rs`
- `lp-core/lpc-wire/src/slot/slot_shape_registry_json.rs`
- `lp-core/lpc-engine/src/engine/project_read_stream.rs`

## Final Cleanup

The owned full-catalog snapshot API has been removed rather than kept behind a
host/default feature. Callers either stream the complete static catalog as JSON
or request bounded pages.
