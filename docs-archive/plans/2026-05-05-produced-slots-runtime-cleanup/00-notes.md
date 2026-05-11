# Produced Slots Runtime Cleanup Notes

## Scope of work

Define and plan a first implementation slice toward a node slot model where:

- A node has one slot namespace, addressed by a slot identifier, not by a value
  field/index path.
- Consumed versus produced is not part of the slot address.
- Consumption is performed through the resolver because bindings, defaults,
  overrides, bus channels, caching, tracing, and errors are all part of reading
  consumed values.
- Production is exposed by runtime nodes through a direct access surface because
  a node owns and writes what it produces.

The likely first implementation slice should clean up the runtime-facing model
without migrating the full wire/view system yet.

Possible in-scope items for the first plan:

- Rename or replace `RuntimePropAccess` and `RuntimeOutputAccess` with a single
  produced-slot access concept.
- Introduce a produced value type that can represent both scalar/model-ish
  values and engine-owned runtime products.
- Rename resolver/query/binding concepts away from `NodeInput` / `NodeOutput`
  toward consumed/produced slot terminology where it clarifies semantics.
- Update rustdocs so direction is described as an access/relationship property,
  not as a fixed namespace property.
- Decide what to do with the old `NodeRuntime` trait now that the demand-driven
  `Node` trait is the runtime spine.

Likely out of scope for the first plan:

- Replacing the compatibility wire `NodeDetail` / node-specific state structs.
- Reworking `lpc-view` into generic node data helpers.
- Implementing inline node definitions or artifact merge rules.
- Removing all current legacy node runtime implementations if they still serve
  compatibility tests or profiling symbols.
- Full source reload lifecycle.

## User notes that should influence the plan

- Consume versus produce is the hard rule. Namespaces such as `config`, `param`,
  and `state` are conventions, not fundamental type-system boundaries.
- `SlotAddress` should not know direction. Direction lives at the access or
  relationship level.
- Nodes have a single slot namespace for both production and consumption.
- A runtime node should have a produced-slot access surface for the things it
  produces.
- Consumption should always go through the resolver because bindings matter.
- Nodes can resolve their own consumed props in `tick()`, but not their produced
  props through the same mechanism.
- The current `Node` / `NodeRuntime` split looks wrong and should be examined.
- Slots probably should not be identified by `PropPath`. A slot is likely a
  string/newtype or specialized identifier; paths are for navigating inside
  structured values.
- The user has started renaming `PropPath` to `ValuePath`. This is the chosen
  name for the parsed `Vec<Segment>` representation.
- Authored path strings should usually be parsed at file/input boundaries,
  rather than carried through the domain model as wrapper types.
- Reference syntax probably wants to separate three concepts: node-or-bus
  locator, slot identifier, and value path within that slot. Binding semantics,
  however, should bind at the slot level because slots are the version boundary.
- The source/runtime node-reference split should be explicit:
  - `RelativeNodeRef` is a parsed source/model reference that needs a current
    node to resolve.
  - `RelativeNodeRefSrc` is only an authored-string helper where serde or
    source diagnostics still need raw text.
  - `NodeRef`, if introduced, should mean resolved runtime identity and should
    not be used for relative source expressions.
- Working terminology:
  - `SlotOwner`: the entity that owns a slot namespace; currently `Node` or
    `Bus`.
  - `SlotRef`: `SlotOwner + SlotName`.
  - `ValueRef`: `SlotRef + ValuePath`.
- A `ValueRef` can name nested data for reads/projection/diffs, but bindings
  should not generally target arbitrary sub-values. `SlotRef` is the intended
  bindable/versioned unit.
- For this plan, a slot name may remain an opaque string such as
  `config.width`. A later structured slot model can decide where that string
  becomes `SlotPath` or `SlotValue` structure.
- A bus should not be forced to become a node. A bus is a slot owner because it
  owns a routing namespace, but it does not have node lifecycle, authored node
  definition, tree position, or tick behavior.

## Current state of the codebase

### Path and value modeling

`lpc-model/src/prop/value_path.rs` represents nested value paths:

```rust
pub enum Segment {
    Field(String),
    Index(usize),
}

pub type ValuePath = Vec<Segment>;
```

This supports paths within a structured value such as:

```text
touches[1].x
image.width
diagnostics.compile_ms
```

This is a parsed representation. It should not also serve as the authored
string wrapper, and it should not identify the slot itself.

`lpc-model/src/node/relative_node_ref.rs` currently provides the source node
reference model:

- `RelativeNodeRefSrc(pub String)` is the authored/source string wrapper.
- `RelativeNodeRef` is the parsed relative reference with `parent_hops` and
  `segments`.

The rename is still mid-flight: comments and errors in that file still mention
`NodeLoc`. The plan should clean that up and decide whether source defs should
store `RelativeNodeRef` directly after serde, keeping `RelativeNodeRefSrc` only
as a boundary helper.

The produced/consumed slot model should mirror that split for value paths and
full value references.

`lpc-model/src/prop/model_value.rs` already provides a portable structured
value representation:

```rust
pub enum ModelValue {
    F32(f32),
    Vec2([f32; 2]),
    Array(Vec<ModelValue>),
    Struct { name: Option<String>, fields: Vec<(String, ModelValue)> },
    ...
}
```

`lpc-source/src/prop/src_shape.rs` provides authored slot shape/default metadata
with recursive `Scalar`, `Array`, and `Struct` forms. This is closest to a
declaration model for consumed authored slots.

### Current namespace model

`lpc-model/src/prop/prop_namespace.rs` currently defines:

```rust
pub enum PropNamespace {
    Params,
    Inputs,
    Outputs,
    State,
}
```

Its docs say `params` and `inputs` are consumed while `outputs` and `state` are
produced. That contradicts the desired model: direction should live on access
or relationship, not on root path segment. This file may become a convention
helper, be renamed, or be removed from the core path.

`NodePropSpec` parses `node#prop` into a `TreePath` and a `PropPath`. It also
has `target_namespace()` and current resolver code uses that to require
`NodeProp` bindings to address `outputs`. This is another hard-coded namespace
rule that should either become produced-slot validation or be relaxed.

### Current runtime node traits

There are two runtime traits:

- `lpc-engine/src/node/node.rs::Node`
- `lpc-engine/src/nodes/node_runtime.rs::NodeRuntime`

`Node` is the newer demand-driven runtime spine. It has:

```rust
fn tick(&mut self, ctx: &mut TickContext<'_>) -> Result<(), NodeError>;
fn destroy(&mut self, ctx: &mut DestroyCtx<'_>) -> Result<(), NodeError>;
fn handle_memory_pressure(...);
fn props(&self) -> &dyn RuntimePropAccess;
fn outputs(&self) -> &dyn RuntimeOutputAccess;
fn runtime_state(&self) -> &dyn RuntimeStateAccess;
```

The comments explicitly call `NodeRuntime` legacy. `NodeRuntime` still lives in
`lpc-engine/src/nodes/node_runtime.rs`, is re-exported from `lpc-engine/src/lib.rs`,
and is implemented by old `TextureRuntime`, `ShaderRuntime`, `OutputRuntime`,
and `FixtureRuntime` under `lpc-engine/src/legacy/nodes/**/runtime.rs`.

`NodeRuntime` has legacy-style methods:

```rust
fn init(&mut self, ctx: &dyn NodeInitContext) -> Result<(), Error>;
fn render(&mut self, ctx: &mut dyn RenderContext) -> Result<(), Error>;
fn update_config(...);
fn handle_fs_change(...);
```

The core project loader now attaches `Box<dyn Node>` instances. The old
`NodeRuntime` trait appears to remain only for legacy node runtime code, tests,
and profiling/debug symbol references.

Follow-up audit after user direction:

- `NodeRuntime` is re-exported from `lpc-engine/src/nodes/mod.rs` and
  `lpc-engine/src/lib.rs`.
- The only implementations are old legacy runtime files:
  - `lpc-engine/src/legacy/nodes/texture/runtime.rs`
  - `lpc-engine/src/legacy/nodes/shader/runtime.rs`
  - `lpc-engine/src/legacy/nodes/output/runtime.rs`
  - `lpc-engine/src/legacy/nodes/fixture/runtime.rs`
- The new core nodes reuse legacy fixture mapping/gamma helpers, but do not
  appear to use the old `*Runtime` implementations.
- `lpc-engine/tests/runtime_spine.rs` has explicit compatibility assertions
  that `NodeRuntime` is still reachable.
- `lp-cli/src/commands/profile/symbolize.rs` has a symbolization test fixture
  string/comment mentioning `FixtureRuntime::render` and `NodeRuntime`.

This suggests the cleanup phase can likely delete `NodeRuntime` and the old
legacy `runtime.rs` files, while preserving shared helper modules such as
fixture mapping and gamma.

### Current produced access split

`lpc-engine/src/prop/runtime_prop_access.rs::RuntimePropAccess` exposes produced
scalar/GLSL-compatible reflection values:

```rust
fn get(&self, path: &PropPath) -> Option<(LpsValueF32, FrameId)>;
fn iter_changed_since(&self, since: FrameId) -> Iterator<(PropPath, LpsValueF32, FrameId)>;
fn snapshot(&self) -> Iterator<(PropPath, LpsValueF32, FrameId)>;
```

`lpc-engine/src/prop/runtime_output_access.rs::RuntimeOutputAccess` exposes
non-scalar node outputs:

```rust
fn get(&self, path: &PropPath) -> Option<(RuntimeProduct, FrameId)>;
```

`RuntimeProduct` can currently be:

```rust
pub enum RuntimeProduct {
    Value(LpsValueF32),
    Render(RenderProductId),
    Buffer(RuntimeBufferId),
}
```

This means the current model is split by representation (`LpsValueF32` versus
`RuntimeProduct`) rather than by relationship (`produced slot`). Since
`RuntimeProduct` already has a `Value` variant, it is a strong candidate for
the produced-slot payload type or for being renamed into that role.

`RuntimeStateAccess` exists but is an empty marker trait reserved for future
sync/debug state snapshots.

### Current resolver model

`lpc-engine/src/resolver/query_key.rs` defines:

```rust
pub enum QueryKey {
    Bus(ChannelName),
    NodeOutput { node: NodeId, output: PropPath },
    NodeInput { node: NodeId, input: PropPath },
}
```

`ResolveSession` resolves:

- `Bus(channel)` by selecting the highest-priority provider binding for a bus.
- `NodeInput { node, input }` by finding a binding targeting that node input,
  otherwise calling `ResolveHost::produce` for an unbound input/default.
- `NodeOutput { node, output }` by calling `ResolveHost::produce`.

`EngineResolveHost` handles `NodeOutput` by ticking the producer node once,
then reading from `node.outputs().get(output)` first and falling back to
`node.props().get(output)` for scalar reflection. It handles unbound
`NodeInput` by producing defaults or local consumed values.

The current demand resolver already has the important conceptual split, but its
names are still `NodeInput` and `NodeOutput`.

### Current binding model

`lpc-engine/src/binding/binding_entry.rs` defines:

```rust
pub enum BindingSource {
    Literal(SrcValueSpec),
    NodeOutput { node: NodeId, output: PropPath },
    BusChannel(ChannelName),
}

pub enum BindingTarget {
    NodeInput { node: NodeId, input: PropPath },
    NodeOutput { node: NodeId, output: PropPath },
    BusChannel(ChannelName),
}
```

`BindingSource::NodeOutput` is conceptually "read a produced slot".
`BindingTarget::NodeInput` is conceptually "write or override a consumed slot".
`BindingTarget::NodeOutput` exists, but it is unclear whether it is needed in
the new model. A produced slot is owned by its node; letting a binding target a
produced slot may violate that ownership unless it represents an intentional
external writer/bus provider pattern.

### Current legacy slot cascade

`lpc-engine/src/resolver/resolver.rs` still contains an older/cascading slot
resolver:

```rust
resolve_slot(config, prop, ctx)
```

It resolves:

1. `NodeInvocation.overrides[prop]`
2. artifact slot `bind`
3. artifact slot `default`

It returns `ResolvedSlot` with `LpsValueF32`, not `RuntimeProduct`. The module
docs explicitly warn not to conflate this path with `ResolveSession` /
`Production`. This may be retained temporarily but should probably be renamed
or quarantined so the runtime model has one obvious resolver path.

## Open questions

### Q1: What is the first implementation scope?

Context: A full switch would touch runtime nodes, resolver, binding registry,
source declarations, compatibility projection, wire payloads, and client view
helpers. That is too much for one safe plan.

Suggested answer: make the first plan an engine-runtime cleanup only:

- introduce/rename produced-slot access and produced-slot payload types;
- update `Node` to expose produced slots through one access surface;
- update `EngineResolveHost` to read produced slots through that surface;
- rename resolver/query/binding terms where this is low-risk;
- update docs/tests;
- leave compatibility wire and `lpc-view` structurally intact, adapting them
  through compatibility projection if needed.

User direction: rename aggressively and delete wrong/transitional concepts where
possible. This is a domain-definition push, not a compatibility/finesse stage.

Status: partially resolved. The plan should be an aggressive engine/runtime
cleanup slice, but still needs a precise boundary around wire/view and old
legacy runtime code.

### Q2: Should `PropNamespace` remain a Rust-level semantic enum?

Context: `PropNamespace` currently bakes in the old idea that `params`/`inputs`
are consumed and `outputs`/`state` are produced. The desired model says nodes
have one slot namespace and direction is access-level.

Suggested answer: remove it from the semantic model. It may disappear entirely,
or a future convenience helper may replace it so nodes/tools can indicate
conventional roots such as `config`, `param`, or `state`. Do not require
produced slots to live under `outputs`.

User answer: this probably goes away. A convenience replacement may be useful
later, but direction must not be encoded as namespace.

Status: resolved.

### Q14: Are bindings slot-level or value-path-level?

Context: structured values create a versioning problem if bindings target
arbitrary nested values. `state.touches` should have a version, but
`state.touches[3].id` should not have an independent version. The slot is the
natural unit for version tracking, client diffs, and binding ownership.

Suggested answer: bindings should happen at the slot level. `ValuePath` remains
useful for addressing nested data inside a slot value, but not as the primary
binding/version boundary. A later plan should introduce a clearer `SlotValue` /
`SlotPath` model for structured data.

Status: resolved by user direction; captured in `future.md`.

### Q3: What should the produced slot payload type be?

Context: `RuntimeProduct` already represents scalar values and engine-owned
product handles. `RuntimePropAccess` exists only because scalar reflection is
separate from non-scalar products.

Suggested answer: keep `RuntimeProduct` as the produced slot payload. It is
already a produced thing: either a direct `Value` or a reference to an
engine-owned product/resource. The `Value` variant is important because a value
is not a resource reference.

User answer: `RuntimeProduct` is probably right.

Status: resolved.

### Q4: Should produced slot access support snapshots and changed-since for all produced values?

Context: `RuntimePropAccess` supports `snapshot` and `iter_changed_since`, but
`RuntimeOutputAccess` only supports `get`. Generic sync will eventually need
snapshot/change iteration over all produced values, including resource refs.

Suggested answer: yes for the new trait, but allow a minimal implementation
that returns boxed iterators and can be empty for nodes that only support `get`
at first. Prefer adding this capability now so compatibility projection has a
single future path.

Status: resolved by scope. Include this in the runtime cleanup so produced
access has one obvious shape.

### Q5: What happens to `NodeRuntime`?

Context: `NodeRuntime` is old render/init/update machinery. The newer
demand-driven `Node` trait is the active core runtime path. Keeping both names
without clear boundaries is confusing.

Suggested answer: start with a cleanup phase that decides and executes the
`NodeRuntime` removal/rename boundary. Having both `Node` and `NodeRuntime`
visible is confusing because it is not obvious which one is real. Prefer
deleting or deeply quarantining `NodeRuntime` before the slot rename work.

User answer: likely make this phase one; the current split is confusing.

Status: resolved in direction; implementation feasibility still needs review.

### Q6: Should binding endpoint names change now?

Context: `BindingSource::NodeOutput` and `BindingTarget::NodeInput` are close
to the desired model but use input/output language. `BindingTarget::NodeOutput`
may be semantically suspect if produced slots are owned by nodes.

Suggested answer: rename endpoints to role-based names:

```rust
BindingSource::ProducedSlot { node, path }
BindingTarget::ConsumedSlot { node, path }
```

Remove or postpone `BindingTarget::ProducedSlot` unless there is a clear current
use. Do not preserve old names just to reduce churn.

User answer: yes.

Status: resolved.

### Q7: Should `QueryKey` names change now?

Context: `QueryKey::NodeInput` really means "resolve this node's consumed
slot"; `QueryKey::NodeOutput` means "produce/read this node's produced slot".

Suggested answer: yes, if this is the runtime cleanup plan:

```rust
QueryKey::ConsumedSlot { node, path }
QueryKey::ProducedSlot { node, path }
```

This makes `TickContext::resolve(QueryKey::ConsumedSlot { ... })` read naturally
and prevents future code from assuming `output` is a namespace.

User answer: yes.

Status: resolved.

### Q8: Do we need a separate consumed-slot declaration type in this plan?

Context: `SrcSlot` already expresses shape/default/bind/presentation metadata,
but current core node defs still have hard-coded fields like `pin`,
`texture_loc`, and `glsl_path`. A complete consumed-slot declaration system
would touch source definitions and UI.

Suggested answer: no. Do not introduce a broad consumed-slot declaration model
in the first runtime cleanup plan. Continue resolving hard-coded consumed paths
through the resolver. Capture declarative slot metadata as future work.

User answer: probably not, depending on final scope.

Status: resolved by scope. Out of scope for this plan except for preserving any
existing hard-coded/default behavior.

### Q9: Should this plan include generic wire/view data?

Context: Generic wire data is a major migration. Current client/demo code still
depends on compatibility `NodeDetail` and node-specific state structs.

Suggested answer: no. The plan should create a runtime model that makes generic
wire possible later, but keep the compatibility projection working.

Status: resolved by scope. Out of scope for this plan; keep compatibility
projection structurally working.

### Q10: Are slots identified by `PropPath` or by a separate slot identifier?

Context: A path such as `output.image.width` is ambiguous if slots themselves
are `PropPath`s. It could mean:

- slot `output`, value path `image.width`;
- slot `output.image`, value path `width`;
- slot `output.image.width`, empty value path.

This suggests `PropPath` is really a value path, not a slot id. A slot may need
to be a separate identifier, probably a string/newtype with a restricted
grammar.

Possible reference shape:

```text
<node-or-bus> <sep> <slot> <sep> <value-path>
```

Examples under discussion:

```text
.lfo#output#phase
/bus#video_in#width
.lfo/output/phase
bus/video_in.0/width
```

Suggested answer: introduce a distinct slot identifier concept and rename or
alias `PropPath` toward `ValuePath`. The first implementation may not need the
final text syntax, but the Rust model should not blur slot identity with value
field traversal.

User answer: yes, slot identity should be separate. A `SlotRef` does not have a
value path; that is a `ValueRef`.

Status: resolved in concept.

### Q11: What is the abstraction over node and bus slot namespaces?

Context: both nodes and buses can own named slots/channels, but they have
different lifecycle and resolution semantics. The bus could theoretically be a
node, but that would force routing state into the node tree and add awkward
lifecycle/tick/config questions.

Suggested answer: use `SlotOwner { Node, Bus }` as the shared abstraction. A bus
is a slot owner, not a node.

Status: resolved.

### Q12: How do parsed value paths differ from authored path strings?

Context: the user has renamed `PropPath` to `ValuePath`, but the current type
is still effectively `Vec<Segment>`. That is a parsed path through a value, not
the string a user authored. This should line up with the node-location model:
source strings are wrappers/locators; runtime references are parsed structured
types.

Suggested answer: keep `ValuePath` as the parsed representation. Do not add
public string-wrapper domain types unless a specific boundary needs delayed
resolution or exact round-tripping. Parse authored strings at source/input
boundaries into semantic types.

```rust
pub struct SlotName(String);
pub enum SlotOwner { Node(NodeRef), Bus(BusRef) }
pub struct SlotRef { owner: SlotOwner, slot: SlotName }
pub struct ValueRef { slot: SlotRef, path: ValuePath }
```

Under this model:

- `NodeLoc` remains useful where node location resolution is context-dependent.
- Value-path strings should normally parse directly into `ValuePath`.
- Full value-reference strings should normally parse directly into `ValueRef`.
- `SlotRef` does not contain a value path.
- `ValueRef` is the first type that combines slot identity with field/index
  traversal inside the slot's value.

Status: resolved.

### Q13: Should source definitions store `RelativeNodeRefSrc` or `RelativeNodeRef`?

Context: after the user rename, source defs such as `ShaderDef.texture_loc` and
`FixtureDef.output_loc` still store `RelativeNodeRefSrc`. `ProjectLoader`
parses those refs when resolving node ids. This keeps raw authored strings in
the source model, but the emerging rule says unparsed values should not leak
past file/input boundaries unless round-tripping or delayed resolution requires
it.

Suggested answer: migrate source definitions toward parsed `RelativeNodeRef`
where possible. Keep `RelativeNodeRefSrc` only as a serde helper or input
adapter if deriving direct `Deserialize` for `RelativeNodeRef` is awkward.

Status: resolved by direction; implement as part of the model cleanup phase if
low-risk, otherwise document the remaining `Src` type as a temporary serde
boundary.
