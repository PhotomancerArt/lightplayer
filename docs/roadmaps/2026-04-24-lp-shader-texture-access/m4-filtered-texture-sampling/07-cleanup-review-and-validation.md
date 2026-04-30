# Scope of Phase

Perform final cleanup, review, validation, and plan summary for M4 filtered
texture sampling.

This phase should make the completed milestone merge-ready:

- remove temporary code and placeholders;
- ensure generated builtin files are up to date;
- run focused and broader validation;
- check for warnings and lint issues;
- write `summary.md` in this plan directory.

Out of scope:

- Adding new sampling features beyond the agreed M4 scope.
- Expanding the supported format/filter/wrap matrix beyond what earlier phases
  implemented.
- Committing changes. The parent plan process handles the final commit.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from this phase.

# Implementation Details

Review the full diff for:

- temporary placeholders left from the builtin ABI phase;
- debug prints;
- accidental `TODO` comments that should be resolved before merge;
- `unwrap`/`expect` in production paths;
- new `#[allow(...)]` attributes;
- disabled/skipped/weakened tests;
- unrelated refactors or formatting churn.

Generated files:

- Ensure builtin generated files are consistent with the final texture builtin
  externs.
- If generated files are stale, regenerate using the repo's standard command:

```bash
cargo run -p lps-builtins-gen-app
```

or:

```bash
scripts/build-builtins.sh
```

Use whichever command is established by the current branch. Do not hand-edit
generated files.

Docs:

- Update `docs/roadmaps/2026-04-24-lp-shader-texture-access/m4-filtered-texture-sampling.md`
  only if the milestone summary is now stale or contradicted by implementation.
- Do not archive this plan directory; roadmap milestone plans stay in place.

Write:

```text
docs/roadmaps/2026-04-24-lp-shader-texture-access/m4-filtered-texture-sampling/summary.md
```

Use this structure:

```markdown
# Summary

## What was built

- ...

## Decisions for future reference

#### Builtin specialization boundary

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
- **Revisit when:** ...
```

Capture 0-5 decisions. Likely decisions worth recording:

- builtin-first `texture()` path rather than inline LPIR for v0;
- specialize by format + dimensionality, not full format/filter/wrap matrix;
- keep filter/wrap runtime inside sampler builtins;
- `HeightOne` drops Y in frontend lowering while preserving GLSL `sampler2D`;
- linear filetests use tolerances while `texelFetch` remains exact.

# Validate

Run focused validation first:

```bash
cargo test -p lps-builtins texture
cargo test -p lps-frontend texture
cargo test -p lps-filetests textures
```

Then run broader shader pipeline validation appropriate for this repo:

```bash
cargo check -p lpa-server
cargo test -p lpa-server --no-run
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
```

If time allows before final commit/push, run the repo CI gate:

```bash
rustup update nightly
just check
just build-ci
just test
```

At minimum, report exactly which commands passed and which were not run.
