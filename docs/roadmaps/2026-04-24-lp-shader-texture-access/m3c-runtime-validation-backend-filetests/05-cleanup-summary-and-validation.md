# Scope Of Phase

Clean up M3c, write the plan summary, and run final validation.

In scope:

- Remove temporary comments, TODOs, debug prints, or scratch code introduced by
  this plan.
- Confirm the guest `LpsTexture2DDescriptor` ABI remains four lanes:
  `ptr`, `width`, `height`, `row_stride`.
- Confirm texture runtime validation uses the typed host texture value and does
  not reintroduce raw `UVec4` descriptor writes as the public binding path.
- Write `summary.md` in this plan directory.
- Run focused validation for touched crates and texture filetests.

Out of scope:

- Committing changes.
- Moving this roadmap plan directory.
- Implementing new texture formats.
- Emulator ISA-profile gating.

# Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

# Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within "Scope Of Phase".
- Do not suppress warnings or `#[allow(...)]` problems away; fix them.
- Do not disable, skip, or weaken existing tests to make the build pass.
- If something blocks completion, stop and report back rather than improvising.
- Report back: what changed, what was validated, and any deviations from the
  phase plan.

# Implementation Details

Search the final diff for:

- `TODO`
- `dbg!`
- `println!`
- `eprintln!`
- `unimplemented!`
- `todo!`
- raw texture descriptor stand-ins such as `UVec4` being accepted for
  `Texture2D`
- commented-out code

Do not remove unrelated pre-existing TODOs outside the M3c diff.

Write:

`docs/roadmaps/2026-04-24-lp-shader-texture-access/m3c-runtime-validation-backend-filetests/summary.md`

Use this shape:

```markdown
### What was built

- ...

### Decisions for future reference

#### Host Texture Value vs Guest Descriptor ABI

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
- **Revisit when:** ...

#### Format-Specific Texture Layout Alignment

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
- **Revisit when:** ...
```

Keep the summary terse and update bullets to match the actual diff.

# Validate

Run:

```bash
cargo fmt --check
cargo test -p lps-shared
cargo test -p lpvm
cargo test -p lp-shader
cargo test -p lps-filetests
TEST_FILE=textures cargo test -p lps-filetests --test filetests filetests -- --ignored --nocapture
```

If any command cannot be run locally, record exactly what was not run and why in
the phase report.
