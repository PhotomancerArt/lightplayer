---
status: fixed
found: 2026-08-04      # how: live-debugging (TimeProduct M2 P3, pinned not fixed)
fixed: this change
area: lpc-engine engine host (EngineResolveHost) + shader/compute shader nodes
class: state-conflation
related:
  - 2026-07-28-playlist-entry-selection.md
  - 2026-08-03-wasm-shader-instances-share-vmctx.md
---
# An unbound shader uniform warns for behaving exactly as authored

**Symptom** — a shader or compute-shader node with a uniform nothing binds
reported a permanent runtime status:

```
Warn("input \"t\" using its default: unresolved consumed slot \"t\" on NodeId(3)")
```

Nothing was wrong. The uniform had no binding, so it ran on its authored
default — the designed behaviour for an unbound uniform. The value was
correct; only the status lied. Every project with an unbound uniform wore
the warning from the first frame, forever — and it was user-visible, not
just internal: `project_node_status_view` maps `Warn` to a "Warning" badge
with the message. `examples/plasma` (`scale`, `speed`), `examples/meteor`,
and `examples/fyeah-*` all have uniforms nothing binds.

**Root cause** — two namespaces addressed through one lookup.
`EngineResolveHost::produce_consumed_slot` answers an unbound
`QueryKey::ConsumedSlot` by calling `read_authored_def_product(node, slot)`
with `slot` = the **uniform name** (`time`, `speed`). That runs
`lookup_slot_data_and_shape` against the node's `NodeDef` record shape —
and `ShaderDef`/`ComputeShaderDef` have no field per uniform. Uniforms live
in the `consumed` map, keyed by name. So the lookup could never hit, for
any uniform, on any shader: it failed with `UnresolvedConsumedSlot` 100% of
the time.

`resolve_or_default_input` (shader_node.rs) then recorded that `Err` as an
input resolve failure, and `runtime_status()` turned it into `Warn`. Its
doc comment asserted the opposite — "an unbound slot resolves `Ok` through
the authored-default production, so any `Err` here means a genuinely broken
binding" — which was a claim about a host projection that did not exist.

The conflation is what makes this worth a registry entry, not the missing
match arm. `Warn` was carrying two different facts: *this node has no
binding here* (normal, the overwhelmingly common case) and *this node's
binding is broken* (a project defect — no bus provider, ambiguous
providers, dangling target, cycle). The second is the one the warning was
built for, and it was unreadable because it looked exactly like the first.

The engine already knew about the second namespace: the merge-policy read
right beside it, `read_shader_consumed_slot_merge_policy`, special-cases
shader defs to reach `consumed[<name>]`. Only the *value* half was missing.

**Fix** — `read_shader_consumed_slot_default` in `engine.rs`, a value-side
twin of the merge-policy projection (both now share
`shader_consumed_slot_def`). `produce_consumed_slot` consults it before the
plain def lookup and produces the authored default: a value slot's `default`
(0.0 when unauthored), an empty map for a map slot. Those are the two shapes
`materialize_shader_input` already collapses absent data to, so every
uniform materializes to the same `LpsValueF32` it did before — no uniform's
*value* changed, only the spurious `Err` disappeared. The
`resolve_or_default_input` doc comment now names the host projection its
claim depends on instead of asserting it.

Only the plain `ConsumedSlot` path needs this. A `SlotAccessor` is compiled
against the def's own shape, so a uniform name can never become one.

**Regression coverage** —
`unbound_uniform_runs_on_its_authored_default_without_warning`, one per node
kind (`shader_node.rs`, `compute_shader_node.rs`), each asserting both
halves: the `ConsumedSlot` query resolves `Ok` to the authored default, and
`runtime_status()` is `None`. They sit next to the negative tests
(`unresolvable_bound_input_reports_warning_status`,
`ambiguous_bus_providers_report_warning_status`) that they give meaning to.
Two tests because the projection keys off the `NodeDef` variant and
`ComputeShader` alone would leave the `Shader` arm unpinned.

Also outstanding: the TimeProduct M2 branches (`claude/timeproduct-m2-*`,
commit 284da1e52) carry
`an_unbound_uniform_warns_today_even_though_it_runs_on_its_authored_default`
in `lp-core/lpc-engine/src/engine/shader_timebase_tests.rs`, which pins the
*old* behaviour on purpose and whose panic message says to invert it when
this is fixed. That file does not exist on main, so it must be inverted
where it lives, at merge.

**Lesson** — a warning that fires for the normal case is worse than no
warning: it does not degrade to noise, it actively hides the signal it was
built for. Two of this node's tests assert that a broken binding surfaces as
`Warn` and both passed the whole time, because a test that asserts *a
warning appears* is only half a contract. The other half — nothing warns
when nothing is wrong — is the half that catches a diagnostic drowning in
its own false positives, and it is the half nobody writes.

The mechanism generalizes past shaders. Whenever a query is keyed in one
namespace (uniform names) and answered by a lookup in another (record
fields), the failure surfaces worded as the *subject's* fault
("unresolved consumed slot `t`") rather than as "I looked in the wrong
place", so it reads as a finding about the project rather than a bug in the
resolver. That wording is why this survived: it was a plausible-looking
diagnostic. When a lookup can only ever succeed for some node kinds, the
kinds it cannot serve need an explicit arm, not an error path — the error
path will be indistinguishable from a real one.
