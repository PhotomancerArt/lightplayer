# Milestone 6: Cleanup + validation + summary

## Goal

Remove debt accumulated during the spine cutover, run the full
validation matrix end-to-end, update design docs to reflect
what shipped, and write `summary.md` for the roadmap with a
pointer to the next one.

## Suggested plan location

`docs/roadmaps/2026-04-28-node-runtime/m6-cleanup-validation/`

Small plan: `plan.md`. Cleanup + validation has enough
distinct items that one round of `/plan-small` iteration
catches gaps.

## Scope

**In scope:**

- **Dead code removal:**
  - The old `NodeRuntime` trait (replaced by `Node` in M5).
  - The old `ProjectRuntime` impl (replaced by the
    `NodeTree`-backed engine in M5).
  - `lpv-model`'s `std`-only one-shot artifact loader
    (replaced by `ArtifactManager` in M4).
  - Compatibility shims left in `lp-model` / `lp-engine` (if
    M2 left any).
- **Doc updates:**
  - `docs/design/lightplayer/domain.md` — reflect the new
    spine.
  - `docs/design/lightplayer/notes.md` — reflect the new
    spine.
  - `AGENTS.md` — update the architecture diagram if it
    needs it.
  - Remove or archive any stale notes referring to old
    runtime shapes.
- **Validation matrix:**
  - `just check` — fmt + clippy host + clippy rv32.
  - `just build-ci` — host + rv32 builtins + emu-guest.
  - `just test` — host workspace + glsl filetests.
  - `cargo check -p fw-esp32 --target
    riscv32imac-unknown-none-elf --profile release-esp32
    --features esp32c6,server`.
  - `cargo check -p fw-emu --target
    riscv32imac-unknown-none-elf --profile release-emu`.
  - `cargo test -p fw-tests --test scene_render_emu --test
    profile_alloc_emu` — real shader compile + execute on
    the emulator.
  - End-to-end `lp-cli` smoke (or whatever the equivalent is
    today) to confirm the protocol surface still works.
- **`summary.md`** in the roadmap directory:
  - What shipped (per milestone).
  - What changed in the codebase shape (the new crate map).
  - Decisions that surfaced during execution
    (cross-reference `decisions.md`).
  - Known limitations entering the next roadmap.
  - **Next steps** — point at the lpfx + lp-vis roadmap,
    which reworks
    `docs/roadmaps/2026-04-23-lp-render-mvp/`.
- **Stub the next roadmap.** A scratch / notes file in
  `docs/roadmaps/<next>-lpfx-vis/` (or whatever name we
  pick) with the open questions inherited from this
  roadmap. Don't write the next roadmap; just hand off.

**Out of scope:**

- New feature work.
- Performance optimisation beyond meeting baseline.
- Anything visual subsystem-related — that's the next
  roadmap, even if obvious.
- Renaming `lpfx` or splitting it into `lpfx-cpu` /
  `lpfx-gpu` (next roadmap).
- Refining the visual model (`lpv-model`) — that's the
  next roadmap; M6 only validates the M2 rename landed
  cleanly.

## Key decisions

- **Validation is non-negotiable.** Every gate runs.
  Cleanup that breaks a gate gets reverted, not waived.
- **`summary.md` is short and scannable.** It exists for
  future-us looking back, not as a marketing doc. Bullet
  list per milestone, links to milestone files, links to
  `decisions.md`.
- **Stubbing the next roadmap is part of this one.** Open
  questions inherited from M3 / M4 / M5 (deferred
  decisions, `lp-vis` design uncertainties, `lpfx` split
  shape) get captured before context fades.

## Deliverables

- All dead code removed; workspace gates green.
- Updated design docs.
- `docs/roadmaps/2026-04-28-node-runtime/summary.md`.
- Stub for the next roadmap with inherited open questions.

## Dependencies

- M5 — cutover must be done before cleanup makes sense.

## Execution strategy

**Option B — small plan (`/plan-small`).**

Justification: Cleanup + validation has enough distinct
items (per-validation gate, per-doc update, per-dead-code
removal, summary, stub) that listing them as a small plan
catches one or two that direct execution would forget.

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?
