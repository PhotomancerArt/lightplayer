# Milestone 1: Prior-art investigation

## Goal

Study existing node / scene / artifact systems (Godot 4 + VCV Rack
primarily) and produce a focused `prior-art.md` that informs the
M3 spine design pass. Scope of study is narrow: the specific design
surface we're building, not a comprehensive engine review.

This is **research only**. No code changes in this repo.

## Suggested plan location

`docs/roadmaps/2026-04-28-node-runtime/m1-prior-art/`

Small plan: `plan.md`.

## Scope

**In scope:**

- **Clone references to a sibling untracked folder.** Target:
  `~/dev/reference/godot/`, `~/dev/reference/vcv-rack/` (or
  similar). Untracked from this repo.
- **Read-only investigation** of the following design surfaces in
  each system:
  - Lifecycle hooks (Godot's `_ready` / `_process` /
    `_exit_tree`; VCV's `Module::onAdd` / `onRemove` /
    `onReset` / `process`).
  - Node tree teardown order (parent-first or child-first;
    deferred deletion semantics).
  - Path grammar (Godot's `NodePath` `$Player/Camera`;
    VCV's module/port addressing).
  - Resource / asset refcounting (Godot's `Ref<T>` / `Resource`;
    VCV's preset/patch model).
  - Scene / patch instantiation (Godot's `PackedScene` →
    `Node` tree; VCV's patch loading).
  - Change / dirty tracking and editor sync (Godot's
    `_notification(NOTIFICATION_DIRTY)`; VCV's parameter
    automation).
  - Property reflection (Godot's `Object::get` / `set` /
    `_get_property_list`; VCV's parameter API).
- **Optional secondary references** if a question lacks a clear
  answer from Godot / VCV: TouchDesigner docs (closed-source
  but documented), Bevy ECS (asset/refcount story),
  Unreal UObject GC (probably overkill, but instructive on
  what *not* to do at our scale).
- **`prior-art.md` output** in the roadmap directory. Sections
  per design surface; "what to copy" + "what to avoid" for
  each.

**Out of scope:**

- Comprehensive engine documentation; we want answers to *our*
  questions, not a generic essay.
- Code changes in this repo; M3 is where designs land.
- Trait sketches or strawman APIs; those go in M3.
- Prior art for the *visual subsystem* (Pattern / Effect /
  Stack semantics); that's an lp-vis concern, not this
  roadmap's.
- Prior art for shader compilation, lpvm, or wgpu — those
  systems are well-understood already.

## Key decisions

- **Focused, not comprehensive.** Each section in `prior-art.md`
  exists because we have a specific design call to make in M3.
  If a section can't be tied to one of those calls, it
  doesn't belong.
- **Two primary references; expand only if needed.** Godot 4
  has the closest analogous architecture (scene tree +
  Resource refcount + change events); VCV Rack is small and
  legible enough to read end-to-end. Adding more references
  costs time without proportional clarity.
- **References live outside the repo.** `~/dev/reference/` (or
  equivalent untracked sibling). Don't pollute the workspace
  with vendored copies.

## Deliverables

- `~/dev/reference/godot/` and `~/dev/reference/vcv-rack/`
  cloned (untracked from this repo).
- `docs/roadmaps/2026-04-28-node-runtime/prior-art.md` —
  short, scannable, with clear "what to copy / what to avoid"
  per section.
- Specific cross-references in the form
  `(godot:scene/main/node.cpp:line)` or
  `(vcv-rack:src/Module.cpp:line)` so future-us can verify
  claims.

## Dependencies

- None. M1 runs in parallel with M2.
- Blocks: M3 (spine design pass) consumes `prior-art.md`.

## Execution strategy

**Option B — small plan (`/plan-small`).**

Justification: Research milestone with a clear single output
(`prior-art.md`), but the scope deserves planning — which
sections, which references per section, what depth per question.
A direct-execution path risks producing either too-broad or
too-narrow research. One round of small-plan question iteration
nails the section list and keeps the milestone focused.

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?
