# lpc-engine

The LightPlayer engine runtime for loaded projects.

This crate owns engine-only behavior: project runtime state, node trees,
resolution, bindings, runtime property access, and the boundary between
shader/runtime values and portable model or wire values.

**Runtime spine:** `engine::Engine` is the core runtime owner for the new
demand-driven path. It owns the `NodeTree`, `BindingRegistry`, engine-level
`Resolver`, artifact manager, frame state, and demand roots. Legacy visual
runtimes remain under `nodes` (`LegacyNodeRuntime`) and `legacy` beside this
spine until the cutover milestones.

**Bindings and resolution:** `binding::BindingRegistry` owns binding identity,
metadata, and discovery indexes. Bus names remain useful runtime vocabulary for
labeled channels, but resolved values are cached by the engine resolver rather
than by a bus object or `tree::NodeEntry`.

`resolver::Resolver` owns same-frame query cache state. `ResolveSession` is the
active per-frame/per-demand object that resolves `QueryKey`s through the
registry, calls a `ResolveHost` on cache misses, and carries a `ResolveTrace`.
`ResolveTrace` combines cycle detection with optional structured trace events so
tests and future diagnostics can explain value provenance.

The first runnable core slice uses test-only dummy shader/fixture/output nodes
from `engine::test_support` to validate demand roots, bus binding selection,
same-frame caching, recursive resolution, cycle detection, and versioned values
without porting concrete legacy runtimes yet.

Unlike `lpc-model`, `lpc-source`, and `lpc-wire`, this crate may depend on
`lps-shared` because it is responsible for converting between `LpsValue` /
`LpsType` and `ModelValue` / `ModelType`.

**Naming:** Prefer plain engine/runtime nouns when the crate already owns the
concept (`Engine`, `ProjectRuntime`, `NodeTree`, `BindingRegistry`, `Resolver`).
Use an `Engine*` prefix only when ambiguity with another layer remains high.
Conversion helpers should name both sides of the boundary (for example functions
that mention `model_value` / `ModelType` vs `LpsValueF32` / `LpsType`).
