# Phase 6: Cleanup, Docs, And Validation

sub-agent: supervised
model: gpt-5.5
parallel: -

## Scope of phase

Perform final cleanup, formatting, validation, and plan summary for M2.1.

In scope:

- Remove stray temporary code, debug prints, stale comments, and obsolete TODOs
  introduced by this plan.
- Run formatting and validation.
- Update plan docs if implementation deviated from `00-design.md`.
- Add `summary.md` in this plan directory.

Out of scope:

- Do not add new runtime domains.
- Do not implement real render products.
- Do not design texture wire transport.
- Do not port legacy runtimes.
- Do not commit.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place public types and impls near the top; helpers below them.
- Place tests at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a `TODO` comment so it can be found later.

## Sub-agent Reminders

- Do not commit. The plan commits at the end as a single unit.
- Do not expand scope. Stay strictly within this phase.
- Do not suppress warnings or add `#[allow(...)]`; fix warnings.
- Do not disable, skip, or weaken existing tests.
- If blocked or ambiguous, stop and report instead of improvising.
- Report back: files changed, validation run, result, and deviations.

## Implementation Details

Check the diff for:

- `todo!`, `unimplemented!`, `dbg!`, `println!`, `eprintln!`
- newly added `TODO` comments
- commented-out code
- stale `ProducedValue` references that should now be `Production`
- stale `RuntimeValue` references that should now be `RuntimeProduct`
- comments implying texture/render products live in `ModelValue`

Useful searches:

```bash
rg "todo!|unimplemented!|dbg!|println!|eprintln!|ProducedValue|RuntimeValue|Texture2D" \
  docs/roadmaps/2026-05-01-runtime-core/m2.1-runtime-value-domains \
  lp-core/lpc-engine/src lp-core/lpc-model/src lp-core/lpc-source/src
```

Only remove/adjust hits that are part of this plan or now stale because of this
plan. Do not remove unrelated historical TODOs.

Run formatting:

```bash
cargo +nightly fmt
```

Run focused validation:

```bash
cargo test -p lpc-model -p lpc-source -p lpc-engine
```

Because this plan touches `lp-core`, also run the required embedded check if the
focused host tests pass:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

If the embedded check fails for an obvious local issue introduced by this plan,
fix it. If it fails for an existing unrelated toolchain/dependency issue, report
the failure with the relevant error.

Create:

```text
docs/roadmaps/2026-05-01-runtime-core/m2.1-runtime-value-domains/summary.md
```

Use this format:

```markdown
### What was built

- ...

### Decisions for future reference

#### Runtime Products Carry Domains

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
- **Revisit when:** ...
```

Capture only decisions future readers might relitigate:

- `Production` as the resolver/cache/provenance envelope.
- `RuntimeProduct::{Value, Render}` as the domain/product enum.
- Render products are handles to engine-managed storage, not payloads in the
  resolver cache.
- Texture wire transport is future work and should separate references from
  pixel payloads.

## Validate

Run:

```bash
cargo +nightly fmt
cargo test -p lpc-model -p lpc-source -p lpc-engine
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```
