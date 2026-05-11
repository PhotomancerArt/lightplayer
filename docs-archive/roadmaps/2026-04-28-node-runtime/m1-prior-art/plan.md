# M1 — Prior-art investigation (research log)

This milestone uses a **research-pass workflow** rather than
plan-small's implementation-phase structure. Research isn't a
sequence of phases — it's iterative
question → answer → synthesize, with optional follow-up passes
to fill gaps.

We may later codify this as its own command (`/research` or
similar) if the workflow holds up. For now, M1 is its
prototype.

## Scope of work

Produce **`docs/roadmaps/2026-04-28-node-runtime/prior-art.md`**:
a focused, scannable document distilling lessons from
**Godot 4**, **Bevy**, **VCV Rack**, **LX Studio**, and
**Three.js** that inform M3's spine design pass.

Each section in `prior-art.md` ties to a specific design call
M3 has to make. Output is judgment-laden — "what to copy /
what to avoid" — but the judgment is *for a system like ours*
(embedded JIT, LED show, `no_std + alloc`, client / server),
**not** vs the specific strawman in `notes.md`. Strawman
synthesis is M3's job.

This is research only. No code touched in this repo.

## Process (per pass)

1. **Construct prompt** — main agent drafts
   `passN/prompt.md`: a self-contained sub-agent prompt with
   Lightplayer context, what makes an answer useful, format
   requirements, and the uniform question set. User reviews
   before dispatch.
2. **Dispatch sub-agents** — one `generalPurpose` sub-agent
   per reference, parallelisable. Each is given the prompt
   path + its assigned reference + output path; reads only
   its assigned codebase; writes `passN/answers-<ref>.md`
   with citations (`(godot:scene/main/node.cpp:L<n>)`).
3. **Review** — main agent reads all `answers-*.md`, captures
   observations in `passN/notes.md`. Identifies gaps,
   contradictions, "this answer needs a spot-check."
4. **Decide** — gaps fillable with another pass? Run pass N+1
   (typically narrower; possibly only some references). No
   gaps? Proceed to synthesis.
5. **Synthesise** — main agent writes
   `prior-art.md` at the roadmap root, "what to copy / what
   to avoid" per section, with citations carried through from
   the per-reference answers.

## Current state of the codebase

- M2 has not started. `lp-domain` / `lp-engine` / `lp-model`
  in pre-roadmap shape. Strawman lives in
  `docs/roadmaps/2026-04-28-node-runtime/notes.md`. M3
  consumes both `prior-art.md` and `notes.md`; M1 stays
  out of strawman synthesis.
- References cloned to `~/dev/photomancer/prior-art/`:
  - `godot/` (2.1G, Godot 4 master) — scene tree, NodePath,
    `Ref<T>` + `Resource`, scene inheritance, property
    reflection.
  - `bevy/` (147M, shallow) — `Asset<T>` + `Handle<T>`,
    `AssetEvent<T>`, asset error model, reflection.
  - `VCVRack/` (39M) — `Module` lifecycle (`onAdd` /
    `onReset` / `process`), parameter API, preset / patch
    save-load.
  - `LX/` (4.7M) — Pattern / Effect / Channel composition,
    modulation routing (≈ our bus).
  - `three.js/` (969M, shallow) — minimalist `Object3D`
    scene graph, manual `dispose()` resource model
    (counter-example to refcount), graph without heavy
    lifecycle hooks.

## Resolved setup decisions

| #  | Decision                                                                        |
| -- | ------------------------------------------------------------------------------- |
| Q1 | Use the seven design surfaces from milestone + two add-ons.                     |
| Q2 | Add **dynamic / composed children** + **error model on missing / malformed**.   |
| Q3 | Output goes at `docs/roadmaps/2026-04-28-node-runtime/prior-art.md`.            |
| Q4 | "What to copy" + "what to avoid" subsections per section.                       |
| Q5 | Citation form `(godot:scene/main/node.cpp:L<n>)`.                               |
| Q6 | Depth: short paragraph + 1–3 cited locations per reference; not engine docs.    |
| Q7 | Bevy + LX Studio promoted to primary references.                                |
| Q8 | Include Three.js (minimalist scene graph counter-example). Skip TD / Unreal / Blender. |
| Q9 | `prior-art.md` is *generic-context* judgment, not vs strawman.                  |

## Pass log

### Pass 1 — broad survey

**Status:** drafting questions.

Files (created during this pass):

- `pass1/prompt.md` — self-contained sub-agent prompt
  (context, format, uniform question set across all references).
- `pass1/answers-godot.md` — sub-agent output.
- `pass1/answers-bevy.md` — sub-agent output.
- `pass1/answers-vcv.md` — sub-agent output.
- `pass1/answers-lx.md` — sub-agent output.
- `pass1/answers-threejs.md` — sub-agent output.
- `pass1/notes.md` — main-agent observations after answers come in.

### Pass 2+

(May or may not exist — depends on Pass 1 gaps.)

## Synthesis output

`docs/roadmaps/2026-04-28-node-runtime/prior-art.md` — written
in the synthesis step after passes complete. "What to copy /
what to avoid" per section, with citations carried through.

## Cleanup

After synthesis:

- Decide whether to keep `pass*/` directories (probably yes,
  they're cheap and useful as raw material if M3 questions a
  citation).
- Validate `prior-art.md` reads top-to-bottom without missing
  citations.
- Single commit at end of milestone.
