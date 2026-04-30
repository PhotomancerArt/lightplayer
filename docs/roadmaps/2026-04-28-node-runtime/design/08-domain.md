# 08 — `ProjectDomain`, legacy bridge, visual mapping

> **M4.3a update:** The generic spine crate is `lpc-engine`, the
> wire crate is `lpc-wire`, and authored source types live in
> `lpc-source`. Older references to `lpc-runtime` in this document
> should be read as `lpc-engine`.

The spine is generic over a `ProjectDomain` trait that pins the
**domain-specific** parts: which artifact types exist, how to
instantiate a node from one, what response shape the wire ships.
M5 instantiates `LegacyDomain`; the next roadmap adds
`VisualDomain`.

This file is also the **legacy bridge plan** — it's where the
"what is `NodeKind`?" / "how do legacy nodes run on the new spine?"
questions get pinned.

## `ProjectDomain` trait

```rust
pub trait ProjectDomain: Send + Sync + 'static {
    /// The domain's artifact union. Legacy: Box<dyn LegacyConfig>.
    /// Future visual: enum { Pattern, Effect, Stack, ... }.
    type Artifact: Send + Sync + 'static;

    /// The domain's response payload for `get_changes`.
    /// Legacy: lpl_model::ProjectResponse.
    type Response: serde::Serialize + Send + Sync + 'static;

    /// Per-domain extra config the engine boots with.
    type BootConfig: Default;

    /// Construct a Node from an artifact + config + context.
    /// Called on EntryState::Pending → Alive transitions.
    fn instantiate(
        &self,
        artifact: &Self::Artifact,
        config: &NodeConfig,
        ctx: &mut InstantiateCtx,
    ) -> Result<Box<dyn Node>, Error>;

    /// Determine the artifact "kind tag" from a path on disk.
    /// Used by the loader to pick a parser.
    fn artifact_kind_from_path(&self, path: &LpPath) -> Result<&'static str, Error>;

    /// Parse an artifact from disk bytes into the domain's union.
    fn load_artifact(
        &self,
        kind: &'static str,
        bytes: &[u8],
        ctx: &mut LoadCtx,
    ) -> Result<Self::Artifact, Error>;

    /// Hook into ProjectRuntime::tick for domain-specific tick order.
    /// Legacy: the lazy "ensure_texture_rendered → ensure_shader →
    /// fixture_render → output_send" chain.
    /// Visual: post-order traversal.
    fn tick_pass(&self, rt: &mut ProjectRuntime<Self>) -> Result<(), Error>;

    /// Build a get_changes response.
    fn build_response(
        &self,
        rt: &ProjectRuntime<Self>,
        since_frame: FrameId,
        detail_specifier: &ApiNodeSpecifier,
        theoretical_fps: Option<f32>,
    ) -> Result<Self::Response, Error>;

    /// React to a filesystem change. The spine routes the change to
    /// the relevant entry; the domain decides what to do
    /// (re-parse, bump config_ver, recreate child, ...).
    fn handle_fs_change(
        &self,
        rt: &mut ProjectRuntime<Self>,
        change: &FsChange,
    ) -> Result<(), Error>;

    /// Called once at construction; sets up the bus, any default
    /// channels, and other domain-wide state.
    fn boot(&self, rt: &mut ProjectRuntime<Self>) -> Result<(), Error> {
        Ok(())
    }
}
```

The trait is small but does most of the per-domain work.
Everything that's *generic* — tree management, lifecycle, frame
versioning, panic recovery, sync skeleton, artifact manager —
lives in `lpc-engine` directly; everything that's *specific* —
which artifact types, how to instantiate, what to do on fs changes
— is `ProjectDomain` impl.

## `ProjectRuntime<D>`

```rust
pub struct ProjectRuntime<D: ProjectDomain> {
    pub frame_id: FrameId,
    pub frame_time: FrameTime,
    pub fs: Rc<RefCell<dyn LpFs>>,
    pub output_provider: Rc<RefCell<dyn OutputProvider>>,
    pub graphics: Arc<dyn LpGraphics>,
    pub time_provider: Option<Rc<dyn TimeProvider>>,
    pub memory_stats: Option<MemoryStatsFn>,

    pub tree: NodeTree<D>,                   // §01
    pub artifacts: ArtifactManager<D>,       // §03
    pub bus: Bus,                            // §06 (stub in M5)
    pub domain: D,
}
```

Methods on the runtime drive the trait via `self.domain.*`. The
generic skeleton:

```rust
impl<D: ProjectDomain> ProjectRuntime<D> {
    pub fn tick(&mut self, /* ... */) -> Result<(), Error> {
        self.frame_id = self.frame_id.next();
        self.publish_engine_time();           // bus update
        self.domain.tick_pass(self)           // domain decides order
    }

    pub fn handle_fs_changes(&mut self, changes: &[FsChange]) -> Result<(), Error> {
        for change in changes {
            self.domain.handle_fs_change(self, change)?;
        }
        Ok(())
    }

    pub fn get_changes(
        &self, since: FrameId, spec: &ApiNodeSpecifier, fps: Option<f32>,
    ) -> Result<D::Response, Error> {
        self.domain.build_response(self, since, spec, fps)
    }

    pub fn set_property(
        &mut self, node: &NodePath, prop: &PropPath, value: WireValue,
    ) -> Result<(), Error> {
        // generic: walk to entry, insert into config.overrides,
        // bump config_ver. No domain hook.
    }
}
```

Tick / fs-changes / get_changes go through the domain because they
need domain-specific logic (legacy lazy traversal, legacy response
shape). `set_property` is generic — every domain treats it the
same way.

## `LegacyDomain` (M5)

The existing `lpl-runtime` legacy_hooks.rs functionality, now
expressed as a trait impl.

```rust
pub struct LegacyDomain {
    // any state legacy needs
}

impl ProjectDomain for LegacyDomain {
    type Artifact = Box<dyn LegacyConfig>;     // self-pointing artifact
    type Response = lpl_model::ProjectResponse;
    type BootConfig = ();

    fn instantiate(
        &self,
        artifact: &Self::Artifact,
        config: &NodeConfig,
        ctx: &mut InstantiateCtx,
    ) -> Result<Box<dyn Node>, Error> {
        match artifact.kind() {
            NodeKind::Texture => {
                let tex_cfg = artifact.as_any()
                    .downcast_ref::<TextureConfig>()
                    .ok_or(...)?;
                Ok(Box::new(TextureRuntime::new(tex_cfg, ctx)?))
            }
            NodeKind::Shader  => { ... ShaderRuntime  ... }
            NodeKind::Output  => { ... OutputRuntime  ... }
            NodeKind::Fixture => { ... FixtureRuntime ... }
        }
    }

    fn artifact_kind_from_path(&self, path: &LpPath) -> Result<&'static str, Error> {
        // ".texture", ".shader", ".output", ".fixture" → "texture", ...
    }

    fn load_artifact(
        &self, kind: &'static str, bytes: &[u8], ctx: &mut LoadCtx,
    ) -> Result<Self::Artifact, Error> {
        // serde_json against TextureConfig / ShaderConfig / ... per kind
    }

    fn tick_pass(&self, rt: &mut ProjectRuntime<Self>) -> Result<(), Error> {
        // lp-engine's lazy traversal:
        //   for each Output { fixture.render() pulls texture pixels
        //   from the shader chain that targets that texture }
        // ported to the new tree shape.
    }

    fn build_response(...) -> Result<lpl_model::ProjectResponse, Error> {
        // walk the tree, build NodeChange list keyed on FrameIds
        // (status_ver, config_ver, state_ver); same logic as today.
    }

    fn handle_fs_change(...) -> Result<(), Error> {
        // node.json deleted → tree.remove(entry)
        // node directory created → tree.add_child + ArtifactManager.resolve
        // node file changed inside an existing entry → bump config_ver
    }
}
```

### `LegacyConfig` (private to `lpl-runtime`)

```rust
trait LegacyConfig: core::fmt::Debug {
    fn kind(&self) -> NodeKind;
    fn as_any(&self) -> &dyn Any;
}

impl LegacyConfig for TextureConfig { ... }
impl LegacyConfig for ShaderConfig  { ... }
impl LegacyConfig for OutputConfig  { ... }
impl LegacyConfig for FixtureConfig { ... }
```

This is the M2 `lpl_model::NodeConfig` trait, renamed and
narrowed:

- **Renamed** `NodeConfig` → `LegacyConfig` to free `NodeConfig`
  for the spine concept ([04](04-config.md)).
- **Moved** from `lpl-model` (where it shipped in M2) to
  `lpl-runtime` (where it's used). `lpl-model` keeps the per-kind
  `*Config` structs but no longer has the trait — the trait is a
  legacy-bridge implementation detail, not a wire concern.
- **No longer `pub`** outside `lpl-runtime`. Visual impls don't
  need it; legacy domain consumes it internally.

### Legacy node mapping

| Legacy `*Runtime`   | New `Node` trait method                                                                                      |
|---------------------|--------------------------------------------------------------------------------------------------------------|
| `init`              | retired — `LegacyDomain::instantiate` constructs the node, including any one-time setup                      |
| `update_config`     | retired — node observes via `ctx.changed_since` next tick                                                    |
| `update_artifact`   | retired — node observes via `ctx.artifact_changed_since` next tick                                           |
| `render(ctx, …)`    | `tick(&mut self, ctx)`; `delta_secs` → take a param bound to `engine/delta_secs` if you actually need it     |
| `destroy`           | `destroy(&mut self, ctx)`                                                                                    |
| `shed_optional_buffers` | `handle_memory_pressure(&mut self, level, ctx)`                                                          |

Per-node specifics:

#### `Texture`

| Slot                                    | Where it goes |
|-----------------------------------------|----------------|
| `width`, `height`, `format` (state today) | `params`, eventually — but M5 keeps them as `state` for the bridge; O-4 deferred |
| The pixel buffer                        | `outputs[0]`  |
| `frame` (last update)                   | `state`       |

`tick` is a no-op: textures are filled by shaders that target them.
The legacy "pull on demand" stays — `tick_pass` walks Outputs which
pull Fixtures which pull Textures which pull Shaders.

#### `Shader`

| Slot                       | Where it goes |
|----------------------------|----------------|
| `glsl_path`                | artifact field (legacy's degenerate artifact) |
| `texture_spec` (target)    | bus binding to a texture channel              |
| `render_order`             | artifact field                                |
| `glsl_opts`                | artifact field                                |
| `compile_time_ms`, `program` | `state`                                     |
| Compile error              | `state.error: Option<String>`                 |

`tick` runs the shader: resolves params (which may bind to
buses or sibling outputs), invokes the JIT-compiled program,
writes to the target texture.

#### `Output`

| Slot                  | Where it goes |
|-----------------------|----------------|
| `pin`                 | artifact (legacy degenerate)                                  |
| `options`             | artifact                                                       |
| Output channel handle | `state` (server-only — `#[prop(state, server_only)]`)         |

`tick` pushes the assembled buffer to the output provider.

#### `Fixture`

| Slot                                        | Where it goes |
|---------------------------------------------|----------------|
| `output_spec`, `texture_spec`, `mapping`, etc. | artifact (legacy degenerate)                            |
| Mapped pixel cache                          | `state`                                                  |

`tick` reads the texture, applies the mapping, writes to the
output's channel slice.

## `VisualDomain` (next roadmap)

Sketch for later. Lands in `lpv-runtime`:

```rust
pub struct VisualDomain {
    // bus, builtins registry, shader compiler ref, etc.
}

impl ProjectDomain for VisualDomain {
    type Artifact = lpv_model::VisualArtifact;
    type Response = VisualProjectResponse;
    type BootConfig = VisualBootConfig;

    fn instantiate(...) -> ... {
        match artifact {
            VisualArtifact::Pattern(p)  => { ... PatternRuntime::new(p, config) }
            VisualArtifact::Effect(e)   => { ... EffectRuntime::new(e, config) }
            VisualArtifact::Stack(s)    => { ... StackRuntime::new(s, config) }
            VisualArtifact::Live(l)     => { ... LiveRuntime::new(l, config) }
            // ...
        }
    }

    fn artifact_kind_from_path(...) -> &'static str {
        // "*.pattern.toml" → "pattern", etc.
    }

    fn load_artifact(...) -> Result<VisualArtifact, _> {
        // toml::from_str into the matching type
    }

    fn tick_pass(...) -> Result<(), _> {
        // visual graph traversal — possibly post-order on the
        // tree; or driven by Output nodes pulling texture chains
        // (similar to legacy). Pin in lpv-runtime.
    }
}

pub enum VisualArtifact {
    Pattern(lpv_model::Pattern),
    Effect (lpv_model::Effect),
    Stack  (lpv_model::Stack),
    Live   (lpv_model::Live),
    Transition(lpv_model::Transition),
    Playlist(lpv_model::Playlist),
}
```

Crucially: same spine, different domain. The **`lpc-engine`** crate footprint
doesn't need to balloon when visual lands (`ProjectDomain` parameterisation).

## Co-existence: legacy + visual?

Two domains can coexist via a meta-domain that dispatches by path
extension:

```rust
pub struct MultiDomain {
    legacy: LegacyDomain,
    visual: VisualDomain,
}

impl ProjectDomain for MultiDomain { ... }
```

This is the migration path: a project with both `*.shader` legacy
files and `*.pattern.toml` visual files runs them side by side
under one `ProjectRuntime`. Lands when `lpv-runtime` ships and
real projects start migrating; M5 doesn't need it.

## What changes in `lp-server`

`lp-server` today is hard-coded to `ProjectRuntime` (no generic).
Cutover plan:

```rust
pub type Project = ProjectRuntime<LegacyDomain>;     // M5 default
```

Type alias keeps the `lp-server` source surface tiny. Server code
that does `project.tick()` etc. compiles unchanged. Only places
that pattern-match on `lpl_model::ProjectResponse` need awareness
of the domain (and only because the response is domain-specific).

For mixed-domain server, the alias becomes
`type Project = ProjectRuntime<MultiDomain>` and `lp-server` is
generic over `D::Response`. Defer until `lpv-runtime` exists.

## What changes in `lp-engine-client`

Already-shipped `lp-engine-client::ProjectView` is generic over
the response type or hard-coded to `ProjectResponse`. M5 keeps
hard-coded; M6 / next-roadmap parameterises by `D::Response` if
needed for the visual editor. Pin in M5 implementation.

## Why this generalisation now (vs after lpv-runtime)?

We could leave `ProjectRuntime` non-generic until visual lands,
then generic-ify. We don't, because:

1. **The legacy bridge is itself a domain impl.** Designing the
   trait around legacy at the same time as M5 implementation
   means the interface gets one user before it freezes (and it
   freezes against the trickier user — the legacy bridge has more
   surface area than visual will).
2. **`PropAccess` derive and `NodeConfig` shape are co-designed
   with the trait.** Pinning the trait forces those
   collaborations to happen now, not when visual surfaces them.
3. **It's cheap.** `ProjectRuntime` becomes
   `ProjectRuntime<D>`; the generics are tight, monomorphisation
   has one impl in M5. Compilation cost is negligible.

## What this design intentionally does *not* settle

- **Channel routing across domains.** A legacy Texture publishing
  to a channel the visual side reads. The bus needs to be
  domain-agnostic; the spine carries a single `Bus` instance
  (not a `D::Bus` associated type). When mixed-domain projects
  arrive, validate.
- **Cross-domain bindings.** `Binding::NodeProp` from a
  visual node to a legacy node's   outputs. The spine treats both
  as `Box<dyn Node>` with runtime property access (`RuntimePropAccess` /
  engine-side `LpsValueF32`); cross-domain works when wire-facing recipes
  agree on **`WireValue` shape**.
- **Multi-domain `ArtifactManager`.** One manager per
  `ProjectRuntime<D>` keeps it simple. If `MultiDomain` arrives,
  the manager grows a per-extension dispatch (same crate, same
  cache).
- **Migration semantics for legacy → visual.** A legacy `.shader`
  becomes a `Pattern` artifact. The migration path is
  domain-author work; the spine offers `D::artifact_kind_from_path`
  + `D::load_artifact` as the integration points.

## Open questions

- **Domain composition naming.** `MultiDomain`? `MixedDomain`?
  `Domains`? Not biding now.
- **`InstantiateCtx` surface.** What does the constructor get?
  Legacy needs `&dyn LpFs` (to read auxiliary files like
  `main.glsl`), output provider, graphics, time provider. Visual
  may need a shader compiler handle. Lean: pass a generic
  `InstantiateCtx` that exposes everything via accessor traits;
  per-domain wrappers narrow as needed. Pin in M5 implementation.
- **Whether to inline `D::tick_pass` or expose generic helpers.**
  `tick_pass` could either be wholly opaque (each domain writes
  its own walker) or a strategy enum (`PostOrder` / `Lazy` /
  custom). Lean: opaque for M5, refactor if the legacy + visual
  walkers turn out near-identical.
