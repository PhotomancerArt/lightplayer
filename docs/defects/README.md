# Defect registry

A durable record of defects worth remembering. ADRs record decisions;
defects record failures. Where an ADR captures "we chose X among
plausible alternatives," a defect entry captures "the system did Y when
it should have done Z, and here is the mechanism" — so the same
mechanism is recognized the next time it dresses up in a different
symptom.

Entries live in this directory, one dated file each:
`YYYY-MM-DD-slug.md`, dated by when the defect was **found**.

## The filing bar

File a defect when at least one of these holds:

- It **reached a user or a hardware walk** — someone observed the
  failure outside a test run.
- It **revealed a contract or model gap** — the bug is evidence that
  two components disagree about an interface, or that the domain model
  conflates things it shouldn't.
- It **produced (or should have produced) a regression test** — if the
  fix deserved a named test, the failure deserves a record; if coverage
  was impossible, that gap is itself worth recording.
- The **lesson outlives the fix** — the entry would change how someone
  writes the next feature, not just how they read this diff.

Fix-forward trivialities — typos, off-by-ones caught in review, build
breakage — stay commit messages. The registry is for defects whose
*shape* recurs.

Write the entry **at fix time, riding the fix commit**: the same change
that fixes a qualifying bug adds its entry (and updates the index
below). `status: open` entries are legal and expected for
found-not-yet-fixed defects — hardware-walk and live-debugging findings
get a home immediately, before anyone decides when to fix them.

## Entry template

```markdown
---
status: fixed          # open | fixed | wontfix
found: YYYY-MM-DD      # how: hardware-walk | live-debugging | ci | e2e | report
fixed: <commit>        # absent while open
area: <crate/module>
class: <one from the vocabulary>
related: []            # other defects, ADRs, plan dirs
---
# <one-line title>

**Symptom** — what was observed, verbatim error text included.
**Root cause** — the mechanism, not the patch.
**Fix** — what changed and where (the commit is the diff; this is the shape).
**Regression coverage** — named tests, or "none: <why>".
**Lesson** — one paragraph; what this implies beyond the fix.
```

## Class vocabulary

Every entry carries a `class` — the failure's mechanism, not its
surface. The vocabulary is extensible: add a class when a defect
genuinely fits none of these, and define it here in one line.

- **`backend-contract-divergence`** — two implementations of one
  contract disagree on details only real hardware surfaces.
- **`lifecycle-ownership`** — two layers both believe they own a
  resource's lifecycle.
- **`partial-knowledge-loss`** — an error path discards facts already
  learned.
- **`policy-leak`** — one context's policy applied in another.
- **`assumed-context`** — code presumes state instead of asking the
  source of truth.
- **`state-conflation`** — one state models two different facts.
- **`stand-in-divergence`** — a stand-in (placeholder, mock, fallback)
  meant to be equivalent to what it replaces diverges in a dimension the
  substitution didn't model.

## Index

Grouped by class, because a class that keeps recurring is the
model-smell signal: one `backend-contract-divergence` is a bug, two in
a week is an argument for a conformance suite. When a class accumulates
entries, say so out loud — that is an architecture finding, not a
bookkeeping fact.

| Class | Date | Entry | Status | Area |
| --- | --- | --- | --- | --- |
| backend-contract-divergence | 2026-07-17 | [deletedir-error-shape](2026-07-17-deletedir-error-shape.md) | fixed | lpa-server + lpa-client |
| backend-contract-divergence | 2026-07-22 | [littlefs-listdir-doubled](2026-07-22-littlefs-listdir-doubled.md) | fixed | fw-esp32/fs |
| lifecycle-ownership | 2026-07-16 | [browser-serial-endpoint-lost](2026-07-16-browser-serial-endpoint-lost.md) | fixed | lpa-link/registry |
| lifecycle-ownership | 2026-07-22 | [flash-session-map-deleted](2026-07-22-flash-session-map-deleted.md) | fixed | lpa-link/browser-serial |
| state-conflation | 2026-07-17 | [unreadable-masqueraded-as-empty](2026-07-17-unreadable-masqueraded-as-empty.md) | fixed | lpa-studio-core/roster |
| state-conflation | 2026-07-22 | [read-failure-vs-unreadable-content](2026-07-22-read-failure-vs-unreadable-content.md) | **open** | lpa-studio-core/roster |
| assumed-context | 2026-07-17 | [storage-slot-assumed](2026-07-17-storage-slot-assumed.md) | fixed | lpa-studio-core/places |
| assumed-context | 2026-07-23 | [deploy-dialog-ignores-running-project](2026-07-23-deploy-dialog-ignores-running-project.md) | fixed | lpa-studio-core/device |
| partial-knowledge-loss | 2026-07-22 | [identity-lost-on-failed-read](2026-07-22-identity-lost-on-failed-read.md) | fixed | lpa-studio-core/places+studio |
| partial-knowledge-loss | 2026-07-23 | [reconnect-transient-twin-card](2026-07-23-reconnect-transient-twin-card.md) | fixed | lpa-studio-core/home + device |
| policy-leak | 2026-07-17 | [hardware-attach-opened-editor](2026-07-17-hardware-attach-opened-editor.md) | fixed | lpa-studio-core/studio |
| stand-in-divergence | 2026-07-23 | [popover-open-resizes-card](2026-07-23-popover-open-resizes-card.md) | fixed | lpa-studio-web/base/popover |

## Predecessor: `docs/bugs/`

Two ad-hoc pre-registry writeups live in `docs/bugs/` (2026-03 JIT
filetest segfault, cranelift rv32 ld instruction). They stay where they
are as historical record; new entries belong here.
