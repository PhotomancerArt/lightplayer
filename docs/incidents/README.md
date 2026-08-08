# Incident registry

Blameless post-mortems for production impact. The third registry,
completing the set:

- **`docs/defects/`** — a *mechanism* failed: the system did Y when it
  should have done Z. Filed at fix time, mechanism-shaped.
- **`docs/debt/`** — a *condition* stands: a known gap we are living
  with, with a trigger for paying it down.
- **`docs/incidents/`** — an *event* hurt a deployed surface: real
  users, real data, or the live service felt it. Filed after recovery,
  process-shaped.

An incident needs no defect: the 2026-08-08 uid incident shipped code
that did exactly what it was designed to do — the failure lived in the
process around the change. Conversely, most defects never become
incidents. When both exist, they cross-link.

## The filing bar

File an incident when a change or operation **impacted a deployed
surface**: the live cloud service, fielded devices, or real user data
(including our own production library — dogfood data is user data).
Near-misses that were one step from impact qualify at the author's
discretion; test/CI breakage never does.

Small still counts. A ten-minute self-inflicted outage with one affected
user is a cheap rehearsal for the one with a hundred.

## Severity

Grade **by fraction and function, as if at scale** — headcount belongs
in the Impact paragraph, never in the grade. "All existing projects
unopenable for 100% of users" is the same severity at two users as at
two million; a tiny userbase is a *mitigating fact about impact*, not a
*discount on severity*, and grading it down would teach us nothing for
the day the userbase isn't tiny.

- **critical** — data loss or corruption, a security/privacy breach, or
  the product wholly unusable (no core function at all) for a large
  fraction of users; any irreversible harm is automatically critical.
- **severe** — a core workflow broken or existing content unavailable
  for a large fraction of users, with no workaround or only
  destructive/manual ones. Duration doesn't change the grade; it's
  reported separately.
- **minor** — degraded or annoying with a reasonable workaround, or a
  broken edge affecting a small fraction.

When torn between two grades, **grade up** — the opposite of review
findings, deliberately: a review finding graded down communicates merge
risk honestly; an incident graded up buys the rehearsal for the bigger
version of itself.

## Blameless, agentic edition

The classic rule: name conditions, not actors — "the merge raced the
in-flight upgrader," never "X merged too early." Causes are things that
would have caught the problem regardless of who was driving.

For a team of agents the rule sharpens into something operational:
**agent "carelessness" is never a cause, because agents don't carry
lessons — standing instructions do.** An agent session ends; what
persists is AGENTS.md, CI gates, corpus fixtures, skills, ADR deferred
rows, and memory. So every contributing cause must be phrased as the
*missing standing thing* ("no rule distinguished persisted bytes from
wire bytes"), and every action item must name its **enforcement
surface** — the standing thing that now exists. An action item whose
only artifact is a paragraph in the incident doc is a smell: future
agents load AGENTS.md and the gates; they do not re-read old
post-mortems.

## Running a post-mortem

Write it while context is hot — same day, ideally by the session that
drove the response (it holds the whole timeline). The steps:

1. **Reconstruct the timeline** from artifacts, not memory: PR
   `mergedAt` timestamps, CI run times, commit dates, session logs.
   Human observations ("user reported X") stay approximate; machine
   events get exact UTC times.
2. **State impact honestly**: who/what, for how long, and what remedy
   users had meanwhile (including "none").
3. **List contributing causes** — plural, systemic, blameless as above.
   Ask of each: "what standing instruction or gate, had it existed,
   would have caught this with a different driver?"
4. **Record what went well and where we got lucky** — luck is a cause
   that hasn't happened yet; each "lucky" line is a candidate cause of
   the next incident.
5. **File action items, each with its enforcement surface**, and mark
   the ones already done during the response. Open items get a home
   (ADR deferred table, debt file, chip) — never only this doc.
6. Add the entry to the index below and land it by PR — review of the
   post-mortem IS the ratification of its causes and actions.

## Entry template

```markdown
---
status: actions-open    # actions-open | closed
date: YYYY-MM-DD        # when impact began
severity: severe        # minor | severe | critical (see Severity above)
duration: <impact window, humans-first ("5h 10m")>
related: []             # defects, debt, ADRs, PRs
---
# <one-line title, past tense, impact-first>

**Impact** — who/what/how long, and the remedy users had meanwhile.
**Timeline** — UTC, artifact-anchored; human events approximate.
**Contributing causes** — numbered, systemic, blameless.
**What went well / where we got lucky** — both lists, honestly.
**Action items** — checkbox list; every line names its enforcement
surface; done-during-response items stay listed, checked.
```

## Index

| Date | Incident | Severity | Status |
|---|---|---|---|
| 2026-08-08 | [Uid format change broke every deployed project](2026-08-08-uid-format-broke-prod-projects.md) | severe | closed |
