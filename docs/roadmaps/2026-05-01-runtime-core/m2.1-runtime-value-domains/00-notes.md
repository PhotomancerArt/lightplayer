# Scope of Work

M2.1 introduces the first runtime value/domain envelope for the core engine. The
goal is to let engine-owned resolution return a versioned produced value whose
payload is not hard-coded to `LpsValueF32`, while keeping M2's ordinary scalar /
shader-value behavior working.

This is a bridge milestone between M2 core engine work and M3/M4 legacy source /
runtime migration. It should prevent the legacy texture and future render
product path from being forced deeper into scalar/model value containers.

In scope:

- Add a small runtime value payload type for produced values.
- Move `ProducedValue` from `Versioned<LpsValueF32>` to
  `Versioned<runtime-value-envelope>`.
- Preserve simple value resolution through the existing resolver/session/cache
  path.
- Add lightweight domain/type inspection helpers so tests and future diagnostics
  can distinguish simple values from render-like products.
- Decide where portable domain descriptors live versus engine-only runtime
  handles.

Out of scope:

- Implementing real sampled render products.
- Implementing GPU texture storage or desktop render graph behavior.
- Porting concrete legacy shader/fixture/output runtimes.
- Removing every historical `Texture2D` occurrence from `ModelValue` or
  `LpsValueF32` in this milestone.
- Adding a full static type checker for graph bindings.

# Current State

The M2 core engine path has an engine-owned resolution stack:

- `lpc-engine/src/engine/engine.rs` owns `Engine`, `Resolver`,
  `BindingRegistry`, `NodeTree`, frame state, artifacts, and demand roots.
- `lpc-engine/src/resolver/query_key.rs` defines `QueryKey::{Bus, NodeOutput,
  NodeInput}`.
- `lpc-engine/src/resolver/produced_value.rs` defines `ProducedValue` as
  `Versioned<LpsValueF32>` plus `ProductionSource`.
- `lpc-engine/src/resolver/resolve_session.rs` resolves bus, node input, and
  node output queries, caches `ProducedValue`, and records trace/provenance
  events.
- `lpc-engine/src/node/contexts.rs` exposes `TickContext::resolve(...) ->
  ProducedValue`.

The value shape is still scalar/shader-value centric:

- `ProducedValue.value` is currently `Versioned<LpsValueF32>`.
- `ResolvedSlot` and several legacy/resolver cache paths still store
  `LpsValueF32` directly.
- Node runtime property access still returns `Option<(LpsValueF32, FrameId)>`.
- Literal materialization converts `ModelValue` into `LpsValueF32`.

There is already pressure from render-like data:

- `lpc-model/src/prop/model_value.rs` has `ModelValue::Texture2D { ptr, width,
  height, row_stride }`.
- `lpc-model/src/prop/model_type.rs` has `ModelType::Texture2D`.
- `lps-shared/src/lps_value_f32.rs` has `LpsValueF32::Texture2D`.
- `lpc-engine/src/wire_bridge/lps_value_to_model_value.rs` documents that
  converting `LpsValueF32::Texture2D` to `ModelValue::Texture2D` drops host
  metadata.

That texture descriptor path is useful compatibility evidence, but it should
not become the long-term representation for render products. The new engine
value envelope should make it possible for resolver/cache/provenance code to
handle multiple value domains while large or capability-backed products remain
owned by engine/node/product registries.

# Downstream Impact Areas

The domain/value split is likely to affect more than `ProducedValue`.

## Runtime property access

`lpc-engine/src/prop/runtime_prop_access.rs` currently documents and exposes
node-produced fields as `LpsValueF32`. That is acceptable for legacy or
shader-compatible props, but the core engine should not assume every node output
or state field is directly an `LpsValueF32` forever. M2.1 should either update
the new engine-facing access path to `RuntimeValue` or clearly leave
`RuntimePropAccess` as a legacy/data-only bridge and introduce the new path
elsewhere.

## Source defaults and params

`lpc-source/src/prop/src_value_spec.rs` still treats authored defaults as either
`SrcValueSpec::Literal(ModelValue)` or `SrcValueSpec::Texture(SrcTextureSpec)`.
This is a sign that params no longer map one-to-one to `LpsValueF32`: authored
source has recipes, literal data, and eventually domain-specific values.
M2.1 should not over-expand source loading, but the design should make clear
that source params materialize into runtime domains through the engine, not
directly into a single universal shader value.

## Kind and storage recipes

`lpc-model/src/prop/kind.rs` currently describes `Kind::Texture` as an opaque
handle with a `ModelType::Struct` storage recipe. That already points toward
domain separation: `Kind` is semantic meaning, `ModelType` is portable storage,
and `RuntimeValue` is the engine-time domain payload. M2.1 should preserve that
separation rather than turning `ModelType` into runtime resource identity.

## Wire bridge conversion

`lpc-engine/src/wire_bridge/lps_value_to_model_value.rs` currently converts
`LpsValueF32::Texture2D` to `ModelValue::Texture2D` while dropping host metadata.
If `ModelValue::Texture2D` is removed, the bridge needs to either stop exposing
texture descriptors through `ModelValue` or map them to an explicit compatibility
shape. M2.1 should treat this as cleanup of the old workaround, not as the new
render-product inspection API.

The new runtime-domain shape gives the wire protocol a better future path:
send texture/render-product references as identity-bearing references, while
actual texture payloads move through a separate resource/update channel. Two
references to the same texture should not duplicate pixels. This should be
planned soon, but it is not part of M2.1 unless needed for cleanup.

## Legacy resolver cache

`ResolvedSlot` and older resolver paths still store `LpsValueF32`. These are
legacy/transitional paths. They should not drive the new design, but phase work
should check whether any of them are still used by the M2 core engine path and
avoid silently reintroducing scalar-only assumptions there.

## Domains as an extension point

The plan should treat domains as a core runtime concept. Adding a future domain
should mean adding an enum variant and the corresponding engine capability,
tests, and diagnostics shape. This makes the cost of a new domain explicit
instead of hiding it behind `ModelValue` or `LpsValueF32`.

# Questions

## Q1: What should the runtime payload enum be called?

Context: The milestone needs a name for the payload inside `ProducedValue`, such
as `RuntimeValue`, `ProducedData`, `EngineValue`, or `CoreValue`.

Suggested answer: Use `RuntimeValue`. It is clear that this is an in-engine
runtime payload, not a portable model/wire value and not the whole
`ProducedValue` envelope.

Answer: Yes. Use `RuntimeValue`.

## Q2: Where should the value-domain descriptors live?

Context: `lpc-model` already owns portable value/type vocabulary
(`ModelValue`, `ModelType`, `Kind`), while `lpc-engine` can depend on
`lps-shared` and owns runtime-only conversion/resource behavior.

Suggested answer: Put portable domain/type descriptors in `lpc-model`, but keep
the actual `RuntimeValue` enum and any render-product handles in `lpc-engine`.
For M2.1, add the minimum descriptor needed for domain introspection; do not
move resource ownership into `lpc-model`.

Answer: Yes. Keep portable descriptors in `lpc-model`; keep runtime handles and
resource ownership in `lpc-engine`.

## Q3: Should M2.1 include a render-product placeholder variant?

Context: Adding only `RuntimeValue::Simple(LpsValueF32)` is the smallest change,
but it may let M3/M4 keep assuming all resolved values are simple shader values.
Adding a handle-shaped render variant now would make the intended split explicit
without implementing sampled products.

Suggested answer: Add a placeholder/handle-shaped render variant now, but keep it
small and non-functional. The resolver cache can carry it, tests can assert its
domain, and the real registry/trait work remains in M6.

Answer: Yes, and start thinking about `RenderProduct` now. A handle into
engine-managed product storage feels like the right pattern.

## Q4: What should happen to `ModelValue::Texture2D` and
`LpsValueF32::Texture2D` in this milestone?

Context: Those variants already exist and may still be used by shader ABI,
fixtures, tests, or compatibility conversion. Removing them now would be a wider
cross-crate migration.

Suggested answer: Leave them in place for compatibility, but document that they
are descriptor/ABI compatibility shapes rather than the future engine render
product abstraction. M2.1 should stop new `ProducedValue` APIs from depending on
texture-in-`LpsValue` as the only render path.

Answer: `LpsValueF32::Texture2D` is out of scope, but
`ModelValue::Texture2D` can be removed in M2.1 if the plan accounts for the
affected model/wire bridge tests and call sites.

## Q5: How much of the existing scalar API should be preserved?

Context: Many tests and call sites currently do `pv.value.get().eq(...)` or
pattern-match directly on `LpsValueF32`. A pure replacement would cause noisy
mechanical churn.

Suggested answer: Add ergonomic helpers such as `RuntimeValue::simple(...)`,
`RuntimeValue::as_simple()`, and possibly `ProducedValue::simple(...)`. Update
call sites deliberately, but keep scalar tests readable.

Answer: Yes. Add scalar/simple convenience helpers.

## Q6: Should legacy `ResolvedSlot` move to the new runtime value now?

Context: `ResolvedSlot` is an older per-node resolver cache shape that still
stores `LpsValueF32`. It may be transitional while M2 moves resolution into the
engine.

Suggested answer: Do not force all legacy cache types onto `RuntimeValue` unless
needed to compile after the `ProducedValue` change. Keep M2.1 focused on the
new engine-owned produced value path.

Answer: Yes. Keep legacy `ResolvedSlot` unchanged unless the new produced-value
path forces a narrower update.

## Q7: How much `RenderProduct` behavior belongs in M2.1?

Context: A `RuntimeValue::RenderProduct(handle)` variant would establish the
domain pattern, but the real product behavior may need more than a placeholder.
The important core behavior is likely not "texture storage" yet; it is the
ability for a fixture or consumer to ask an engine-managed render product for a
batch of sampled values. That makes shader/visual rendering lazy and fixture
demand-driven rather than forcing every visual output to materialize a full
texture first.

Suggested answer: Include the core wiring in M2.1, but not a full renderer. Add
the `RenderProduct` vocabulary, a handle type, a minimal sample request/result
shape, and an engine/product-store boundary that can answer batch sample
requests in tests. Defer real shader-backed products, texture-backed products,
GPU storage, and optimization to later milestones.

Answer: Yes. Include the core `RenderProduct` wiring now: handle-shaped runtime
values plus the ability to request sampled values from an engine-managed product
in tests. Keep concrete shader rendering, texture products, GPU storage, and
optimization policies out of M2.1.

## Q8: What should the non-render runtime value variant be called?

Context: `RuntimeValue::Simple(...)` describes complexity, not shape. The value
can be any ordinary GLSL/shader-compatible value: scalar, vector, matrix, array,
or struct. It is not necessarily scalar and not necessarily authored as a
literal.

Suggested answer: Use `RuntimeValue::Data(LpsValueF32)` or
`RuntimeValue::Value(LpsValueF32)`. Avoid `Literal` because a node output,
bus value, animated input, or default can all produce the same payload shape.
`Literal` should remain a production/source concept, not a value-domain name.

Answer: Use `RuntimeValue::Data(LpsValueF32)` for now. It is a little broad,
but less jargony than `Immediate`. In this milestone, "data" means directly
carried GLSL-compatible runtime data, as opposed to an engine-managed
capability/handle such as a render product.

Follow-up: Consider using the singular `Datum` instead of `Data`, and possibly
renaming `ModelValue` to `ModelDatum`. `Datum` makes the variant/type distinct
from generic "data" domains such as future audio buffers, though it is not a
perfect natural-language fit for compound values like vectors, arrays, or
structs.

## Q9: Should the direct payload be named Datum, and should ModelValue become ModelDatum?

Context: `Value` is becoming overloaded: the engine has `ProducedValue`,
`RuntimeValue`, source literals, model/wire values, and future render products.
`Data` is broad; `Datum` is more distinct and could make the direct carried
payload feel like one runtime datum, even if it is structurally compound.

Suggested answer: Use `RuntimeValue::Datum(LpsValueF32)` if the wording feels
acceptable, because it better distinguishes the direct payload variant from
future domains. Treat `ModelValue -> ModelDatum` as a separate, possibly same-
milestone rename only if the mechanical churn is acceptable; otherwise record it
as follow-up and avoid expanding M2.1 too far.
