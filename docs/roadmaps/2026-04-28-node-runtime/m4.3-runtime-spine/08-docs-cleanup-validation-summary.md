# Phase 8 — Docs, Cleanup, Validation, Summary

sub-agent: supervised
parallel: -

# Scope of phase

Final cleanup for M4.3:

- Update active roadmap docs to reflect what landed.
- Search for stale pre-M4.3 names in active code/docs.
- Remove accidental TODOs, debug prints, commented-out scratch code, and
  unused stubs.
- Run validation.
- Write `summary.md`.

Out of scope:

- Do not implement new runtime behavior beyond small cleanup fixes.
- Do not archive this roadmap-backed plan.
- Do not push or commit.

# Code organization reminders

- Keep docs concise and accurate.
- Do not leave temporary code.
- Do not add compatibility aliases.
- Prefer fixing warnings over suppressing them.

# Sub-agent reminders

- Do not commit.
- Do not push.
- Do not suppress warnings.
- Do not weaken tests.
- If validation fails for a non-trivial reason, stop and report.
- Report files changed, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md`, `00-design.md`, and phase files 01-07 first.

Update active docs as needed:

- `docs/roadmaps/2026-04-28-node-runtime/m4.3-runtime-spine/plan.md`
- `docs/roadmaps/2026-04-28-node-runtime/m4.4-domain-sync/plan.md`
- `docs/roadmaps/2026-04-28-node-runtime/m5-node-spine-cutover.md`
- relevant `docs/roadmaps/2026-04-28-node-runtime/design/*.md` if they are
  now misleading
- `lp-core/lpc-engine/README.md` if new public modules should be mentioned

Search active code for stale names:

```bash
rg "NodeRuntime" lp-core/lpc-engine/src --glob '*.rs'
rg "lpc-runtime" docs/roadmaps/2026-04-28-node-runtime/m4.3-runtime-spine docs/roadmaps/2026-04-28-node-runtime/m4.4-domain-sync docs/roadmaps/2026-04-28-node-runtime/m5-node-spine-cutover
rg "PropAccess\\b|WireValue|WireType|ArtifactSpec\\b|NodeConfig\\b" docs/roadmaps/2026-04-28-node-runtime/m4.3-runtime-spine lp-core/lpc-engine/src --glob '*.{md,rs}'
```

Interpret results carefully:

- `LegacyNodeRuntime` is expected.
- `RuntimePropAccess` is expected.
- `SrcArtifactSpec` and `SrcNodeConfig` are expected.
- Historical docs can mention old terms if clearly historical, but active
  M4.3 design/phase docs should use current names.

Search the diff for shortcuts:

```bash
git diff --check
rg "todo!\\(|unimplemented!\\(|dbg!\\(|println!" lp-core/lpc-engine/src lp-core/lpc-engine/tests
rg "#\\[allow\\(" lp-core/lpc-engine/src lp-core/lpc-engine/tests
```

Write:

`docs/roadmaps/2026-04-28-node-runtime/m4.3-runtime-spine/summary.md`

Format:

```markdown
### What was built

- ...

### Decisions for future reference

#### <decision>

- **Decision:** ...
- **Why:** ...
- **Rejected alternatives:** ...
- **Revisit when:** ...
```

Likely decisions to record:

- M4.3 stages the new engine spine side-by-side; M5 cuts over.
- Artifact manager is generic/closure-loaded without `ProjectDomain`.
- M4.3 owns engine-side `NodeProp` dereference; M4.4 owns wire/view prop
  mirroring.
- New `node/` contracts stay separate from legacy `nodes/`.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-engine -p lpc-source
cargo test -p lpc-engine
cargo test -p lpc-source
```
