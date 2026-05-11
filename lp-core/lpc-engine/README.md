# lpc-engine

The LightPlayer engine runtime for loaded projects.

This crate owns engine-only behavior: project runtime state, node trees,
resolution, bindings, produced-slot access, and the boundary between
shader/runtime values and portable model or wire values.

**Runtime spine:** `engine::Engine` is the core runtime owner for the new
demand-driven path. It owns the `NodeTree`, engine-level `Resolver`, artifact
store, frame state, slot shape registry, runtime buffers, and demand roots.

**Bindings and resolution:** bindings are node-instance data stored on
`node::NodeEntry` and indexed by `node::NodeTree`. Bus names remain useful
runtime vocabulary for labeled channels, but resolved values are cached by the
engine resolver rather than by a bus object.

`resolver::Resolver` owns same-frame query cache state. `ResolveSession` is the
active per-frame/per-demand object that resolves `QueryKey`s through the
active `ResolveHost`, calls that host on cache misses, and carries a
`ResolveTrace`.
`ResolveTrace` combines cycle detection with optional structured trace events so
tests and future diagnostics can explain value provenance.

The first runnable core slice uses test-only dummy shader/fixture/output nodes
from `engine::test_support` to validate demand roots, bus binding selection,
same-frame caching, recursive resolution, cycle detection, and versioned values
without porting concrete legacy runtimes yet.

Unlike `lpc-model`, `lpc-source`, and `lpc-wire`, this crate may depend on
`lps-shared` because it is responsible for converting between `LpsValue` /
`LpsType` and `ModelValue` / `ModelType`.

**Produced values:** demand-driven resolution caches
[`resolver::production::Production`]: a versioned
[`runtime_product::RuntimeProduct`] (`Value` = carried `LpsValueF32`, `Render` =
engine product handle, `Buffer` = runtime-buffer handle). Nodes expose produced
values through their runtime state slot roots.

**Naming:** Prefer plain engine/runtime nouns when the crate already owns the
concept (`Engine`, `ProjectRuntime`, `NodeTree`, `Resolver`).
Use an `Engine*` prefix only when ambiguity with another layer remains high.
Conversion helpers should name both sides of the boundary (for example functions
that mention `model_value` / `ModelType` vs `LpsValueF32` / `LpsType`).
