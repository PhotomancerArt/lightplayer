# Scope of Phase

Final cleanup and validation for the texture interface foundation milestone.

In scope:

- Review the full diff for scope creep and temporary code.
- Remove debug prints, accidental TODOs, unused helpers, or dead imports.
- Run formatting and validation.
- Add `summary.md` for future reference.

Out of scope:

- New texture sampling behavior.
- New lpfx/domain integration.
- Large refactors not required to make this milestone clean.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of Phase".
- Do not suppress warnings or allow-list problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from this
  phase plan.

# Implementation Details

Review the milestone against:

- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m1-texture-interface-foundation.md`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m1-texture-interface-foundation/00-notes.md`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m1-texture-interface-foundation/00-design.md`

Search the current diff for temporary or risky code:

- `TODO`
- `dbg!`
- `println!`
- `eprintln!`
- `unwrap()` or `expect()` in production code
- new lint allow attributes
- disabled, skipped, or weakened tests

Do not remove intentional existing TODOs outside this plan unless the current
diff introduced them.

Add `summary.md` in the plan directory with these sections:

- `What was built`
- `Decisions for future reference`

Capture only decisions that are useful later and not already obvious from code.
Likely candidates:

- `Texture2D` is logical metadata while the descriptor is ABI.
- `compile_px` remains as a compatibility wrapper around the descriptor API.
- Normal scalar uniform writes do not expose texture descriptor fields.

# Validate

Run from the workspace root:

```bash
cargo fmt --all -- --check
cargo test -p lps-shared
cargo test -p lps-frontend
cargo test -p lp-shader
cargo test -p lpvm
cargo check -p lps-shared
cargo check -p lps-frontend
cargo check -p lp-shader
cargo check -p lpvm
```

If time and toolchain availability allow, also run:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

