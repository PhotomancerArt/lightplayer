# lpv-model

`lpv-model` is currently disabled and no longer part of the workspace build.

This crate contains the older visual authoring model from before the project
definition work consolidated around `lpc-model` slots and SlotCodec. It is kept
on disk as reference material because some ideas may be useful when rebuilding
visual authoring concepts in the slot-native model.

Do not add new dependencies on this crate or fix workspace build failures by
making `lpv-model` compile. New implementation work should happen in
`lpc-model` and the slot-native project/node definition path.

The old `lpc-source` crate has been retired and removed from the active repo.
Some archived `lpv-model` source files may still mention old `lpc_source::*`
types as historical context; those references are not an active dependency and
should not be restored.
