# Resolver And Binding Rename

## Scope of phase

Rename resolver and binding concepts from input/output namespace language to
consumed/produced slot language.

In scope:

- Rename `QueryKey::NodeInput` to `ConsumedSlot`.
- Rename `QueryKey::NodeOutput` to `ProducedSlot`.
- Rename binding endpoint variants to produced source / consumed target
  language.
- Remove produced-slot binding targets unless a current use requires a named
  replacement.
- Treat bindings as slot-level relationships. Do not introduce sub-value
  binding semantics in this plan.
- Update resolver trace, cache, errors, test support, and rustdocs.

Out of scope:

- Changing resolver behavior beyond names and produced access integration.
- Final binding string syntax.
- Generic wire/view redesign.

## Code organization reminders

- Prefer semantic names over compatibility names.
- Keep resolver docs clear that consumed resolution goes through bindings and
  produced resolution asks the owner node to produce/read its slot.
- Avoid adding namespace checks that recreate `params`/`outputs` as semantic
  direction.

## Sub-agent reminders

- Do not commit.
- Do not expand scope.
- Do not suppress warnings or weaken tests to get green builds.
- If blocked, stop and report instead of improvising.
- Report what changed, what was validated, and any deviations.

## Implementation details

Relevant files:

- `lp-core/lpc-engine/src/resolver/query_key.rs`
- `lp-core/lpc-engine/src/resolver/resolve_session.rs`
- `lp-core/lpc-engine/src/resolver/production.rs`
- `lp-core/lpc-engine/src/resolver/resolve_error.rs`
- `lp-core/lpc-engine/src/resolver/resolve_trace.rs`
- `lp-core/lpc-engine/src/resolver/resolver_cache.rs`
- `lp-core/lpc-engine/src/binding/binding_entry.rs`
- `lp-core/lpc-engine/src/binding/binding_registry.rs`
- `lp-core/lpc-engine/src/engine/test_support.rs`
- `lp-core/lpc-engine/src/project_runtime/*.rs`

Expected changes:

- New resolver/binding names should compile across the engine.
- Tests should read in consumed/produced terms.
- If transitional resolver keys still carry a `ValuePath`, document that as
  current implementation shape rather than final sub-value binding semantics.
- If bus channels still use `ChannelName`, keep that as a compatibility name
  unless replacing it with `SlotName` is small and clear.

## Validate

```bash
cargo test -p lpc-engine
```
