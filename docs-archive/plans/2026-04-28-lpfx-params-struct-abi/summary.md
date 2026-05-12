### What was built

- Added D15 decision entry to `docs/roadmaps/2026-04-23-lp-render-mvp/decisions.md` documenting the structured `params` uniform ABI.
- Updated `docs/roadmaps/2026-04-23-lp-render-mvp/overview.md` "Texture resources" section to describe `params.*` pattern with dotted texture binding specs.
- Updated M1, M4, M5 milestone docs to reflect `params` struct ABI for shader parameters.
- Updated M3, M6, M7 milestone docs for editor and validation context.
- Migrated 4 canonical example shaders from flat `param_*` uniforms to `params` struct:
  - `rainbow.pattern.toml`, `fbm/main.glsl`, `tint.effect.toml`, `kaleidoscope.effect.toml`
- Added clarifying comments to `lp-domain/src/kind.rs` distinguishing authoring/storage recipes from shader-visible runtime forms.
- Added clarifying comment to `lp-domain/src/visual/effect.rs` noting `inputColor` naming is stale/pending M4 design.

### Decisions for future reference

#### Shader params use one `params` struct, not flat top-level uniforms

- **Decision:** Authored shader parameters materialize as a single `params` struct uniform. Scalar params become fields like `params.speed`. Texture-valued params (palette, gradient) become fields like `params.gradient` with dotted texture binding spec keys.
- **Why:** `ParamsTable` is already an implicit `Shape::Struct`; the shader ABI mirrors the domain model. One struct reduces top-level uniform sprawl.
- **Rejected alternatives:** Flat top-level param uniforms like `param_speed` (too much sprawl); separate palette/gradient top-level samplers (unnecessarily separates texture-valued params from scalar params).
- **Revisit when:** If we discover strong need for per-param binding specs at top level, or if nested texture field performance becomes problematic.

#### Graph-fed texture inputs remain outside authored `params`

- **Decision:** Textures supplied by graph composition (Effect upstream, bus textures) remain as top-level resource uniforms, not inside `params`.
- **Why:** Keeps the parameter surface (`params`) meaningfully "what the artist can edit" vs "what the graph wires up."
- **Rejected alternatives:** Putting everything in `params` would muddy the semantic distinction; it would also require nested texture binding specs for composition inputs that have different lifetime semantics than authored params.
- **Revisit when:** M4 Stack/Effect design settles the final naming convention (currently `inputColor` is likely stale; candidates: `input`, `inputImage`, `inputTex`).

#### Palette/gradient as texture fields inside `params`

- **Decision:** With texture-in-struct support merged, palette and gradient params live inside `params` as `sampler2D` fields (e.g., `params.gradient`) rather than as separate top-level samplers.
- **Why:** Keeps all authored params in one place; dotted texture paths like `params.gradient` are now supported by `lp-shader`; keeps authoring recipes separate from runtime texture resources.
- **Rejected alternatives:** Top-level palette/gradient samplers (unnecessary top-level sprawl); passing palette/gradient as fixed-size uniform structs (doesn't match the actual shader sampling pattern).
- **Revisit when:** If we add arrays of textures support and want `params.gradients[0]` style access; that would require additional lp-shader work.
