# 03 — `Artifact`, `ArtifactSpec`, `ArtifactManager`

The **artifact** is a Lightplayer's first-class novelty over a
plain "scene tree." It's the on-disk *class* / prototype, separate
from any one running instance.

## What an artifact is

An artifact is a versioned, on-disk, parsed, schema-validated
prototype that the runtime can instantiate one or more nodes from.

Concretely, an artifact carries:

- A **schema version** (`schema_version: u32`) for migration.
- A **kind tag** (`KIND: &'static str`, e.g. `"pattern"`).
- A **slot schema** — the node's parameter surface (see [05](05-slots-and-props.md)).
- **Default values** for every slot (mandatory, recursive into
  Array / Struct).
- **Default bindings** (optional `bind` per slot — author hint
  about the "natural" bus channel).
- **Embedded code** for nodes that wrap shaders or builtins
  (e.g., GLSL source, `builtin = "fluid"`).
- **Structural children** declared by the artifact itself: the
  Stack's `[input]` and `[[effects]]`, a node's `[children.<name>]`
  Sidecars. (Inline children come from the parent's `NodeConfig`,
  not the artifact — [04](04-config.md).)

What an artifact does *not* carry:

- A `NodeId`. Artifacts are shared; ids are per-instance.
- Authored use-site data. Per-instance overrides (literals,
  bindings, child instantiations from `[input.params]`) live on
  the *parent's* `NodeConfig`, not the child's artifact
  ([04](04-config.md)).
- Runtime state. No `Prop<T>`s. The artifact is Plain Old Data.

## `Artifact` trait (lpc-model)

Already shipped in M2 as a stub:

```rust
pub trait Artifact {
    const KIND: &'static str;       // "pattern", "effect", "texture", …
    const CURRENT_VERSION: u32;     // breaking-schema bump only

    fn schema_version(&self) -> u32;
    fn walk_slots<F: FnMut(&Slot)>(&self, _f: F) {}
}
```

M3 / M4 extend it as needed. Likely additions when the runtime
spine lands:

```rust
    /// Iterate the artifact-declared structural children
    /// (Input + Sidecar). Inline children are not declared by
    /// the artifact.
    fn walk_structural_children(
        &self,
        f: &mut dyn FnMut(StructuralChild<'_>),
    );

    /// Optional: prepare-once payload (e.g. compiled shader
    /// program). Called by the manager when transitioning
    /// Loaded → Prepared. Default impl is no-op.
    fn prepare(&self, ctx: &mut PrepareCtx) -> Result<(), PrepareError> {
        Ok(())
    }
```

```rust
pub enum StructuralChild<'a> {
    Input    { name: NodeName, slot_idx: usize, source: &'a VisualInput },
    Sidecar  { name: NodeName,                 source: &'a SidecarRef },
}
```

`StructuralChild` carries enough for the runtime to spawn an
`EntryState::Pending` child without parsing TOML. The exact shape
is settled in M4 when the visual integration pulls on it; for M5,
legacy nodes have no structural children and the walk is empty.

## `ArtifactSpec` (lpc-model)

```rust
pub struct ArtifactSpec(pub String);
```

Already shipped in M2. An opaque string payload referring to an
on-disk artifact. M5 leaves resolution intentionally minimal: it's
a path-from-root or path-from-parent string, treated as an LpPath.

Future enhancements (deferred):

- **Search-path resolution.** `"core://patterns/fluid"` resolving
  through a project search path.
- **Versioned references.** `"./fluid.pattern.toml@v3"` pinning
  to a specific schema version.
- **Builtin references.** `"builtin://fluid"` for prototypes
  shipped with `lp-vis`.

The opaque-string design supports all three additively.

## Loading

Already shipped in M2's `lpc_model::artifact::load_artifact`:

```rust
pub fn load_artifact<T, R>(fs: &R, path: &LpPath)
    -> Result<T, LoadError<R::Err>>
where
    T: Artifact + serde::de::DeserializeOwned,
    R: ArtifactReadRoot,
{ … }
```

This handles the **Resolved → Loaded** transition (parse + schema
validate) only. Prepare is the manager's job.

## `ArtifactManager` (lpc-runtime, lands in M4)

The manager owns:

- A `BTreeMap<ArtifactSpec, ArtifactEntry<D>>` of currently-known
  artifacts.
- A handle / refcount discipline (see below).
- The state machine.
- An fs-watch hook that bumps `content_frame` on disk changes.

```rust
pub struct ArtifactManager<D: ProjectDomain> {
    artifacts: BTreeMap<ArtifactSpec, ArtifactEntry<D>>,
    fs: Rc<RefCell<dyn LpFs>>,
    next_handle: u32,
}

pub struct ArtifactEntry<D: ProjectDomain> {
    pub spec: ArtifactSpec,
    pub state: ArtifactState<D::Artifact>,
    pub refcount: u32,
    pub content_frame: FrameId,   // bumped on each successful reload
    pub error: Option<ErrorReason>,
}
```

`D::Artifact` is the domain's artifact union (see
[08](08-domain.md)). For `LegacyDomain`, it's
`Box<dyn LegacyConfig>`; for the future `VisualDomain`, it's an
enum over `Pattern` | `Effect` | `Stack` | `Live` | `Playlist` |
`Transition`.

### State machine

```rust
pub enum ArtifactState<A> {
    /// Path validated; refcount > 0; TOML not yet parsed.
    Resolved,
    /// TOML parsed and schema-validated; ready to spawn instances.
    Loaded(A),
    /// One-time prep done (e.g. shader compiled into a shared
    /// program). For artifacts without expensive prep, identical
    /// to Loaded with an extra marker.
    Prepared(A),
    /// Refcount = 0 but cached; eligible for eviction under
    /// memory pressure. Idle artifacts still answer queries.
    Idle(A),
    /// Path lookup or filesystem read failed.
    ResolutionError,
    /// TOML parse / schema validation failed.
    LoadError,
    /// One-time prep failed (e.g. shader didn't compile).
    PrepareError,
}
```

### Transitions

- **`<unknown>` → `Resolved`** on first reference (path probe + ref).
- **`Resolved` → `Loaded`** **eagerly at parent-init**. Catches
  schema errors at the earliest possible moment; cost is bounded
  TOML parse per referenced artifact. (Lazy variant rejected:
  reading "schema error in fluid.pattern.toml" only when the user
  switches to that pattern is bad UX.) See "Open: when does
  Resolved → Loaded happen?" in [`../notes.md`](../notes.md) for
  the full discussion.
- **`Loaded` → `Prepared`** on first instance wake that requires
  prep. For artifacts with no `prepare` (default impl), this is a
  trivial state bump. For shader-bearing artifacts, this is a
  GLSL compile (= JIT, = real time); see also [02](02-node.md)
  §1.Z note on warmup-pass refinement.
- **`*` → `Idle`** when refcount drops to 0; entry retained.
- **`Idle` → eviction** (drop entry entirely) under memory pressure
  or explicit LRU.
- **`Idle` → `Loaded` / `Prepared`** again on next reference (no
  re-parse needed unless evicted).
- **Any → corresponding `*Error` state** on failure; the error
  propagates to whatever called for the transition.

### Refcounting

Each `EntryState::{Pending, Alive, Failed}` holds one ref on its
artifact. **`Failed` retains the ref** because a memory-pressure
release may demand a re-attempt without re-parsing.

Ref-bump happens at `EntryState::Pending` creation (parent-init);
ref-drop happens at entry destruction. There is no separate
"re-resolve" path — node config changes that swap artifact refs
go through the standard drop-old / acquire-new sequence.

### Hot reload

When the fs-watcher reports a change to the artifact's source file:

1. Manager re-parses the file. On success: replace the cached
   `Loaded(A)` payload, **bump `content_frame`**, leave refcount
   alone.
2. On parse failure: transition to `LoadError`, keep the previous
   payload around for one more tick (so dependents can finish their
   current tick), then drop. Status of dependent entries → `Warn`.
3. **Nodes observe the change at next `tick`** via
   `ctx.artifact_changed_since(frame)`. The spine never invokes a
   node hook on artifact reload; nodes pull when they're ready.

This is the same pull-at-tick pattern used for `NodeConfig` changes
([06](06-bindings-and-resolution.md)).

### Prepare for shader artifacts

A `Pattern` or `Effect` carries embedded GLSL. The first node that
wakes against a freshly-`Loaded` shader artifact triggers
`Loaded → Prepared`, which compiles the GLSL to native code via
`lpvm-native` (or `lpvm-cranelift` on host).

The compiled shader is **shared across all instances** of that
artifact: the artifact's `Prepared(A)` payload owns the compiled
program; instances borrow it. (A 2nd, 3rd, … instance of the same
Pattern doesn't pay the compile cost.) M5 has no instances of this
shape (legacy `Shader` nodes carry their own `node.json`-shaped
artifact with bespoke compile hooks); the prepare path is exercised
when `lpv-runtime` lands.

### Artifact handles

Spine code passes `ArtifactRef<A>` rather than raw `&A`:

```rust
pub struct ArtifactRef<A> {
    handle: u32,                                   // index into manager
    payload: NonNull<ArtifactEntry<A>>,            // for cheap deref
}
```

- `ArtifactRef::deref()` returns `&A` (panics if state ≠
  `Loaded`/`Prepared`/`Idle` — a programmer error).
- `ArtifactRef::content_frame()` returns the current
  `content_frame` so callers can compare against their last-seen.
- Drop decrements the refcount.

For the prototype, this can degenerate to `Rc<ArtifactEntry<A>>`
on host. ESP32 single-thread justifies the simple impl.

## What about `lpc-model` vs `lpc-runtime`?

- **`lpc-model::artifact`** owns the trait + the loader + spec
  type. No state, no manager. This is the wire-and-disk interface.
- **`lpc-runtime::artifact_manager`** owns the manager + state
  machine + refcount + `ArtifactRef`. This is the runtime
  convenience.

Splitting this way keeps `lpc-model` pure data (good for
filetests, schema gen, lpv-model consumers that want the types but
not the manager), while the manager lives where it can use `LpFs`
and friends.

## Worked examples

### Visual: a `Pattern` referenced from a Stack

```toml
# stack.toml (parent)
[input]
visual = "../patterns/fbm.pattern.toml"
[input.params]
scale = 6.0
```

Sequence:

1. Stack's parent (e.g., a `Live` candidate) is loaded.
2. Stack's artifact (`stack.toml`) is `Resolved → Loaded`.
3. Walking Stack's structural children, the runtime finds an
   `Input` referencing `"../patterns/fbm.pattern.toml"`.
4. The manager `Resolves` and `Loads` `fbm.pattern.toml` —
   refcount = 1.
5. Runtime creates a child `NodeEntry` with state `Pending`,
   storing the pattern's `ArtifactRef` and a `NodeConfig` whose
   overrides include `params.scale → Binding::Literal(6.0)`.
6. When something binds to the child's output (or the warmup
   pass walks the binding graph), `Pending → Alive` triggers:
   - Manager: `Loaded → Prepared` if prepare needed
     (compile the shader once).
   - Runtime: `D::instantiate(artifact, config, ctx)` →
     `Box<dyn Node>`.

### Visual: a second `Pattern` instance of the same artifact

The *second* Pattern instance referencing
`"./fluid.pattern.toml"`:

1. Manager already has `fbm.pattern.toml` in `Prepared`. Refcount
   1 → 2. No re-parse, no re-compile.
2. Runtime creates a `NodeEntry` with `Pending` state.
3. Wake: `D::instantiate` runs against the same shared artifact
   payload, producing a fresh `Box<dyn Node>`.

### Legacy: a Shader node

Legacy nodes are 1:1 with their `node.json`. Sequence:

1. fs-discovery finds `/src/my-shader.shader/node.json`.
2. Manager registers `ArtifactSpec("/src/my-shader.shader")` —
   `Resolved → Loaded` parses the JSON into a `ShaderConfig`
   (which, in the legacy bridge, *is* the artifact:
   `D::Artifact = Box<dyn LegacyConfig>` for `LegacyDomain`).
3. Runtime creates a `NodeEntry` with `Pending`.
4. Wake: `D::instantiate(artifact, config, ctx)` constructs a
   `ShaderRuntime` from the parsed config; the GLSL compiles
   inside `instantiate` (legacy bridge folds prepare and
   instantiate together because there's no shared compile across
   instances — every legacy shader is its own node).

The difference between visual and legacy artifact ergonomics is
*entirely* contained in `D::instantiate` and the `D::Artifact`
type. The spine sees `Resolved → Loaded → Prepared → instantiated`
the same way for both.

## Open questions

- **`Idle` cache size policy.** LRU? Fixed cap? Memory-pressure
  driven? Probably memory-pressure driven (the spine has a memory
  pressure event already), with a soft cap (e.g., 16) on host /
  emulator and 4 on ESP32. Not designing here.
- **Cross-artifact dependencies.** A `Stack` references a
  `Pattern`; a `Pattern` may reference (via `Inline` child) a
  `Modulator` artifact. The manager builds a dependency DAG
  implicitly (refcounting). Cycles in the DAG are possible if
  two Stacks reference each other through a chain. Detection
  belongs to the binding-resolver (cycle = error, [01](01-tree.md));
  manager doesn't need to know about the DAG.
- **Migration / `Migration` trait.** Schema-version bumps want
  a migration path. M2 stubs the trait; M5 doesn't exercise it
  (only one schema version per artifact kind exists). When
  schema_version 2 of `Pattern` arrives, the migration trait
  gets a real impl.
- **Disk-format heterogeneity.** Visual artifacts are `.toml`;
  legacy is `.json`. Manager picks the parser by file extension
  (or by domain hook). Pinned in M5 implementation, not here.
