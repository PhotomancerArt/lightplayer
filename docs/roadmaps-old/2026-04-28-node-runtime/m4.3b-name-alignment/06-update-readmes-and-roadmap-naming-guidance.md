# Phase 6 — Update READMEs and Roadmap Naming Guidance

sub-agent: yes
parallel: -

# Scope of phase

Document the M4.3b naming policy in the relevant crate READMEs and active
node-runtime roadmap/design docs.

Out of scope:

- Code renames beyond fixing stale docs examples.
- Editing archived plan docs unless they are actively misleading current
  work.
- Large prose rewrites unrelated to naming boundaries.

# Code organization reminders

- Keep README additions concise and scannable.
- Put naming guidance near crate-role/boundary sections.
- Use current canonical type names from the completed previous phases.
- Avoid duplicating a giant policy block verbatim everywhere; tailor each
  README to the crate.

# Sub-agent reminders

- Do not commit.
- Do not expand scope into product naming or app-crate naming.
- Do not add new roadmap decisions beyond the accepted M4.3b design.
- Report changed files, validation commands/results, and deviations.

# Implementation details

Read `00-notes.md` and `00-design.md` in this directory first.

Update these READMEs if present:

- `lp-core/README.md`
- `lp-core/lpc-model/README.md`
- `lp-core/lpc-source/README.md`
- `lp-core/lpc-wire/README.md`
- `lp-core/lpc-view/README.md`
- `lp-core/lpc-engine/README.md`
- `lp-core/lpc-shared/README.md` only if it has boundary text that now
  needs clarification.

README guidance should capture:

- `lpc-model`: foundational shared nouns are unprefixed; portable
  structural representations use `Model*`; no `Wire*` types live here.
- `lpc-source`: exported authored/source schema concepts use `Src*`; no
  short root aliases for source names.
- `lpc-wire`: message/request/response names imply wire; use `Wire*` for
  disambiguating nouns such as tree deltas/status/specifiers/indices.
- `lpc-view`: local cache/view structures use natural `*View` suffixes;
  reserve `Client*` for real client/app abstractions.
- `lpc-engine`: natural engine runtime nouns stay unprefixed unless
  ambiguous; conversion helpers should name both sides of the boundary.

Update active roadmap/design docs under:

- `docs/roadmaps/2026-04-28-node-runtime/design/`
- `docs/roadmaps/2026-04-28-node-runtime/m4.3-runtime-spine/`
- `docs/roadmaps/2026-04-28-node-runtime/m4.4-domain-sync/`
- this `m4.3b-name-alignment/` plan directory if previous phase results
  changed exact names.

Search targets:

```bash
rg "WireValue|WireType|WireStructMember|NodeSpecifier|NodeProps|ApiNodeSpecifier|SlotIdx|ClientProjectView|ClientNodeTree|WirePropAccess|WirePropsMap" docs lp-core/*.md lp-core/*/README.md
```

Expected result:

- Current READMEs explain the naming policy.
- Active design docs use canonical post-M4.3b names.
- Archived/historical docs may mention old names only when clearly
  historical.

# Validate

Run:

```bash
cargo +nightly fmt
cargo check -p lpc-model -p lpc-source -p lpc-wire -p lpc-view -p lpc-engine
```
