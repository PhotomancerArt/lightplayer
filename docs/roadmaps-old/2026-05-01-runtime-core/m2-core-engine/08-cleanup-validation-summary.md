## Scope of Phase

Clean up the M2 implementation, run validation, update docs, and write the plan
summary.

This is the final phase. It should not introduce new architecture beyond small
fixes needed to make the implemented design coherent and validated.

Out of scope:

- Do not add new engine features.
- Do not port legacy runtimes.
- Do not add UI/wire sync.
- Do not archive this roadmap plan; roadmap plans stay in place.

Suggested sub-agent model: `gpt-5.5`.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public items and entry points near the top, helpers below them, and
  `#[cfg(test)] mod tests` at the bottom of Rust files.
- Keep related functionality grouped together.
- Remove temporary code unless it is a deliberate future marker.
- Any remaining TODO must be justified and actionable.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back what changed, what was validated, and any deviations from this
  phase plan.

## Implementation Details

Cleanup:

1. Review the git diff for temporary code, debug prints, accidental TODOs, and
   drive-by refactors.
2. Remove stale docs that still describe `NodeEntry` as owning `ResolverCache`
   for the new runtime path.
3. Update `lp-core/lpc-engine/README.md` so it briefly describes:
   - `Engine`
   - `BindingRegistry`
   - `Resolver` / `ResolveSession` / `ResolveTrace`
   - dummy validation slice only if useful
4. Ensure `docs/roadmaps/2026-05-01-runtime-core/m2-core-engine/future.md` still
   contains only real future work.

Plan cleanup:

Create `docs/roadmaps/2026-05-01-runtime-core/m2-core-engine/summary.md`.

It must include:

```markdown
### What was built

- ...

### Decisions for future reference

#### ...

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
- **Revisit when:** ...
```

Capture only notable decisions. Good candidates for this plan:

- `Bus` became `BindingRegistry` while keeping bus nomenclature.
- `ResolveTrace` unifies cycle detection and value-provenance tracing.
- M2 used dummy core nodes instead of adapting concrete legacy runtimes.
- `ResolveSession` is the active per-frame/per-demand object.

Do not move this plan directory. It is under `docs/roadmaps/...`, so it stays in
place.

## Validate

Run formatting:

```bash
cargo +nightly fmt
```

Run targeted tests:

```bash
cargo test -p lpc-engine
```

Because this plan touches `lp-core/`, also run the ESP32 check required by the
workspace rules:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If time permits or before pushing, run the normal CI gate:

```bash
just check
```

If any command cannot be run in the environment, report that clearly and include
the blocker.
