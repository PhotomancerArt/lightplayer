# Phase 5: Host and workspace regression

## Scope of phase

Prove **host** workflows (CLI, tests, `lp-server` with **`std`**) still work after embedded-focused
changes. Align with **`justfile`** / CI expectations where practical.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. Run **`cargo check`** / **`cargo test`** for representative packages: **`lp-server`**, *
   *`lp-client`**, **`lps-filetests`** (or a lighter subset if full run is heavy).
2. Fix **feature unification** issues (e.g. **`std` + `glsl`** both on for host defaults).
3. Update **docs** (roadmap stage VI-A pointer, this plan **`00-notes`**) if behavior changed.

## Tests to write

- Rely on existing suites; add only if a **regression** was found and needs a **minimal** guard.

## Validate

```bash
cargo check -p lp-server
cargo test -p lp-server --no-run
# Optional depth (timeboxed):
# cargo test -p lps-filetests --no-run
# just clippy (or workspace clippy excluding cross-only crates per repo convention)
```

Fix warnings introduced or newly exposed.
