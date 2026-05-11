# Lightplayer domain notes

## Design

1. Keep the model explicit and simple for the MVP.
2. Explicit > implicit at v1. Add sugar later (e.g. bare-string bindings).
   It's easier to loosen syntax than to take it away.
3. All validation happens at compose time. Never runtime.
4. Visuals are recursive: composed Visuals (`Stack`, `Live`, `Playlist`)
   reference other Visuals by `ArtifactSpec`. Leaf Visuals (`Pattern`,
   `Effect`, `Transition`) take no Visual children. `Show` is a thin
   `Module` wrapper around a root Visual; recursion lives in the Visual
   tree, not in the Module layer.
5. Bindings: ancestor wins. Pattern's default loses to Playlist's override
   loses to Show's override loses to Project's override.
6. Bus channels follow `<type>/<dir>[/<n>]`. Index 0 is implicit (`audio/in`,
   not `audio/in/0`); explicit indices start at 1 (`audio/in/1`).
7. Shader uniforms: structural names unprefixed (`outputSize`, `input`,
   `inputA`, `inputB`); user/engine params prefixed `param_`.
8. Artifacts: single-file (`fluid.pattern.toml`) or directory
   (`fluid.pattern/pattern.toml`). Directory form when assets are needed.

## TODO LPVM

1. Outputs: do `output` declarations need `bind` too, to publish a Visual's
   output as a named bus channel? Yes.
2. Versioning to allow migrations.
3. Auto-route by type for `param_time`. Decide: implicit `bus = "time"` for
   any unbound `param_time`, or always require explicit `bind`?
4. Priority output for Live shows. Visuals currently produce only textures;
   Live needs a non-visual scalar `priority` per candidate. Per-Visual
   priority shader/builtin? Separate property?
5. `ArtifactSpec` resolution rules: relative paths, `lib:/...`, `std:/...`,
   project-root-relative. Document and pick a syntax.
6. Cycle detection on Visual references. Bounded depth check at load.
7. Hot reload semantics: which params are mutable at runtime vs
   rebuild-required (e.g. fluid `resolution` allocates buffers).
8. Where `[bindings]` may appear (only composite nodes? rigs? project?)
   and how merging works when multiple ancestors override the same key.
9. Control / Panel / Control Surface modeling. Rough shape sketched in
   `domain.md` but no concrete artifact form yet.

## TODO Lightplayer

1. Define `project.toml` shape: module references + cross-cutting
   `[bindings]` overrides + deployment metadata.
2. Define `rig.toml` shape: how sources, sinks, layouts, and hardware
   devices are declared and addressed.
3. Power budgeting model. v1: per-fixture / per-output cap. Later:
   project-level power-budget object that fixtures contribute to.

## Future Work

1. Define `Mixer` (N-arity stateless Visual). Currently a hole in the
   Visual taxonomy.
2. Binding transforms (scale, offset, smoothing, curve). Shape TBD —
   flat fields on the bind table, ordered list, or chained Modulator
   nodes. 9. Self-reference syntax for `[bindings]` (e.g. binding a transition's
   progress to the parent Live's internal `transition_progress`). May be
   `self#...` or implicit / structural.
