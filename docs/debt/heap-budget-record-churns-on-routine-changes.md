---
status: carried
since: 2026-09-06
logged: 2026-09-23
area: scripts/heap-budget-check.sh + scripts/heap-budget-record.json (the heap ratchet, both arms)
related:
  - docs/heap-budget-gate.md
  - docs/debt/reference-images-are-not-reproducible-across-hosts.md
  - lp2025/2026-09-23-1701-lp-json-pack
---
# The heap-budget record re-baselines on routine changes and conflicts on every merge

**Shape** — the ratchet keeps every figure for every project, window and chip
in ONE committed JSON file, and several of those figures move with changes
that have nothing to do with memory budgets. The clearest case is the chips
arm's `stackTotal`, graded **exact** because `docs/heap-budget-gate.md` says
"a change is a linker-script or memory-map change, never a budget". On the C6
and S3 that premise does not hold: the main task's stack is what is left of
RAM after `.data`/`.bss`, so **8 bytes of new static data moves it by 8**. A
serializer change that adds one static table fails the gate with "a change is
a finding", and the only answer is a re-baseline commit. Because every PR that
re-baselines rewrites the same lines (and the `recorded`/`commit` stamp), two
PRs that both re-baseline always conflict, and the merge resolution is "take
main's, re-run the baseline" — a full firmware build plus an emulator boot.

This is structural, not one bug: the record mixes budget figures (a ratchet
is right) with layout figures (a ratchet is noise), and centralises them in a
single conflict hotspot.

**Carrying cost** — `git log origin/main -- scripts/heap-budget-record.json`
holds 67 commits, **37** of them re-baselines, 19 since 2026-09-06 alone.
Each costs a firmware build + emulator boot (~3–6 min per chip) and a line of
review attention that is almost never a finding. Merges of main into a
long-lived branch conflict on this file whenever main re-baselined too.

**Workarounds** —
- A failing `stackTotal` on a C6/S3 whose `.data`/`.bss` grew is expected:
  `just heap-budget-baseline-chips <chip>` for that chip only, and say in the
  commit why statics grew.
- On a merge conflict in the record: `git checkout --theirs
  scripts/heap-budget-record.json`, then re-run `just
  heap-budget-check-chips` (and `just heap-budget-check` for the engine arm)
  and re-baseline only what it names.

**Incident log**
- 2026-09-23 — lp-json-pack P2 (PR #795): the base64 blob change added 8 B of
  `.data`; C6 `stackTotal` 71,152 → 71,144, heap figures identical. Re-baselined.
- 2026-09-23 — lp-json-pack, merging main after lean-wire #791: conflict in
  the record (both sides re-baselined the C6); took main's, C6 `stackTotal`
  moved again 71,136 → 71,128 for the same 8 B. Second re-baseline.

**Exit criteria** — a PR whose only memory effect is a few bytes of statics
passes the gate without touching the record, and two PRs that each
legitimately re-baseline different chips/projects do not conflict. Likely
shapes (a paydown ADR picks one): grade `stackTotal` against a derived
expectation (RAM − statics) or as a band; report layout figures without
gating on them; split the record per chip/project so re-baselines touch
disjoint files.
