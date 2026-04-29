# node-runtime — Decisions

Architectural calls made *before* the roadmap began executing.
Anchors for milestone files; grep targets when scope creeps.

## D-1 — Refactor `lp-core` in place (Path R)

Rejected the alternative of building a parallel runtime spine
in `lpfx` and migrating `lp-engine` later. `lp-core`'s client /
server architecture, frame-versioning, fs-watch routing, and
panic recovery are load-bearing and well-tuned; rebuilding (or
porting "later") wastes the existing investment. Instead, the
new spine concepts (node tree, slots, artifact manager) land
*inside* `lpc-runtime`, with the existing machinery generalised
to support them.

## D-2 — Per-domain model + runtime crate split

Each subsystem (foundation, legacy, future visual, future rig,
…) gets its own model crate and its own runtime crate. Crate
boundaries — not module boundaries — express domain
ownership. Justification: an embedded codebase targeting
ESP32 + browser + host + emulator needs crate-level deps and
per-target feature gating. Module-only splits force every
consumer to compile every domain.

## D-3 — `lpfx` becomes the rendering abstraction only; visual subsystem is `lp-vis`

`lpfx` was conceptually doing two unrelated jobs: pure
rendering abstraction (CPU vs GPU shader execution) and visual
domain (Pattern / Effect / Stack semantics). They have
different consumers and different release cycles. Split into:
**`lpfx`** = backend-agnostic shader interface + per-backend
crates (`lpfx-cpu`, `lpfx-gpu`); **`lp-vis`** = visual
artifacts and instances. Both deferred to the next roadmap;
this roadmap only sets up the architectural slot.

## D-4 — `lp-domain` is dismantled in M2: foundation → `lpc-model`, visual → `lpv-model`

The foundation half of `lp-domain` (`Slot`, `Kind`, `Constraint`,
`ValueSpec`, `Binding`, `Presentation`, identity / addressing
types, `Artifact` + `Migration` traits) is generic and belongs
in `lpc-model`. The visual half (`Pattern`, `Effect`, `Stack`,
`Transition`, `Live`, `Playlist`) is what *makes* `lp-domain`
the visual model — once foundation moves out, calling the
remainder `lp-domain` is a misleading name. M2 renames it to
`lpv-model` (under a new `lp-vis/` parent) so every workspace
crate matches the `lp{x}-` prefix convention. The next roadmap
adds `lpv-runtime` alongside; standalone `lpv-model` between
roadmaps is fine — `lpc-model` and `lpl-model` exist standalone
during parts of M2 too, and there's no visual *runtime* code
anywhere yet to be torn between locations.

## D-5 — Two-letter subsystem prefix convention (`lpc-`, `lpl-`, `lpv-`, `lpfx-`)

The workspace already uses two-letter prefixes for `lp-shader`
(`lps-*`); generalise. `lpc-*` for foundation (lp-core),
`lpl-*` for legacy (lp-legacy), `lpv-*` for visual (lp-vis),
`lpfx-*` for the rendering abstraction. Keeps the "this is
ours" namespace recognisable, narrows the search space when
naming a new crate, and avoids the overgrown `lp-` namespace.

## D-6 — One `Slot` type, four namespaces (`params` / `inputs` / `outputs` / `state`)

A `Node` has four slot namespaces, but a `Slot` is a single
type across all of them. The namespace is expressed by the
*view*, not the slot's type. `params` are named + bus-bindable;
`inputs` are indexed for structural composition; `outputs` are
indexed (usually one); `state` is sidecar runtime state
(named, not authored). This keeps the type system small and
forces the per-namespace semantics into views, which compose
better than four parallel slot types would.

## D-7 — Filetest harness for the spine deferred to the next roadmap

Filetest's real value is CPU↔GPU correctness / perf comparison,
which this roadmap can't deliver because `lpfx` isn't split
yet. Validating the spine via filetest in this roadmap would
duplicate work that legacy node port (M5) already does
end-to-end. Filetest harness lands in the lpfx + lp-vis
roadmap where the comparison is the point.

## D-8 — Implementation order: artifacts (class) before nodes (instance); legacy port, no bridge

M4 implements `ArtifactManager` + slot views + TOML loader in
isolation; the running engine is untouched. M5 then implements
`Node` trait + `NodeTree` + lifecycle on top, and ports legacy
nodes directly into the new shape — no parallel-runtime
bridge. The cutover *is* the validation: if a legacy node
can't be expressed cleanly under the new shape, the trait
surface changes (or M3's `design.md` is updated and the plan
re-iterated).
