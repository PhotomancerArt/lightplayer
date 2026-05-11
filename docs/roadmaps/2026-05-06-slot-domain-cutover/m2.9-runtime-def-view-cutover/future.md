## Resolver-Backed Fixture Mapping

- **Idea:** Read `FixtureDef.mapping` through a generated view and support aggregate enum/record slot access in runtime nodes.
- **Why not now:** Mapping affects resource allocation and cached precomputed pixel entries; it needs a clean resize/rebuild policy.
- **Useful context:** `lp-core/lpc-engine/src/nodes/fixture/fixture_node.rs`, `lp-core/lpc-model/src/nodes/fixture/fixture_def.rs`.

## Shader Source Reload Through Slot Views

- **Idea:** Resolve `ShaderDef.glsl_path` and GLSL source changes through the runtime resolver instead of only during load.
- **Why not now:** Source file IO and reload lifecycle are separate from per-frame slot reads.
- **Useful context:** `CoreProjectLoader::attach_loaded_nodes` still reads GLSL before constructing `ShaderNode`.

## Output Service Slot Views

- **Idea:** Move output service registration and driver options to resolver-backed `OutputDefView` reads.
- **Why not now:** Output flushing currently lives in `RuntimeServices`, outside node tick, and should be reworked deliberately.
- **Useful context:** `lp-core/lpc-engine/src/project_runtime/runtime_services.rs`, `lp-core/lpc-model/src/nodes/output/output_def.rs`.

## Generated Option Payload Views

- **Idea:** Teach generated slot views to expose option payload accessors such as `brightness_some()` for value-bearing option fields.
- **Why not now:** This milestone added compiled `.some` accessor support and a narrow fixture-local helper; general codegen can follow once more option access patterns exist.
- **Useful context:** `SlotAccessorStep::OptionSome`, `FixtureOptionAccessors`.

## Shader Compile Invalidation

- **Idea:** Track compile-option revisions and invalidate/recompile shaders only when relevant config changes.
- **Why not now:** This milestone can cache latest options each tick and keep existing lazy compile behavior.
- **Useful context:** `ShaderNode::ensure_compiled`, generated `ShaderDefView` accessors.
