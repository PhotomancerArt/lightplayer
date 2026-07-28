# ADR: Scoped buses with writer-shadowing

- **Status:** Accepted
- **Date:** 2026-07-28
- **Deciders:** Photomancer
- **Supersedes:** The playlist-entry "suppress produced visual defaults"
  loader rule (an undocumented special case of this model; see Context).
  Builds on declarative default bindings
  (`2026-07-09-declarative-default-bindings.md`) and the
  effects-are-projects decision
  (`2026-07-28-effects-are-projects.md`).
- **Superseded by:** None

## Context

The bus is the engine's "just works" wiring: a shader's consumed `time`
slot declares `default_bind: bus:time`, a clock's produced `seconds`
declares `default_bind: bus:time`, and a project with both animates with
zero authored bindings. Channels were global — one flat
`ChannelName → bindings` namespace per loaded project.

Embedding a project as a child node (the effects slice: an effect **is**
a project) breaks the flat namespace: the effect's inner shader would
publish `visual.out` onto the host's bus and clobber the host's visual,
and two effects side by side would fight over every conventional channel
name. The pre-existing loader rule that suppressed produced visual
defaults for playlist-entry children was exactly this problem in
miniature, solved as a special case: entry shaders are *alternatives*,
so their produced `visual.out` defaults were silently dropped and the
playlist published the blended result instead.

Constraints that shaped the decision:

- **No embedded-mode branches.** An effect must behave identically
  standalone and embedded; any "if embedded, disable X" rule creates
  context-dependent behavior (works alone, breaks inside a host, or vice
  versa).
- **Keep the magic.** Consumed defaults like `time` must keep flowing
  into effects from the host — an effect with no clock of its own should
  inherit host time, or it does not animate.
- **Zero migration.** Every existing project is a flat single-project
  tree; the model change must leave their load results and artifact
  bytes bit-for-bit identical, with no format bump.
- **Device parity.** Whatever the mechanism, it must run identically on
  sim and firmware paths — which means the loader (shared via
  lpa-server) rather than any host-specific service.

## Decision

Bus channels are keyed by `(scope, name)`. Scopes form a tree computed
at load time from the projected node spine:

1. **Every project node introduces a named scope** around its children.
   The root project is simply the outermost scope. Scope introduction is
   a property of the node kind, not the invocation site.
2. **Isolating invocation sites introduce an anonymous scope per owned
   child.** Today: playlist `entries[k].node` children. This *replaces*
   the suppression rule — an entry shader's produced `visual.out`
   default now lands in a scope nobody reads, which is behaviorally
   identical but falls out of the general model. Playlist entries are
   *alternatives* (must be isolated from each other); project children
   are *collaborators* (must share one scope). Container kinds differ in
   child semantics, which is why "container ⇒ scope" would be the wrong
   generalization and the model has two primitives instead of one.
3. **Consume resolution (writer-shadowing):** a consumed `bus:<name>`
   endpoint — authored or declared default — resolves to the nearest
   enclosing scope, starting at the consumer's own scope and walking
   outward, **that has at least one writer** for that channel. If no
   scope has a writer, it resolves to the **root** scope, so unfilled
   channels surface on the root bus exactly as before and a host can
   later fill them.
4. **Produce resolution (locality):** a produced `bus:<name>` endpoint
   always writes the producer's **own nearest scope**. Never outward. An
   effect cannot clobber a host channel by construction.
5. **Output mirror:** every project node exposes a produced `output`
   slot mirroring its own scope's `visual.out` channel. Root included —
   uniformity is the point; the root's mirror is simply unread today.
   The mirror is what makes an effect playlist-playable with zero
   playlist changes: the playlist already reads each entry child's
   produced `output` slot directly (slot reads are node-addressed and
   bus-independent). A scope with no visual writer renders cleared —
   a project without a visual is a legitimate shape, not an error.

The consume/produce asymmetry is deliberate: reads inherit (that is the
"just works" magic — lexical-style shadowing means an effect without a
clock inherits host time, and an effect *with* a clock shadows time for
its own subtree only, which is the self-contained-speed story), while
writes stay local (that is the encapsulation guarantee).

### Implementation shape

- Writer sets are computed at load time, per `(scope, channel)`: authored
  bus-target bindings plus produce-direction declared defaults (after
  rule 4 assigns them to their own scope), minus defaults overridden by
  an authored target on the same slot. Registration is two-pass — pass 1
  collects writers, pass 2 registers all bindings with consume endpoints
  resolved per rule 3. Binding registration order is unchanged (probe
  output ordering rides binding indices). Everything recomputes on
  refresh/mutation like all binding registration.
- The engine keys channels with an internal
  `ScopedChannel { scope: ScopeId, channel: ChannelName }`;
  `ScopeId::Project(node)` for named scopes, `ScopeId::Entry(child)`
  for anonymous entry scopes (distinct variants, so an entry-owned
  project node gets both). The type lives entirely inside `lpc-engine`;
  authored artifacts, schemas, and the wire format are untouched. At the
  probe boundary, root-scope channels display as bare names (identical
  to before) and non-root channels display scope-qualified.
- The mirror is a `ProjectNode` runtime node on the playlist pattern,
  minus the blending: `produce("output")` resolves its own scope's
  `visual.out` on demand and remembers the producer's `VisualProduct`
  handle, the published row carries the project node's *own* handle
  (product rows always name their owning node — playlist parity), and
  `RenderNode` dispatch forwards one hop to the remembered producer
  (cleared when there is none). No bindings are registered for the
  mirror, so the binding graph and bus pane of existing projects are
  unchanged. Its state root is engine-side (not part of the model's
  static shape catalog): no schema surface.
- `bus:time`'s default writer is the clock node's produced
  `ClockState.seconds` slot (`default_bind = "bus:time"`); there is no
  engine-injected time. Inheritance therefore covers effects (host
  clock's writer is in an enclosing scope), but a standalone effect
  project with no clock anywhere gets no time — effect workbench
  projects must include a clock in their preview rig.

## Consequences

- Existing projects: single root scope; every resolution identical;
  artifact bytes identical; no format bump. Nesting could not exist
  before, so the change is backward compatible by construction.
- Playlists: entry isolation now falls out of anonymous scopes; the
  suppression special case is deleted. Entry children's produced
  defaults now *register* (into the anonymous scope) instead of being
  dropped, which is invisible on the root bus pane but makes the entry
  shader's output slot show as bound — Studio labeling handles the
  scope qualifier (M3 of the composite-effects roadmap).
- A consumed channel written nowhere resolves to root and surfaces
  unfilled — hosts can fill an effect's dangling consumes later, which
  is the promotion story's escape hatch.
- Cross-scope references are intentionally not expressible. `bus:^foo`
  (parent) / `bus:/foo` (root) syntax is **reserved** for a future
  slice; the `BindingRef` parser continues to reject such strings
  naturally. Channel visibility/export declarations are likewise
  deferred.

## Alternatives considered

- **No auto-bind when embedded** — rejected: context-dependent behavior
  (an effect works standalone, silently stops animating when embedded,
  or vice versa). Violates the no-embedded-branches constraint.
- **Scope only `visual.out`** — rejected: partial scoping is confusing
  (why is `visual.out` isolated but a custom `energy` channel shared?);
  two effects would still collide on any non-visual channel.
- **Lint forbidding buses inside effects** — rejected: breaks the
  `time` default, i.e. breaks "just works" for the most common case.
- **Path-prefixed channel names as the authored model** (authors write
  `fx.visual.out`) — rejected: same semantics, worse model — pushes
  scope bookkeeping onto authors and makes vendoring a rename problem.
  (The internal `ScopedChannel` key is not this: authors never see it.)
- **One shared scope per playlist** (instead of per-entry anonymous
  scopes) — rejected: moves the `visual.out` collision from host level
  to playlist level instead of fixing it; entries are alternatives, not
  collaborators.
