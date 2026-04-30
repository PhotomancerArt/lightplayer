# lpl-model

Legacy LightPlayer model types.

This crate contains the old node config/state shapes for texture, shader,
output, and fixture nodes. It remains in `lp-core` for now because the current
engine and wire paths still need these types while the new source/runtime model
is being built.

New generic/shared concepts should go in `lpc-model`, authored source concepts
in `lpc-source`, and wire shapes in `lpc-wire`. Add to `lpl-model` only when
maintaining or bridging the legacy node system.

The goal is to retire this crate once the legacy nodes are ported to the new
core source and engine model.
