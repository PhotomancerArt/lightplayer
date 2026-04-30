# lpl-runtime

Legacy LightPlayer runtime nodes.

This crate contains the old runtime implementations for texture, shader,
output, and fixture nodes, plus the bridge hooks that let those nodes run under
the current core engine.

It lives in `lp-core` temporarily so the legacy path can stay close to
`lpc-engine` while the new node/source/runtime model lands. New generic engine
behavior belongs in `lpc-engine`; new authored source schema belongs in
`lpc-source`.

The goal is to retire this crate after the legacy nodes are ported to the new
engine spine.
