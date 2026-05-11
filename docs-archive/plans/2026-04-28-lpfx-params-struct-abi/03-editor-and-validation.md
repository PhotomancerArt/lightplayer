# Phase 3: Update Editor/Validation Docs and Comments

## Scope of phase

Update the editor-facing milestone documents (M3, M6, M7) and clarify `lp-domain` code comments that could mislead about the shader ABI.

In scope:

- Update `m3-pattern-editor.md`:
  - Editor panel walks `Pattern.params` to build `params` struct values
  - Texture-backed params (palette/gradient) populate `params.*` fields
  - Dotted texture paths for resource binding
  
- Update `m6-semantic-editor.md`:
  - Widget edits update `params` struct fields
  - Palette/gradient recipe edits trigger rebake of `params.gradient` texture
  - TOML persistence stores recipes, not baked bytes
  
- Update `m7-cleanup-verification.md`:
  - Add validation audit for `params` struct ABI
  - Example migration checks for flat `param_*` → `params.*`
  
- Update `lp-domain` code comments:
  - `src/kind.rs`: Clarify that `ColorPalette`/`Gradient` storage recipes are for authoring/TOML, and lpfx bakes to texture fields in `params`
  - `src/visual/effect.rs`: Mark `inputColor` as stale/pending M4 naming decision

Out of scope:

- Core domain model changes (model is fine, comments only)
- Implementation code (just comments and docs)
- Runtime milestone docs (Phase 2 handled those)

## Code Organization Reminders

- Keep comment changes minimal - just clarify authoring vs shader ABI distinction.
- Doc updates should be consistent with Phase 2 changes.
- Group related updates together.
- Mark anything uncertain with TODO for M4 design to resolve.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations.

## Implementation Details

### Target files

Docs:
- `docs/roadmaps/2026-04-23-lp-render-mvp/m3-pattern-editor.md`
- `docs/roadmaps/2026-04-23-lp-render-mvp/m6-semantic-editor.md`
- `docs/roadmaps/2026-04-23-lp-render-mvp/m7-cleanup-verification.md`

Code:
- `lp-domain/lp-domain/src/kind.rs`
- `lp-domain/lp-domain/src/visual/effect.rs`

### Doc updates

#### m3-pattern-editor.md

Find and update:

1. "Param panel: walks `Pattern.params.0` (the root `Slot`), calls `lp_studio_widgets::widget_for_slot(slot)`" → "Param panel: walks `Pattern.params.0` (the root `Slot`), builds `params` struct values for the shader; calls `lp_studio_widgets::widget_for_slot(slot)`"

2. "Texture-backed palette/gradient params: the widget writes the authoring recipe, lpfx rebakes the corresponding height-one resource texture" → add "and binds it as `params.gradient` (or `params.palette`)"

3. Key decisions - add:
   - `params` is the shader-visible parameter surface
   - Texture-valued params use dotted paths like `params.gradient`

#### m6-semantic-editor.md

Find and update:

1. "Palette and gradient authoring stays recipe-based: widgets edit TOML recipes, lpfx rebakes width-by-one runtime textures" → add "and writes them to `params` struct fields"

2. "Param tweaks via widgets reserialize into the in-memory TOML buffer" → "Param tweaks via widgets update `params` field values and reserialize"

3. "Palette/gradient widget edits reserialize their authoring recipes and invalidate the corresponding runtime resource texture" → add "so the next preview samples the rebaked texture via `params.palette` or `params.gradient`"

#### m7-cleanup-verification.md

Find and update:

1. Add to "Texture-resource validation audit":
   - `params` struct ABI validation: dotted texture paths work
   - Example shader migration audit: no remaining flat `param_*` uniforms in lp-render MVP examples
   - Palette/gradient as `params.*` texture fields

2. Update "Roadmap summary" deliverable to mention params ABI adoption

### Code comment updates

#### src/kind.rs

Find `ColorPalette` and `Gradient` variant doc comments. They currently describe fixed-max struct storage. Add clarification:

```rust
/// Fixed-max palette: [`MAX_PALETTE_LEN`], `count`, and `entries` (`quantity.md` §3, `color.md`).
/// 
/// Note: This is the **authoring/storage** recipe. At runtime, lpfx bakes the palette
/// to a height-one texture and binds it as a shader field like `params.palette`.
ColorPalette,

/// Gradient with stops; [`MAX_GRADIENT_STOPS`] and [`InterpMethod`] (`quantity.md` §3).
/// 
/// Note: This is the **authoring/storage** recipe. At runtime, lpfx bakes the gradient
/// to a height-one texture and binds it as a shader field like `params.gradient`.
Gradient,
```

Also update the `storage()` method comments if they imply shader struct usage:

```rust
/// Returns the **structural** [`LpsType`] the shader, serializer, and
/// runtime agree on: the "storage recipe" for this [`Kind`]
/// (`docs/design/lightplayer/quantity.md` §3, "Storage recipes", and `impl`
/// block in §3).
/// 
/// For `ColorPalette` and `Gradient`, this is the **authoring** storage type.
/// The shader-visible runtime form is a baked texture field inside `params`.
```

#### src/visual/effect.rs

Find the `inputColor` mention. Update to:

```rust
/// An input-transforming Visual: input slot + shader + parameter
/// surface. The shader reads the input via a sampler uniform
/// (conventionally named `inputColor` in current examples; 
/// M4 Stack/Effect design should settle the final naming: `input`,
/// `inputImage`, or `inputTex`).
/// 
/// Note: The input sampler is **not** part of authored `params`;
/// it is a graph-fed resource supplied by Stack composition or bus binding.
```

### Validate

```bash
# Verify lp-domain builds and tests still pass
cargo test -p lp-domain --lib 2>&1 | tail -10

# Check no broken references in docs
grep -r "param_" docs/roadmaps/2026-04-23-lp-render-mvp/*.md | grep -v "params\." | head -20

# The above should only show intentional references, not outdated examples
```

Report back which files were updated and confirm tests pass.
