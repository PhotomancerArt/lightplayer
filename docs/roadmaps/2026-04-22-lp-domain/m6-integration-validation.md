# Milestone 6: Integration validation + cleanup

## Goal

Tie everything together: validate the full pipeline (load examples,
run all CI gates, verify `no_std` builds), write the README that lives
at the front door of `lp-domain`, link the roadmap into the design
docs, and decide what (if anything) to delete from `lp-core/lp-model`
that's been superseded.

## Suggested plan name

`lp-domain-m6`

## Scope

**In scope:**

- `lp-domain/lp-domain/README.md`:
  - What this crate is and isn't.
  - Crate layout (`src/`, `examples/`, `schemas/`).
  - Feature flags (`std`, `schema-gen`).
  - Linked workflow walkthroughs:
    - "Add a new Visual kind"
    - "Bump a schema version"
    - "Generate schemas locally"
    - "Run the compat gates locally"
- Update `docs/design/lpfx/overview.md` and
  `docs/design/lpfx/concepts.md` to point at `lp-domain` as the
  canonical home for the typed model + examples.
- Update `docs/design/lightplayer/domain.md` similarly where it talks
  about implementation home.
- Verify all gates pass on a clean checkout:
  - `cargo build --workspace`
  - `cargo test --workspace`
  - `cargo build -p lp-domain --no-default-features` (no_std + alloc)
  - `cargo build -p lp-domain --features schema-gen`
  - `lp-cli schema generate --check`
  - `lp-cli migrate --all --dry-run` (should be a no-op)
  - All CI gates (1–6) green
  - ESP32-C6 firmware build still passes
- Decide what in `lp-core/lp-model` is superseded vs still useful;
  delete the superseded parts or mark them with a comment pointing at
  `lp-domain`. Do **not** rewrite `lp-engine` or anything else to use
  `lp-domain` types yet — that's Q8 sequential next-step work, out of
  scope for this milestone.
- Tag a snapshot commit so future "frozen old parser" CI gates have
  a stable reference.

**Out of scope:**

- Rewriting `lp-engine` / `lp-server` / `lpfx` to consume `lp-domain`
  types. That is the next roadmap; this one is about delivering a
  trustworthy domain crate.
- Render hookup of any kind.
- Artifact resolution beyond what M3's loader stub does
  (file-relative, no library registry, no caching). Tracked in the
  artifact-resolution future-roadmap.

## Key decisions

- **No parallel coexistence** with `lp-core/lp-model` (per Q8). The
  cleanup step is "delete what's clearly superseded" — typically
  just things that were placeholders.
- **Tagged baseline** for the forward-compat CI gate — this milestone
  declares the lp-domain-v1.0 baseline.

## Deliverables

- `lp-domain/lp-domain/README.md`
- Updated cross-references in design docs
- All-green run of every gate listed above
- Cleanup of superseded `lp-model` bits (if any)
- Git tag: `lp-domain-v1.0` (or similar baseline marker)

## Dependencies

- M5 complete (gates 1–6 wired and known good in isolation).

## Execution strategy

**Option B — Small plan (`/plan-small`).**

Justification: Verification + README + cross-reference cleanup; no
architectural design surface, but the verification command list and
"what to do if a gate trips" need to be tracked in a single plan
file rather than improvised. Real risk is what shakes out when every
gate runs together for the first time, and the plan file gives us a
spot to capture surprises and the fixes for them.

Estimated size: ~200–300 line README, small cross-reference updates,
modest `lp-model` cleanup (size depends on what's still alive),
verification commands listed in the plan.

> I suggest we use the `/plan-small` process for this milestone, after
> which I will automatically implement. Agree?
