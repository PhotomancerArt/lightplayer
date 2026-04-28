### What was built

- Added recursive `Texture2D` discovery in `lps-shared/src/texture_binding_validate.rs` using dotted paths (e.g., `params.gradient`)
- Added validation that rejects texture fields inside uniform arrays with clear error messages
- Refactored `LpsPxShader::apply_uniforms` in `lp-shader/src/px_shader.rs` to recursively apply nested uniform structs with dotted path keys
- Added test-only `RecordingUniformBackend` and `px_shader_from_parts_for_test` helper for `render_frame` testing
- Updated `lps-frontend/src/lower_texture.rs` to resolve nested texture operands through access chains (struct fields only)
- Modified `lps-frontend/src/lower.rs` to preserve single-struct uniform names (removed field-hoisting behavior)
- Added dotted path validation in `lps-filetests/src/parse/parse_texture.rs` using `parse_path` and `LpsPathSeg`
- Added parse-error filetests for invalid dotted texture binding names, duplicate dotted specs, and indexed path rejection
- Updated `docs/design/lp-shader-texture-access.md` with nested texture binding documentation and examples
- Updated `lp-shader/lps-filetests/README.md` with dotted path syntax documentation

### Decisions for future reference

#### Public API uses string keys, internal traversal uses parsed path helpers

- **Decision:** `BTreeMap<String, TextureBindingSpec>` keys are canonical dotted strings (e.g., `params.gradient`). Internal code uses `parse_path()` → `Vec<LpsPathSeg>` plus `LpsTypePathExt` for type/offset resolution.
- **Why:** Strings are ergonomic for public APIs and filetest directives; parsed segments enable type-safe traversal with existing path resolution infrastructure.
- **Rejected alternatives:** Custom `LpsPath` newtype (overkill for current needs); nested `BTreeMap` structure (would complicate filetest syntax and validation).
- **Revisit when:** We add deep nesting (>2 levels) or need indexed array paths for texture bindings.

#### Struct-only scope for nested textures (arrays rejected)

- **Decision:** Uniform arrays containing `Texture2D` (directly or nested) are rejected at validation time with a clear error.
- **Why:** Indexed paths (`params[0].gradient`) complicate the binding key syntax and runtime uniform application. This matches the "no dynamic indexing" philosophy elsewhere in lp-shader.
- **Rejected alternatives:** Supporting indexed paths (would require `LpsPathSeg::Index` in binding keys and more complex runtime logic).
- **Revisit when:** We have a compelling use case for uniform arrays of sampler bundles.

#### Naga GLSL limitation: sampler2D in struct members rejected at parse

- **Decision:** No GLSL filetests with actual `sampler2D` inside user structs; coverage uses synthetic metadata in unit tests.
- **Why:** Naga's GLSL front-end returns `Invalid struct member type` for `sampler2D`/`texture2D` in user struct definitions. This is a known upstream limitation.
- **Rejected alternatives:** Forking Naga or adding a custom GLSL extension (too heavy for this milestone).
- **Revisit when:** Upstream Naga adds support or we add a custom pre-processor rewrite pass.

#### Single-struct uniforms no longer hoist inner fields

- **Decision:** `uniform Params { sampler2D gradient; } params;` keeps `params` as the uniform root with nested `gradient` field, rather than hoisting `gradient` to top-level.
- **Why:** Aligns with user expectations (they wrote `params.gradient`), matches std430 layout, and makes dotted path resolution consistent across all uniform shapes.
- **Rejected alternatives:** Field hoisting (previous behavior) created mismatch between GLSL source and metadata layout.
- **Revisit when:** Never; this is the correct long-term behavior.
