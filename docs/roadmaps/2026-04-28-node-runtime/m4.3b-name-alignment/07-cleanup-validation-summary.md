# Phase 7 — Cleanup, Validation, and Summary

sub-agent: yes
parallel: -

# Scope of phase

Do the final cleanup pass for M4.3b:

- Search for stale names from pre-M4.3b APIs.
- Fix formatting.
- Run validation.
- Write `summary.md` in this plan directory.

Out of scope:

- New renames not already covered by phases 1-6.
- Behavior changes.
- Archiving the plan or committing. The main agent will handle commit.

# Code organization reminders

- Do not add new compatibility aliases.
- Do not add temporary TODOs, debug prints, commented-out code, or stubs.
- Keep summary terse and decision-focused.

# Sub-agent reminders

- Do not commit.
- Do not push.
- Do not suppress warnings or weaken tests.
- If validation fails for a non-trivial reason, stop and report rather
  than debugging deeply.
- Report changed files, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md`, `00-design.md`, and phase files `01-*.md` through
`06-*.md` first.

Search the active code/docs for stale names:

```bash
rg "WireValue|WireType|WireStructMember|wire_value|wire_type|lps_value_f32_to_wire_value|wire_type_to_lps_type" lp-core lp-app docs/roadmaps/2026-04-28-node-runtime
rg "NodeSpecifier|NodeProps|lpc_model::nodes|pub mod nodes|mod nodes" lp-core lp-app docs/roadmaps/2026-04-28-node-runtime
rg "ApiNodeSpecifier|SlotIdx" lp-core lp-app docs/roadmaps/2026-04-28-node-runtime
rg "ClientProjectView|ClientNodeEntry|ClientNodeTree|ClientTreeEntry|WirePropAccess|WirePropsMap" lp-core/lpc-view lp-app docs/roadmaps/2026-04-28-node-runtime
```

Historical mentions in older completed plan docs are acceptable only if
they are clearly historical. Active design docs and READMEs should use
current names.

Write `docs/roadmaps/2026-04-28-node-runtime/m4.3b-name-alignment/summary.md`
with this format:

```markdown
### What was built

- <one-line bullet per concrete change>

### Decisions for future reference

#### Naming policy by crate

- **Decision:** <one line>
- **Why:** <one or two lines>
- **Rejected alternatives:** <brief alternatives>
- **Revisit when:** <condition, if any>
```

Record only decisions that future agents might relitigate, such as:

- `ModelValue`/`ModelType` belong in `lpc-model`, not `WireValue`.
- `Src*`, selective `Wire*`, and natural `*View` suffix naming.
- No compatibility aliases in shared roots.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-model -p lpc-source -p lpc-wire -p lpc-view -p lpc-engine
cargo test -p lpc-model
cargo test -p lpc-source
cargo test -p lpc-wire
cargo test -p lpc-view
```
