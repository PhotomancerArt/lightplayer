# Phase 5: Audit Prop And Param Domain Boundaries

sub-agent: supervised
model: gpt-5.5
parallel: -

## Scope of phase

Audit the downstream assumptions created by the new domain/product split and
make only the narrow code or documentation changes needed to keep the boundary
clear.

In scope:

- Review `RuntimePropAccess` and decide whether it remains a data-only/legacy
  bridge or needs a small new-engine-facing adjustment.
- Review source params/defaults and document that they materialize into runtime
  products through the engine, not directly into one universal shader value.
- Review `Kind::Texture`, `SrcValueSpec::Texture`, `ResolvedSlot`, and older
  resolver paths for misleading comments or new scalar-only assumptions.
- Update docs/comments/tests where needed to reflect
  `Production` / `RuntimeProduct`.

Out of scope:

- Do not redesign source loading.
- Do not port legacy runtimes.
- Do not migrate all old resolver caches to `RuntimeProduct` unless required by
  compilation or by the new engine path.
- Do not implement texture wire transport.
- Do not add new domains beyond `Value` and `Render`.

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

Start by reading:

```text
docs/roadmaps/2026-05-01-runtime-core/m2.1-runtime-value-domains/00-notes.md
docs/roadmaps/2026-05-01-runtime-core/m2.1-runtime-value-domains/00-design.md
lp-core/lpc-engine/src/prop/runtime_prop_access.rs
lp-core/lpc-source/src/prop/src_value_spec.rs
lp-core/lpc-model/src/prop/kind.rs
lp-core/lpc-engine/src/resolver/resolved_slot.rs
lp-core/lpc-engine/src/resolver/resolver.rs
```

Audit points:

1. `RuntimePropAccess`
   - It currently exposes `(LpsValueF32, FrameId)`.
   - If it is still used only as a data/legacy bridge after the previous phases,
     update docs/comments to say so.
   - If the new engine path now needs products from props, stop and report
     instead of redesigning the trait.

2. Source defaults and params
   - `SrcValueSpec::Literal(ModelValue)` and `SrcValueSpec::Texture` are authored
     specs, not runtime products.
   - Comments should not imply params directly map to `LpsValueF32` forever.

3. `Kind::Texture`
   - Preserve `Kind::Texture` as semantic meaning.
   - Preserve the struct storage recipe if still used.
   - Comments should distinguish portable storage/authoring recipes from
     runtime render products.

4. Legacy resolver paths
   - `ResolvedSlot` may remain `LpsValueF32`.
   - Comments should identify it as old/slot-cache/data path if that prevents
     confusion.

5. Public exports/readme
   - Update `lpc-engine/README.md` or module docs only if stale terms make the
     new architecture confusing.

Keep changes small. This phase is an audit and boundary clarification, not a
new implementation phase.

Suggested tests:

- Only add tests if code behavior changes.
- If docs/comments only change, validation is enough.

## Validate

Run:

```bash
cargo test -p lpc-model -p lpc-source -p lpc-engine
```
