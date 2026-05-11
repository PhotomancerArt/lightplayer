# Phase 5: Cleanup, Summary, and Validation

## Scope of phase

Clean up the M3a implementation, run broader validation, and write the milestone summary.

In scope:

- Remove stray TODOs, debug prints, scratch code, commented-out experiments, and unused imports.
- Verify no M3b data-path code snuck into M3a.
- Run formatting and validation commands.
- Add `summary.md` to this plan directory with what was built and future decisions.

Out of scope:

- Do not implement missing functionality from prior phases unless it is a small obvious fix.
- Do not add new texture sampling features.
- Do not commit.
- Do not push.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope of phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If validation fails for a non-obvious reason, stop and report rather than debugging broadly.
- Report back what changed, what was validated, and any deviations from this phase plan.

## Implementation Details

Read:

- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3a-texture-aware-lowering-contract/00-notes.md`
- `docs/roadmaps/2026-04-24-lp-shader-texture-access/m3a-texture-aware-lowering-contract/00-design.md`
- The git diff for all files changed by this plan.

### Cleanup checks

Inspect the diff for:

- `TODO`
- `todo!`
- `unimplemented!`
- `dbg!`
- `println!` or `eprintln!` debug output
- commented-out code
- new `#[allow(...)]`
- `unwrap`/`expect` in production paths
- any `Load16U`, `Unorm16toF`, row-stride/address math, or vec4 fill logic that belongs to M3b

Do not remove legitimate pre-existing TODOs outside the touched diff.

### Summary

Create:

`docs/roadmaps/2026-04-24-lp-shader-texture-access/m3a-texture-aware-lowering-contract/summary.md`

Use this exact shape:

```md
### What was built

- <one-line concrete change>
- <one-line concrete change>

### Decisions for future reference

#### <short title>

- **Decision:** <one line>
- **Why:** <one or two lines>
- **Rejected alternatives:** <one line>
- **Revisit when:** <optional>
```

Capture decisions worth remembering, especially:

- `lower_with_options`/`LowerOptions` rather than `lower_with_texture_specs`.
- M3a intentionally stops at a placeholder diagnostic for otherwise valid `texelFetch`.
- Direct uniform texture operands only for v0.

Keep it terse; do not restate the whole design.

## Validate

Run:

```bash
cargo +nightly fmt
cargo test -p lps-shared -p lps-frontend
cargo test -p lp-shader -p lps-filetests
```

Then run the required shader-pipeline firmware check from project rules:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If a command is too slow or blocked by missing local tooling/artifacts, report the exact blocker. Do not silently skip validation.

