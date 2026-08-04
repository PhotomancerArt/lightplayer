---
status: fixed          # 2026-08-03 — shape-driven registration (fix direction 2)
found: 2026-08-02      # how: P10 walk prep — authoring a consuming bus binding that never took effect
area: lpc-engine/project_loader (authored binding registration)
class: silent-drop
related:
  - docs/design/panel.md
  - docs/design/modules.md
  - docs/adr/2026-08-02-panel-writers-and-state-persistence.md
---
# An authored `source` binding on a slot the loader doesn't enumerate is dropped without a word

**Symptom** — this clock authors a consuming binding, loads without error,
and behaves as if the binding were not there:

```json
{ "kind": "Clock",
  "bindings": { "controls.rate": { "source": "bus:rate" } },
  "controls": { "rate": 1.0 } }
```

The binding graph after load contains only the clock's default
`seconds → bus:time` publish. No `rate` channel lists, no consuming
binding exists, no warning is logged, and the project reports healthy.

**Cause** — `ProjectLoader` registers authored `source` bindings by
*calling out slot names per node kind*: `register_optional_source_binding`
is invoked for `input` on a fixture, for each key of `consumed_slots` on a
shader/compute-shader, for `time`/`emitters` on other kinds. Any other key
in the `bindings` map is simply never looked up. `binding_source()` is a
map GET — a key nobody asks for is a key nobody misses.

**Why it matters beyond the typo case** — it silently bounds which
controls can be panel controls at all. A panel control is a channel
presentation (panel.md P1), so a slot that cannot carry a consuming bus
binding can never be driven by a panel writer. Concretely today:

- **Shader uniforms can** — every `consumed_slots` key is enumerated, so
  `"bindings": { "glow": { "source": "bus:glow" } }` works.
- **Fixture `brightness` cannot** — only `input` is enumerated. That is
  the scarf scenario's own control (panel.md P10), so the motivating
  example of the whole panel-persistence design is currently unreachable
  except through a shader uniform standing in for it.

No shipped example authors such a binding, so nothing is *broken* today;
what is broken is the feedback when you try.

**Fix directions** (not attempted here — out of P10's scope)

1. **Loudest, cheapest:** after registering, diff the authored `bindings`
   map against the keys actually consumed and log/report the leftovers.
   A binding naming an unknown slot should surface as a node issue, the
   way a bad slot path does.
2. **Structural:** drive registration from the def's slot shape
   (which slots are `consumed`) instead of a hand-listed name set per
   kind, so any consumed slot is bindable by construction. This is the
   direction modules.md implies — publicity is a property of the slot,
   not of a list in the loader.

Either fix makes fixture brightness bindable, which is what lets the
panel own the control the design keeps using as its example.

**Fixed 2026-08-03** — fix direction 2, in
`register_node_bindings`/`resolve_declared_binding_path`
(lp-core/lpc-engine/src/engine/project_loader.rs): every authored
bindings key resolves against the kind's declared def/state shapes (plus
shader/compute dynamic slots), with two refinements the fix surfaced:

- **Option-wrapped slots normalize to their `.some` interior** — binding
  lookup is exact-path and the runtime's accessor reads
  `brightness.some`, so a target registered at `brightness` would have
  been a second silent drop one layer deeper.
- **Unknown keys fail the load loudly** (`InvalidProjectReference`,
  naming the slot) on every closed-namespace kind. Shader/compute kinds
  are exempt: their slot namespace is open and a binding may legally
  arrive before its consumed record — the declared-orphan state the
  agent's `upsert_param` repair flow depends on (pinned by the agent
  e2es). The studio's uniform surface owns that feedback.

The fix flushed out dead weight the drop had been hiding: texture nodes
carried a `bus:visual.out` → `input` binding (test fixtures and the
shared project builder) for a slot `TextureDef` never declared. Fixture
brightness is now not merely bindable but default-bound
(`bus:brightness`, ADR 2026-08-03-panel-visibility-is-derived amendment),
and clock `controls.rate` — the report's own example — registers, both
pinned by loader regression tests.

**Met again 2026-08-03** (modules vision push, P5 content). Authoring
`"brightness": { "source": "bus:brightness" }` on `examples/meteor`'s
fixture, to give that example a second public control, registered
nothing: no `brightness` row in the module's wiring drawer, no fader on
the panel, no diagnostic. The binding was removed and the example ships
with one control instead of two. This is now the second time the defect
has been met by someone trying to do exactly what the design documents
use as their worked example, which is the argument for fix direction 2.
