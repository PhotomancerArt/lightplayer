# 04 — `NodeConfig`

`NodeConfig` is the **per-instance authored use-site data** for a
node. The artifact ([03](03-artifact.md)) is the *class*;
`NodeConfig` is the *instance customization*.

## What `NodeConfig` is for

When a parent's TOML says

```toml
[input]
visual = "../patterns/fbm.pattern.toml"
[input.params]
scale = 6.0
```

…then somewhere in the tree there's a child node that's an
*instance* of `fbm.pattern.toml`. That child's `NodeConfig`
captures the use-site customization:

- **Which artifact** it instantiates: `ArtifactSpec("../patterns/fbm.pattern.toml")`.
- **Which slot defaults / bindings to override:** `params.scale →
  Binding::Literal(6.0)`.

That's the whole job. Two fields:

```rust
pub struct NodeConfig {
    /// Which on-disk artifact this instance uses.
    pub artifact: ArtifactSpec,

    /// Per-instance authored overrides keyed by PropPath into the
    /// artifact's slot tree. Each entry replaces (a) the artifact
    /// slot's default value, or (b) the artifact slot's default
    /// `bind`, depending on whether the binding variant is
    /// `Literal` vs `Bus`/`NodeProp`. Empty for legacy nodes.
    pub overrides: BTreeMap<PropPath, Binding>,
}
```

(The cascade-binding case from `Live` — overrides addressed at
descendants via `[bindings] "candidates/0#emitter_x" = { ... }` —
also lands in `overrides` after path resolution; see §Cascade below.)

## What `NodeConfig` is **not**

- **Not the schema.** The artifact owns the schema. `NodeConfig`
  only carries values (and bindings, which the resolver dereferences
  to values).
- **Not the structural tree.** Children declared in the artifact
  (`[input]`, `[[effects]]`, `[children.<name>]`) come from the
  *artifact*, not `NodeConfig`. Inline children (`[params.<n>.bind]
  visual = "..."`) come from `NodeConfig`'s overrides — but only
  after desugaring (see §Inline children below).
- **Not the runtime state.** `Prop<T>` produced fields live in the
  impl's `*Props`, not here.
- **Not a trait.** The legacy `pub trait NodeConfig { kind() ->
  NodeKind; as_any() -> &dyn Any; }` retires. Kind tags live on
  the artifact (`Artifact::KIND`); downcasting is a legacy-only
  bridge concern (see §Legacy bridge below).

## Where it's stored

On the `NodeEntry`:

```rust
pub struct NodeEntry<D: ProjectDomain> {
    // ...
    pub config:   NodeConfig,                  // this file
    pub artifact: ArtifactRef<D::Artifact>,    // §03
    pub config_ver: FrameId,                   // bumped on any change
    // ...
}
```

`config_ver` increments whenever any field of the entry's `config`
changes — by the editor (set_property), by an fs-reload of the
parent (the cascade source rewrites), by an Inline child being
created or destroyed, etc. The impl's tick observes via
`ctx.changed_since`.

## `Binding` variants in `overrides`

Quick recap (full design in [06](06-bindings-and-resolution.md)):

```rust
pub enum Binding {
    Bus(ChannelName),                  // bus = "audio/in/0/level"
    Literal(LpsValue),                 // literal = 0.7   (or shorthand)
    NodeProp(NodePropRef),             // node = { node = "...", prop = "..." }
}

pub struct NodePropRef {
    pub node: NodePath,                // absolute or relative
    pub prop: PropPath,                // "outputs[0]" etc.
}
```

What an override does:

- **`Literal(value)`** replaces the artifact slot's default value.
  This is what `[input.params] scale = 6.0` parses to (after the
  loader recognises a bare value as `Binding::Literal` — see
  §Authoring shorthand below).
- **`Bus(channel)`** replaces the artifact slot's default `bind`.
  This is `[input.params] speed = { bus = "audio/in/0/level" }`.
- **`NodeProp(ref)`** points to another node's output. Authored
  directly via `bind = { node = { ... } }`, or *implicitly* by
  the `Inline` desugaring (see §Inline children).

## Authoring shorthand

The on-disk form has *two* shapes for a slot override, chosen per
key by the loader:

```toml
[params]
scale  = 6.0                            # Binding::Literal(6.0)
speed  = { bus = "audio/in/0/level" }   # Binding::Bus("...")
color  = "oklch(0.72 0.14 285)"         # Binding::Literal(<color>)
```

The bare value form (left-hand side) is the common case. The
explicit-table form (`{ bus = ... }` / `{ literal = ... }` /
`{ node = ... }`) opts into the full `Binding` enum. The loader
distinguishes by inspecting the value's table shape — a table with
exactly one of `bus` / `literal` / `node` keys is a binding; any
other table or scalar is a literal.

This matches the existing `[params.<name>.bind] = { bus = "..." }`
syntax in `lpv-model::Slot.bind` ([05](05-slots-and-props.md)) and
extends it with the bare-literal shorthand.

## Inline children (loader desugaring)

The `Inline` child kind ([01](01-tree.md)) is *authored* on a
slot's `bind`:

```toml
[params.gradient.bind]
visual = "../fluid.pattern.toml"
[params.gradient.bind.params]
intensity = 0.7
```

The runtime never sees a `Binding::Visual`. Instead, the loader
desugars this into:

1. **Spawn a new child entry** for `fluid.pattern.toml` under the
   parent, with `ChildKind::Inline { source: "params.gradient" }`.
   That child gets its own `NodeConfig`:
   ```rust
   NodeConfig {
       artifact: ArtifactSpec("../fluid.pattern.toml"),
       overrides: { "params.intensity" -> Literal(0.7) },
   }
   ```
2. **Install a `NodeProp` binding** on the parent's slot:
   ```rust
   parent.config.overrides.insert(
       "params.gradient",
       Binding::NodeProp(NodePropRef {
           node: <child path>,
           prop: parse_path("outputs[0]"),
       }),
   );
   ```

So the runtime `Binding` enum stays at three variants. The
authoring sugar is loader-side only.

**Round-tripping** the desugaring back to the original TOML for
serialisation / editor display: the parent's `NodeConfig` carries
the `(slot_path → child_id)` map for any `Inline`-spawned children
so the writer can recombine. M5 doesn't need to round-trip
(initial editor renders the desugared form); add the round-trip
when the visual editor needs it.

## Cascade `[bindings]` (Live, future)

The `Live` artifact has a cascade:

```toml
[bindings]
"candidates/0#emitter_x" = { bus = "touch/in/0/x" }
```

Keys are `<relative NodePath>#<PropPath>`. They address descendants
of the Live node, not the Live node itself.

**Resolution in M5:** the cascade entries are *materialized into the
descendants' `NodeConfig.overrides`* at parent-init time. The Live
node walks `bindings`, parses each key, descends to the addressed
node, and inserts the binding into that node's overrides. From the
spine's perspective, cascades are an authoring convenience — once
loaded, they look exactly like overrides authored at the descendant
itself.

**`config_ver` propagation:** bumping the Live's `config_ver` (e.g.
the user edits the cascade) triggers re-materialisation, which in
turn bumps `config_ver` on every affected descendant. Pull-at-tick
on each descendant picks it up.

**Cascade conflict resolution.** A descendant's own
`config.overrides` (authored at the use-site) *and* a cascade
binding from an ancestor can both target the same slot. M5 picks
**descendant-direct wins**: closer-to-the-leaf authoring is more
specific. Future authoring-error checks catch the conflict at
load.

## Legacy bridge

In M5, the legacy domain (`LegacyDomain` in [08](08-domain.md))
runs the same machinery with degenerate inputs:

| Field        | Visual instance                              | Legacy instance                                     |
|--------------|----------------------------------------------|-----------------------------------------------------|
| `artifact`   | `ArtifactSpec("../patterns/fbm.pattern.toml")` | `ArtifactSpec("/src/my-shader.shader")` (self)     |
| `overrides`  | author-supplied bindings/literals            | empty (legacy authors directly in the artifact)     |

A legacy node's "authored data" lives entirely in the artifact —
because legacy artifacts are 1:1 with instances, the type / instance
distinction is degenerate. There's no notion of "two different
shader nodes share a `node.json`."

To support this without a forked spine:

- `LegacyDomain::Artifact = Box<dyn LegacyConfig>`. The "artifact"
  for a legacy node *is* its parsed `*Config`.
- Legacy `LegacyConfig` is a private trait inside `lpl-runtime`:
  `{ kind() -> NodeKind, as_any() -> &dyn Any }`. Used only by
  `LegacyDomain::instantiate` to dispatch on kind.
- Legacy `node.json` reload through fs-watch updates the
  artifact's `Loaded(Box<dyn LegacyConfig>)` payload and bumps
  `content_frame` — same path visual artifacts use.

Once `lpv-runtime` lands and visual nodes coexist, the legacy
bridge can retire incrementally: a legacy node that gets a real
visual artifact (e.g., a Shader becomes a Pattern with embedded
GLSL) drops out of `LegacyDomain` and into `VisualDomain`. No
spine changes required.

## Hot reload via fs-watch

When a parent's TOML changes on disk:

1. **fs-watch event** routes to the parent's domain
   (`D::handle_fs_change`).
2. The domain re-parses the TOML, computes the new
   `(artifact, overrides)` pair for each affected child entry.
3. **Per-child diff:**
   - If `artifact` changed: drop the old `ArtifactRef` (refcount−),
     resolve the new spec (refcount+), reset `EntryState` to
     `Pending`, bump `config_ver`. Demand-driven re-wake from
     binding-resolution does the actual `D::instantiate`.
   - If only `overrides` changed: apply the diff in place, bump
     `config_ver`. The next tick's `ctx.changed_since` picks it
     up.
   - If a slot's binding changed *kind* and was previously an
     `Inline` `NodeProp`: the desugared child is destroyed (per
     [01](01-tree.md) Inline lifetime rules), and any new
     `Inline` is spawned.
4. **Cascade propagation:** if the parent has a cascade table, its
   entries are re-materialised into descendants' overrides. (See
   §Cascade above.)

The runtime never invokes a node hook on config reload. The impl
observes via `ctx.changed_since` next tick.

## Set-property edits (editor flow)

The editor talks to `lpc-runtime` through the existing
project-level set-property API (M2 has a stub:
`NodeProperties::set_property`). On the spine side:

1. `ProjectRuntime::set_property(node_path, prop_path, value)`
   walks to the entry, applies the override:
   `entry.config.overrides.insert(prop_path, Binding::Literal(value))`.
2. Bumps `entry.config_ver`.
3. Returns `Ok(())`.

That's the whole operation. No node hook fires synchronously. The
node observes the change via `ctx.changed_since` on its next tick
and reconciles internally.

`get_property` reads from the resolver cache (current value) or, if
the cache is stale, falls through to fresh resolution. See
[06](06-bindings-and-resolution.md).

## Worked example — Stack instance

```toml
# psychedelic.stack.toml
[input]
visual = "../patterns/fbm.pattern.toml"
[input.params]
scale = 6.0

[[effects]]
visual = "../effects/tint.effect.toml"

[[effects]]
visual = "../effects/kaleidoscope.effect.toml"
params = { slices = 8 }

[params]
intensity = { kind = "amplitude", default = 1.0 }   # this is artifact-side; goes
                                                     # into the Stack artifact's slot schema.
```

After loading:

- **Stack node**: `NodeConfig { artifact: "psychedelic.stack.toml",
  overrides: {} }`. (No author overrode the Stack's own params at
  the use-site of *this* TOML — they're declared on the artifact
  itself, with defaults already baked in.)
- **fbm.pattern Input child**: `NodeConfig { artifact:
  "../patterns/fbm.pattern.toml", overrides:
  { "params.scale": Literal(6.0) } }`. Spawned because Stack's
  artifact declares an `[input]` (structural).
- **tint.effect Input child**: `NodeConfig { artifact:
  "../effects/tint.effect.toml", overrides: {} }`. No params
  overrides at this use-site.
- **kaleidoscope.effect Input child**: `NodeConfig { artifact:
  "../effects/kaleidoscope.effect.toml", overrides:
  { "params.slices": Literal(8) } }`.

Each child entry is in `EntryState::Pending` until something pulls
on its output. Each artifact is refcounted by exactly the entries
that hold it.

## Why not put the artifact-spec ref in `NodeEntry` directly?

Could collapse `NodeConfig.artifact` into `NodeEntry.artifact_spec`
and drop the field. We don't, because:

- **Editor edit ergonomics.** "Change which Pattern this Stack
  uses" is conceptually a *config* change, not a tree-structural
  one. Keeping `(artifact, overrides)` together makes that one
  edit operation.
- **Hot-reload diffing.** When the parent's TOML changes, the
  domain handler diffs `(old_artifact, old_overrides)` against
  `(new_artifact, new_overrides)`. Having the pair lets it decide
  per-child whether to keep the entry or drop-and-create.
- **fs-snapshot symmetry.** A future fs snapshot of a node entry
  serialises `NodeConfig` directly (the authored shape on disk).

The `ArtifactRef` separately on `NodeEntry` is the *resolved*
form. Both fields are kept; they correspond to different layers
(authored vs resolved).

## Authoring-error model

`NodeConfig` accepts overrides that don't yet validate. Validation
happens at *resolution*:

- **Override path doesn't exist on the artifact's slot tree** —
  resolution warns (`NodeStatus::Warn`), the override is ignored.
- **Override binding kind doesn't match the slot's `Kind`** —
  resolution warns (`NodeStatus::Warn`), falls through to artifact
  default. (E.g., `Binding::Literal(LpsValue::I32(...))` against a
  `Kind::Color` slot.)
- **Override target node doesn't exist** (for `NodeProp`) —
  treated as a missing-channel: falls through to artifact default.
- **Cycle through `NodeProp` bindings** — compile-time error,
  surfaced when the binding graph is built ([01](01-tree.md)).

Validating eagerly at config-load is appealing but requires the
artifact to be `Loaded`, which may not have happened yet when the
parent's TOML is parsed (artifact load is itself deferred). Keep
validation at resolution; let the resolver build a "config issues"
report into `NodeStatus::Warn`.

## Open questions

- **`Override` vs `Binding` naming.** The map's value is a
  `Binding`; its semantic is an *override*. Two reasonable names:
  `overrides: BTreeMap<PropPath, Binding>` (M5 pick — describes the
  *what*), or `bindings: BTreeMap<PropPath, Binding>` (describes
  the *type*). Going with `overrides` for the loader-side framing;
  swap if it reads worse in code.
- **Round-trip of inline-child desugaring.** Carrying the
  `(slot_path → child_id)` map: is that on `NodeConfig` or on
  `NodeEntry`? Argument for `NodeConfig`: it's authored data. Argument
  for `NodeEntry`: the child id is runtime state. Lean
  `NodeEntry`. Pin in M5 implementation.
- **Cascade key parsing.** `"candidates/0#emitter_x"` is a relative
  `NodePropSpec`. Parse to `(NodePath, PropPath)` at config-load
  vs at resolution. Lean config-load (so `NodeConfig.overrides`
  always has a typed key); failed parses surface as `Warn`.
- **`[children.<name>]` on a NodeConfig (not the artifact).** Could
  let an author add a Sidecar at the use-site without authoring a
  fresh artifact. Currently spec'd as artifact-only (per
  [01](01-tree.md)). Defer to a real demand.
