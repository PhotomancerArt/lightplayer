# ADR: Contributor License Agreement for relicensing-safe contributions

- **Status:** Accepted
- **Date:** 2026-07-31
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Related:** `2026-07-29-license-provenance-discipline.md` (discharges its
  "formalize the CLA / DCO-with-grant mechanism" follow-up)

## Context

LightPlayer is `AGPL-3.0-or-later` by choice, with the deliberate option to
dual-license — AGPL for open-source use, proprietary terms for commercial
integrations. The license-provenance ADR preserved that option for *derived*
code; this ADR closes the other exposure: *inbound contributions*.

Today the option is safe by default — every one of the repo's ~2,700 commits
is by the maintainer, so relicensing requires no one else's consent. The first
outside contribution changes that permanently: code merged without an explicit
relicensing grant can never be sold under commercial terms without tracking
down its author, and that consent is cheap to collect at submission time and
expensive-to-impossible to reconstruct later. A concrete commercial
integration discussion (Digital Ambiance's Light Path orchestration system)
makes this no longer theoretical, and outside code contributions are expected.

Until now, `CONTRIBUTING.md` carried lightweight contribution terms: by
submitting, a contributor implicitly agreed to a relicensing grant. That is
inbound=outbound plus a grant, but agreed to only *implicitly* — nobody signs
anything, so the evidentiary record is weak, and the provenance ADR itself
recorded formalization as an open follow-up.

## Decision

**1. Grant-style Individual CLA, adapted from the Apache ICLA.** Outside
contributors sign the LightPlayer Individual Contributor License Agreement
(`docs/cla/individual-cla.md`) once, before their first PR merges. Its
substance:

- perpetual, worldwide, irrevocable, royalty-free copyright license including
  sublicensing **and an explicit right to relicense under any terms,
  including proprietary and commercial terms** (the clause the dual-licensing
  option rests on);
- an Apache-style patent grant with defensive termination;
- the contributor **retains ownership** — this is a license, not an
  assignment;
- a reciprocal commitment: every contribution also remains available under an
  OSI-approved license (currently AGPL-3.0-or-later), so the grant cannot be
  used to take contributions fully closed;
- representations covering originality, employer rights, and third-party
  material disclosure, tied to the provenance ADR's rules.

**2. Corporate CLA variant for employer-owned work.**
`docs/cla/corporate-cla.md` mirrors the individual agreement but is executed
by the employing entity with a schedule of designated contributors. Work made
for hire is owned by the employer, so an individual signature alone is not
sufficient for it — this is the most common way relicensing chains break, and
the individual CLA's representations route employed contributors here.

**3. Mechanical enforcement via CLA Assistant.**
`.github/workflows/cla.yml` runs `contributor-assistant/github-action` on
every PR: first-time contributors are prompted with the CLA, sign by posting
an affirmative comment, and the signature (GitHub account + timestamp) is
recorded in `signatures/v1/cla.json` on the `cla-signatures` branch. The
check blocks merge until signed. The maintainer and bots are allowlisted.
The point is the unbroken, affirmative record — the same reasoning as the
provenance ADR's headers: cheap at authoring time, unreconstructible later.

**4. DCO sign-off alongside, not instead.** Outside contributions must be
signed off (`git commit -s`, Developer Certificate of Origin). The CLA covers
*rights*; the sign-off certifies *origin* per commit and complements the
provenance discipline. Documented in `CONTRIBUTING.md`; enforcement stays
social/review-level for now rather than adding a second required check.

**5. Copyright assignment rejected.** An FSF-style assignment (contributor
transfers copyright) would be marginally stronger legally but is socially
costly, deters contributors, and buys little over a well-drafted grant CLA —
the relicensing right is what the option needs, not ownership.

## Consequences

- 100% of outside code is born covered: the CLA lands while the contributor
  count is still exactly one, so no retroactive consent-hunting can ever be
  needed.
- Contributors see an honest trade framed in the CLA itself: they keep
  ownership, their work stays open-source forever, and the maintainer can
  also license it commercially.
- The CLA text is self-drafted (Apache-ICLA-derived) and has not had legal
  review. It is good enough to start collecting signatures under; it should
  be reviewed by a lawyer before a substantial outside contribution or the
  first commercial license is executed (see Follow-ups).
- `CONTRIBUTING.md`'s implicit contribution terms are replaced by pointers to
  the signed CLA flow.
- Agreements name "Yona Appletree … including any successor legal entity" and
  are assignable, so forming a company later does not require re-signing.

## Alternatives Considered

- **Keep the implicit CONTRIBUTING.md terms.** Rejected: weakest evidentiary
  form — a contested contribution would rest on "they must have read the
  file." Fine at zero contributors; not fine as the basis for a commercial
  licensing business.
- **DCO alone.** Rejected: the DCO certifies origin and licenses inbound
  under the project's *existing* license only — it grants no relicensing
  right, so it cannot support dual licensing by itself.
- **Copyright assignment.** Rejected as above: high social cost, low marginal
  benefit over the grant CLA.
- **Hosted CLA services (EasyCLA, cla-assistant.io SaaS).** Rejected for now:
  the GitHub Action keeps signatures in-repo under the project's control with
  no external dependency; a hosted service can be adopted later without
  re-signing if scale demands it.

## Follow-ups

- Lawyer review of both CLA texts before accepting a substantial outside
  contribution or executing the first commercial license.
- When a corporate CLA is first executed, decide where executed copies and
  Schedule A updates are archived (currently: maintainer's records).
