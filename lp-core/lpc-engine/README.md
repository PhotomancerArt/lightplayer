# lpc-engine

The LightPlayer engine runtime for loaded projects.

This crate owns engine-only behavior: loaded project runtime state, node trees,
resolution, bindings, runtime state slot roots, and the boundary between
shader/runtime values and portable model or wire values.

**Runtime spine:** `engine::Engine` is the core runtime owner for the new
demand-driven path. It owns the `NodeTree`, engine-level `Resolver`, artifact
store, frame state, slot shape registry, runtime buffers, and demand roots.

**Bindings and resolution:** bindings are node-instance data stored on
`node::NodeEntry` and indexed by `node::NodeTree`. Bus names remain useful
runtime vocabulary for labeled channels, but resolved values are cached by the
engine resolver rather than by a bus object.

**Bus scopes:** channels are keyed by `(scope, name)`
(`dataflow::bus::ScopedChannel`). Every project node introduces a named scope
around its children (the root project is the outermost scope) and playlist
entries wrap each owned child in an anonymous scope; the loader resolves
consumed endpoints to the nearest enclosing scope with a writer (else the
root scope) and produced endpoints to the producer's own scope. Every project
node's runtime (`nodes::project::ProjectNode`) mirrors its scope's
`visual.out` as a produced `output`. See
`docs/adr/2026-07-28-scoped-buses.md`.

`resolver::Resolver` owns same-frame query cache state. `ResolveSession` is the
active per-frame/per-demand object that resolves `QueryKey`s through the
active `ResolveHost`, calls that host on cache misses, and carries a
`ResolveTrace`.
`ResolveTrace` combines cycle detection with optional structured trace events so
tests and future diagnostics can explain value provenance.

The first runnable core slice uses test-only dummy shader/fixture/output nodes
from `engine::test_support` to validate demand roots, bus binding selection,
same-frame caching, recursive resolution, cycle detection, and revised values
without depending on concrete node implementations.

Unlike `lpc-model` and `lpc-wire`, this crate may depend on `lps-shared`
because it is responsible for converting between `LpsValue` / `LpsType` and
`LpValue` / `LpType`.

**Produced values:** demand-driven resolution caches
[`resolver::production::Production`]: an `LpValue` plus revision provenance.
Nodes expose produced values through their runtime state slot roots. Shader ABI
values are converted at node/shader boundaries; lazy graph products travel as
`LpValue::Product`.

**Naming:** Prefer plain engine/runtime nouns when the crate already owns the
concept (`Engine`, `NodeTree`, `Resolver`).
Use an `Engine*` prefix only when ambiguity with another layer remains high.
Conversion helpers should name both sides of the boundary (for example functions
that mention `lp_value` / `LpType` vs `LpsValueF32` / `LpsType`).
