# Milestone 4: Stack + Effect — multi-node graph and texture pipeline

## Goal

Add `Effect` and `Stack` to the runtime: a multi-node graph
executor, a ping-pong texture pipeline that pipes one node's
output texture into the next node's input, shader sampler binding
for Effect inputs, artifact-cache
cross-file resolution (a `Stack` references `Pattern`s and
`Effect`s by relative path), and a stack editor page in
lp-studio. **`[input]` is `Visual`-only in this milestone**
(`[input] visual = "..."`). Bus inputs (`[input] bus = "..."`)
arrive in M5.

After M4, `psychedelic.stack.toml` (`fbm.pattern.toml` →
`tint.effect.toml` → `kaleidoscope.effect.toml`) renders
end-to-end in the editor.

## Suggested plan location

`docs/roadmaps/2026-04-23-lp-render-mvp/m4-stack-and-effect/`

Full plan: `00-notes.md`, `00-design.md`, numbered phase files.

## Scope

**In scope:**

- **`EffectInstance` node** in `lpfx/lpfx/src/nodes/effect.rs`:
  - Implements `NodeInstance`. One input texture (from upstream
    node's output), one output texture, one shader, params.
  - Compiles `Effect.shader` (same `ShaderRef` handling as
    `PatternInstance`).
  - At compile: derives a general 2D `TextureBindingSpec` for the
    conventional `inputColor` sampler (or the finalized convention
    from the design phase), using the upstream/output texture
    format.
  - At render: reads input texture handle from upstream, supplies
    it as `LpsValueF32::Texture2D` via
    `LpsTextureBuf::to_named_texture_uniform("inputColor")`,
    applies `params` struct (scalar and texture-valued fields), writes output.
- **`StackInstance` node** in `lpfx/lpfx/src/nodes/stack.rs`:
  - Implements `NodeInstance`. Composes a `[input] visual` child
    with a chain of `[[effects]]`.
  - Instantiates the input child Visual (recursive: `Pattern` or
    another `Stack`) via the engine.
  - Instantiates each `EffectRef.visual` Effect, cached.
  - At render: walks the chain, ping-ponging textures.
  - Param overrides on `EffectRef.params` and
    `VisualInputVisual.params` apply at instantiation time.
- **Multi-node graph executor** in `lpfx/lpfx/src/runtime/graph.rs`:
  - The previous "single-node" graph from M1 evolves into a real
    DAG executor. Topological order, render in dependency order.
  - For M4 the graph is always linear (Stack = chain of nodes).
    Branching graphs aren't a v1 concern (Live / Playlist / cross-
    artifact references would surface them).
- **Texture pipeline** in `lpfx/lpfx/src/texture/`:
  - Ping-pong buffer allocator. Two textures per pipeline,
    alternating as input/output across the chain.
  - Texture sizes: matched to the Stack's resolution (or default
    if unspecified). Resolution propagation rules resolved in
    design phase — Pattern produces at its `[meta]` resolution,
    Effects accept whatever input dimensions they get.
  - Keep ping-pong frame/output buffers separate from resource
    textures introduced in M1 (palette/gradient strips, later bus
    textures). Effects can sample either category, but only
    frame/output buffers participate in Stack ping-pong reuse.
  - Clarification: resource textures (params-owned like `params.gradient`)
    are distinct from graph-fed inputs (`inputColor`). Both are samplers,
    but their lifetimes differ: resource textures persist across frames;
    ping-pong buffers are ephemeral per-Stack-render.
- **Cross-file artifact resolution** in
  `lpfx/lpfx/src/runtime/cache.rs`:
  - `ArtifactSpec`-based resolution: when a Stack says
    `visual = "../patterns/fbm.pattern.toml"`, the cache resolves
    against the Stack's own path.
  - Cycle detection: a Stack referencing itself (directly or
    transitively) errors at instantiation.
- **Param override application**:
  - `EffectRef.params: BTreeMap<String, toml::Value>` overrides
    the Effect's default params at instantiation. Type checking:
    types must match the Effect's `ParamsTable` (this is the
    cross-artifact validation lp-domain M3 explicitly punted —
    we land it here for real).
  - Same for `VisualInputVisual.params`.
- **`StackEditor` page** in
  `lp-app/lp-studio/src/pages/stack_editor.rs`:
  - Open `*.stack.toml` files.
  - Preview pane shows the Stack's output (final effect's
    output).
  - Show the chain visually: input → effect → effect → output.
    Click an effect to see its params on the side panel.
  - Param panel works the same as M3's pattern editor (widgets
    via `widget_for_slot`).
  - In-memory edits only; persistence is M6.
- **Routing**: `/stack/:path`, plus link from files page.
- **Tests**:
  - `cargo test -p lpfx`: load `psychedelic.stack.toml`,
    instantiate, render, assert non-black output.
  - Cycle detection test: synthetic stack referencing itself
    errors cleanly.
  - Param override test: `[[effects]] params.intensity = 2.0`
    actually flows through to the Effect's render.
  - Effect texture input test: compile an Effect with `inputColor`
    as a general 2D sampler, bind an upstream texture, and assert
    the sampled output changes as expected.

**Out of scope:**

- Bus / `[input] bus = "..."` (M5).
- Live / Transition / Playlist node impls.
- Cross-artifact param-type validation across schema versions
  (the validation lands here for the *current* schema; multi-
  version validation is lp-domain M5 territory after this
  roadmap).
- DAG visual editor (drag nodes, connect with edges) — M4 shows
  the chain read-only; node-graph UI is a future concern.
- Resolution mismatch handling beyond "Pattern decides, Effects
  follow" — sophisticated rules later if needed.

## Key decisions

- **Effect authored params live in `params` struct; graph-fed inputs remain outside `params`.**
  The `inputColor` sampler (naming pending M4 design: `input` vs `inputImage` vs `inputTex`)
  is a top-level resource uniform, not a field of `params`. This keeps authored knobs separate
  from pipeline wiring.
- **Graph is a real DAG executor from M4 even though Stack is
  always linear.** Avoids a refactor when more topology arrives.
  Topological-order render; ping-pong textures across the chain.
- **Param-override type checking lands here.** lp-domain M3
  explicitly punted cross-artifact validation; M4 needs it
  because Stack overrides Effect/Pattern params. The validation
  is local (Stack vs the Visuals it directly references), not
  transitive — same model as compose-time validation in
  `quantity.md`.
- **`[input] visual` only** in M4. Examples like
  `psychedelic.stack.toml` use `visual` inputs; the example
  corpus is solved by visual-input alone. Bus inputs arrive in
  M5 alongside the bus itself.
- **Cycle detection at instantiation, not at parse.** The TOML
  parser doesn't know what other files exist; cycle detection
  needs the cache + the resolution graph. Errors propagate up
  from `Engine::instantiate_*`.
- **Stack editor UI is read-only graph view + per-node params
  panel** in M4. Drag-to-reorder effects, add/remove effects,
  visual node-graph editor — all later concerns. M4's UI exists
  to let you see and tweak existing stacks, not to author them
  from scratch.
- **Texture lifetime: ping-pong, two textures per Stack.**
  Effects don't see each other's prior outputs. If we ever want
  multi-input effects (e.g. blend two stacks), that's a future
  concern that motivates a real texture pool.
- **Effect inputs use the lp-shader texture contract.** There is
  no ad hoc `inputColor` pointer convention: compile-time
  `TextureBindingSpec` and runtime `Texture2D` uniforms are the
  contract. The naming convention is still lpfx policy, not
  lp-shader policy.

## Deliverables

- `lpfx/lpfx/src/nodes/effect.rs` (`EffectInstance`).
- `lpfx/lpfx/src/nodes/stack.rs` (`StackInstance`).
- Updated `lpfx/lpfx/src/runtime/graph.rs` (multi-node executor).
- New `lpfx/lpfx/src/texture/` module (ping-pong allocator).
- General 2D sampler binding for Effect input textures.
- Cache cross-file resolution + cycle detection.
- Param-override type validation.
- `lp-app/lp-studio/src/pages/stack_editor.rs` + routing.
- Tests covering Stack/Effect instantiation, render, param
  overrides, cycle detection.

## Acceptance smoke tests

```bash
cargo test -p lpfx --test render
# → psychedelic.stack.toml renders end-to-end

cd lp-app/lp-studio && dx serve
# → open /files, click psychedelic.stack.toml → "open in editor"
# → preview shows fbm tinted + kaleidoscoped
# → click "tint" effect, see its color param, edit, see preview update
# → click "kaleidoscope", see its segments param, edit, see preview update
```

## Dependencies

- M3 complete (Pattern editor proves the integration path
  works; Stack/Effect editor is "more of the same plus a
  graph").
- lp-domain Effect / Stack types already exist (M3 of lp-domain
  roadmap).

## Execution strategy

**Option C — Full plan (`/plan`).**

Justification: Multi-node graph executor + texture pipeline +
cross-file resolution + cycle detection + param-override
validation + new editor page. Real graph topology and texture
lifetime questions; the param-override-type-check brings in
lp-domain's cross-artifact validation that M3 punted.
Phaseable: Effect node + texture pipeline as one phase,
Stack node + cross-file cache as another, Stack editor UI
as a third.

> This milestone needs a full plan. I'll run the `/plan` process —
> question iteration, design, then phase files — and then `/implement`
> to dispatch. Agree?
