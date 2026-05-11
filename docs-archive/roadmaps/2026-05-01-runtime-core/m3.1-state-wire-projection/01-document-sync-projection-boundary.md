# Phase 1: Document SyncProjection Boundary

## Scope of Phase

Align the M3.1 roadmap docs around the agreed `SyncProjection` vocabulary.

In scope:

- Update the milestone overview to use `SyncProjection` language.
- Ensure `00-notes.md` and `00-design.md` consistently describe:
  - `SyncProjection`: given a client frame id and watch/detail interests, project
    current state into a client-usable form;
  - `LegacySyncProjection`: the current compatibility path that emits legacy
    `GetChanges` payloads;
  - heavy byte fields as compatibility snapshots, not authoritative runtime
    storage.

Out of scope:

- Code changes.
- New wire payload types.
- New runtime product/buffer storage.
- Phase files for other phases.

## Code Organization Reminders

- Keep docs concise and grep-friendly.
- Prefer the roadmap's existing heading style.
- Avoid introducing new terminology beyond `SyncProjection`,
  `LegacySyncProjection`, `snapshot`, and `product/buffer`.
- Any temporary note should be marked with a `TODO`, but avoid temporary notes if
  possible.

## Sub-agent Reminders

This phase is tagged `sub-agent: main`; the main agent should execute it
directly.

- Do not commit.
- Do not expand scope.
- Do not touch code.
- Do not rewrite unrelated roadmap content.

## Implementation Details

Files:

- `docs/roadmaps/2026-05-01-runtime-core/m3.1-state-wire-projection.md`
- `docs/roadmaps/2026-05-01-runtime-core/m3.1-state-wire-projection/00-notes.md`
- `docs/roadmaps/2026-05-01-runtime-core/m3.1-state-wire-projection/00-design.md`

Update the milestone overview from generic "state wire projection" wording to
the accepted `SyncProjection` framing:

- The milestone goal should say this hardens the sync projection path before M4.
- The context should say current legacy sync projects state/config/status through
  `ProjectResponse::GetChanges`.
- The key decision should prefer `SyncProjection` / `LegacySyncProjection`.

Keep the already-recorded Q1 answer in `00-notes.md`.

## Validate

No code validation is required. After editing, run:

```bash
git diff -- docs/roadmaps/2026-05-01-runtime-core/m3.1-state-wire-projection.md docs/roadmaps/2026-05-01-runtime-core/m3.1-state-wire-projection/00-notes.md docs/roadmaps/2026-05-01-runtime-core/m3.1-state-wire-projection/00-design.md
```
