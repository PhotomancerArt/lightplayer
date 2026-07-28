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
fixed: <commit>        # absent while open. NOTE: an entry cannot cite
                       # its OWN commit (the hash doesn't exist yet, and
                       # amending changes it) — write `fixed: this change`
                       # at commit time and fill the real hash in the NEXT
                       # commit that touches the registry.
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
- **`inline-emit-stack-imbalance`** — a code-emitter leaves the operand
  stack unbalanced, and a downstream construct hides it from validation.
- **`untested-path`** — a variant of a fixed bug survives in a sibling
  code path the fix and its tests never reached.
- **`stale-measurement`** — a cached measurement outlives its validity
  because the events that invalidate it aren't all observed.

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
| backend-contract-divergence | 2026-07-27 | [created-package-unloadable](2026-07-27-created-package-unloadable.md) | fixed | lpa-studio-core/library |
| lifecycle-ownership | 2026-07-16 | [browser-serial-endpoint-lost](2026-07-16-browser-serial-endpoint-lost.md) | fixed | lpa-link/registry |
| lifecycle-ownership | 2026-07-22 | [flash-session-map-deleted](2026-07-22-flash-session-map-deleted.md) | fixed | lpa-link/browser-serial |
| state-conflation | 2026-07-17 | [unreadable-masqueraded-as-empty](2026-07-17-unreadable-masqueraded-as-empty.md) | fixed | lpa-studio-core/roster |
| state-conflation | 2026-07-22 | [read-failure-vs-unreadable-content](2026-07-22-read-failure-vs-unreadable-content.md) | **open** | lpa-studio-core/roster |
| state-conflation | 2026-07-26 | [worker-poisoned-instance-reuse](2026-07-26-worker-poisoned-instance-reuse.md) | fixed | fw-browser + lpa-link/browser-worker |
| assumed-context | 2026-07-17 | [storage-slot-assumed](2026-07-17-storage-slot-assumed.md) | fixed | lpa-studio-core/places |
| assumed-context | 2026-07-23 | [deploy-dialog-ignores-running-project](2026-07-23-deploy-dialog-ignores-running-project.md) | fixed | lpa-studio-core/device |
| assumed-context | 2026-07-27 | [launch-json-pinned-port](2026-07-27-launch-json-pinned-port.md) | fixed | dev tooling (launch.json + dev-port.sh) |
| partial-knowledge-loss | 2026-07-22 | [identity-lost-on-failed-read](2026-07-22-identity-lost-on-failed-read.md) | fixed | lpa-studio-core/places+studio |
| partial-knowledge-loss | 2026-07-23 | [reconnect-transient-twin-card](2026-07-23-reconnect-transient-twin-card.md) | fixed | lpa-studio-core/home + device |
| policy-leak | 2026-07-17 | [hardware-attach-opened-editor](2026-07-17-hardware-attach-opened-editor.md) | fixed | lpa-studio-core/studio |
| stand-in-divergence | 2026-07-23 | [popover-open-resizes-card](2026-07-23-popover-open-resizes-card.md) | fixed | lpa-studio-web/base/popover |
| stand-in-divergence | 2026-07-27 | [story-check-tolerance-ignores-amplitude](2026-07-27-story-check-tolerance-ignores-amplitude.md) | **open** | lpa-studio-web/scripts + CI |
| nondeterministic-capture | 2026-07-28 | [overview-composite-capture-races](2026-07-28-overview-composite-capture-races.md) | **open** | lpa-studio-web story capture (overview composites) |
| retired-surface-still-reachable | 2026-07-28 | [retired-device-pane-still-reachable](2026-07-28-retired-device-pane-still-reachable.md) | **open** | lpa-studio-core/home + studio_shell |
| stale-measurement | 2026-07-26 | [popover-outline-stale-on-content-resize](2026-07-26-popover-outline-stale-on-content-resize.md) | fixed | lpa-studio-web/base/popover |
| stale-measurement | 2026-07-27 | [code-editor-gutter-misaligned](2026-07-27-code-editor-gutter-misaligned.md) | **open** | lpa-studio-web/base/code_editor |
| inline-emit-stack-imbalance | 2026-07-27 | [wasm-q32-fabs-stack-leak](2026-07-27-wasm-q32-fabs-stack-leak.md) | fixed | lpvm-wasm emit (+ lpvm-cranelift trunc) |
| untested-path | 2026-07-27 | [cranelift-q32-floor-ceil](2026-07-27-cranelift-q32-floor-ceil.md) | fixed | lpvm-cranelift q32_emit (rv32c) |

## Predecessor: `docs/bugs/`

Two ad-hoc pre-registry writeups live in `docs/bugs/` (2026-03 JIT
filetest segfault, cranelift rv32 ld instruction). They stay where they
are as historical record; new entries belong here.
