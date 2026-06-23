# Architecture Decision Records

Architecture Decision Records, or ADRs, capture durable architecture and process
decisions for this repo.

Use ADRs for decisions that choose a direction among plausible alternatives and
have lasting architectural, operational, security, data-model, API, workflow,
product, embedded, or cross-repo/process consequences.

Do not create ADRs for ordinary feature work, bug fixes, UI copy/layout
changes, mechanical refactors, tests, scripts, helpers, or phase sequencing
unless they set a broader precedent.

## Filename

Use date-based filenames:

```text
YYYY-MM-DD-short-title.md
```

Date-based names keep files sortable and reduce conflicts between parallel
branches.

## Status

Use one of:

- `Proposed`
- `Accepted`
- `Superseded`
- `Rejected`

Treat ADRs as durable history. If a decision changes, create a new ADR that
supersedes the old one instead of rewriting old context heavily.

## Relationship To Shared Planning

Plans, roadmap-level plans, reviews, reports, scratch notes, and phase prompts
live in the personal planning workspace configured by `PHOTOMANCER_PLANNING_ROOT`
or `~/.photomancer/planning`.

Only durable decisions graduate into `docs/adr/`.
