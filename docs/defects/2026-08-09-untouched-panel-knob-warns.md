---
status: fixed
found: 2026-08-09      # how: Yona, opening the gallery ("something I wanted to fix for a while")
fixed: pending
area: lpc-engine shader/compute shader nodes (input resolve status)
class: state-conflation
related:
  - 2026-08-04-unbound-shader-uniform-warns.md
  - 2026-08-03-wasm-shader-instances-share-vmctx.md
---
# An untouched panel knob warns for being untouched

**Symptom** — six of the gallery examples opened onto a permanent Warning
badge on their shader card:

```
Warn("input \"glow\" using its default: no bus provider for channel ChannelName(\"glow\")")
```

`fire2012` (`reach`, `sparks`), `plasma` and `plasma-duo` (`scale`), `comet`
(`tail`), `palette-waves` (`depth`, `span`), `meteor` (`decay`; and its
compute node's `count`, `speed`). Nothing was wrong with any of them. Every
one of those inputs is the standard panel-knob idiom: the shader declares a
value uniform bound to a bus channel so the panel derives a control for it
(`panel.md` P1), and until someone touches that control nothing writes the
channel, so the uniform runs on the slot's authored default.

That is the *designed* state, and every other surface said so. The panel's
own control for the same channel reads `UiPanelControlState::ReadDefault` —
documented in place as "an unfilled public input is an invitation, not an
error", quoting `modules.md` R6. The card next to it said "Warning".

**Root cause** — `Warn` was again carrying two different facts, one namespace
over from the 2026-08-04 entry. That fix taught the engine host to answer an
*unbound* uniform's `ConsumedSlot` query with the authored default, so an
unbound uniform stopped warning. A uniform **bound to a channel nobody
writes** still took the error path: `resolve_or_default_input` recorded every
`Err` from `ctx.resolve` as an input resolve failure, and
`SessionResolveError::NoBusProvider` is an `Err` like any other.

So the surviving conflation was *bound but unwritten* (normal — the knob at
rest) against *bound and broken* (a project defect — ambiguous writers, a
dangling target, a cycle, a writer whose value the uniform cannot hold). The
first is the overwhelmingly common case in any project with a panel, which is
what made the second unreadable.

Worth noting where the resolver already had this right: `ResolveSession`
falls back quietly for a consumed slot whose binding the loader *materialized
from `default_bind`* (R6, at `BindingPriority::default_fallback`), and both
the `phasor` and `palette` paths ask `consumed_slot_bus_provenance` first,
which returns `None` when no scope in the chain writes the channel — so
neither of those kinds ever warned for this. Only the value path, and only
for an authored binding, was loud. That is why the examples split the way
they did: `rise`/`sweep`/`phase` (phasor, and `default_bind`-declared) were
quiet while `reach`/`scale`/`tail` (value, authored binding only) warned —
the same wiring, two tones, decided by which code path read it.

**Fix** — `unwritten_channel_at_rest` in `shader_node.rs`: a `NoBusProvider`
failure on a `Value`-kind input is not recorded as a failure at all. Every
other failure shape still is, and every other kind still is — a `map` or
`buffer` has no authored scalar to fall back on (the empty fill is
materialization's invention, not intent), so a typo'd channel name on one of
those keeps reporting.

**Known cost** — a typo'd channel name on a *value* input is now silent. The
truer rule would have been "quiet only when the slot declares its own
`default`", which would have kept that case loud, but that distinction does
not survive to where the decision is made:
`sync_optional_value_from_authored` reads an absent authored `default` option
as `0.0` and *creates* the runtime option, so by tick time every value slot
carries one. Diagnosis for the typo now comes from the panel — the control
lists under a channel name nobody recognizes, reading "default". Making it
loud again means giving the runtime slot an authored-vs-materialized
distinction first.

**Regression coverage** —
`no_gallery_example_opens_onto_a_warning_badge` (`studio_face_e2e_tests.rs`)
walks every embedded package's node tree through a real `LpServer` and fails
on any Warning tone: the product-level statement, in the same harness that
made the defect visible. Under it,
`unwritten_channel_on_a_defaulted_input_stays_quiet` and
`a_dangling_input_binding_still_reports_warning_status`
(`compute_shader_node.rs`) pin the two halves end to end, and
`only_a_value_input_rests_on_an_unwritten_channel` (`shader_node.rs`) pins
the policy itself, including the `map`/`buffer` kinds whose defs are too
array-shaped to drive through a whole compute node.

**Lesson** — the 2026-08-04 entry closes with "a warning that fires for the
normal case is worse than no warning", and that is exactly what this is: the
same lesson, surviving one namespace over, five days later, on the six
packages that are the product's front door. Fixing the instance is not fixing
the conflation: that entry's own regression tests both assert the *unbound*
case, so the badge the gallery actually wore had nothing holding it — and
`unresolvable_bound_input_reports_warning_status`, sitting right beside them,
asserted the surviving false positive was correct.

The generalizable part is the check: when two surfaces render the same runtime
condition, they must agree on whether it is a problem. The panel had already
named this state `ReadDefault` and written the ruling down in the enum's own
doc comment. A status tone that disagrees with a state name for the same
condition is a defect in one of them, and finding out which is a five-minute
read — cheaper than the eight months.
