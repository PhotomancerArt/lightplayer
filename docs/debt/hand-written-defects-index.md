---
status: retired        # 2026-10-10, PR #1128: the index is built from entries' frontmatter
since: 2026-07-23      # the table's first row (852538d0b)
logged: 2026-10-10
area: docs/defects/README.md index table
related:
  - ../defects/README.md
  - ../../scripts/defects-index.py
  - ../../AGENTS.md ("Defect tracking")
  - two-green-prs-can-red-main.md
---
# The defects index was one hand-written table every defect PR edited

**Shape** — `docs/defects/README.md` kept the registry's index as one
markdown table, newest first. AGENTS.md told every PR that fixes or finds
a qualifying bug to write its entry *and add a row at the top of that
table*. So every defect PR edited the same few lines, and any two open
at once conflicted. That was not bad luck: the rule required it, and it
got worse as the number of agents working in parallel grew. A conflicted
PR also gets **no CI at all**. GitHub cannot build its merge ref, so no
`pull_request` workflow runs, and only the CLA check keeps reporting.
The PR looks quiet, not broken, until someone merges main into it by
hand.

The table drifted too. On the day it was removed it was missing 29 of
277 entries, listed one entry twice, and disagreed with two entries'
own frontmatter (a class, and a status still open in the entry). A
whole branch on 2026-10-01
(`claude/auto-2026-10-01-40x-defect-missing-index-row`) existed only to
add one missing row.

**Carrying cost** — a hand merge of the README for nearly every defect PR
that was open while another one landed. On busy days it took a branch
more than one merge (#1120 needed two in an afternoon), and each round
cost a CI cycle that had silently stopped while the PR was conflicted.
Since 2026-09-01, 151 merge commits touched the file, and in 102 of them
the merged README differs from both parents: rows from both sides were
combined by hand.

**Workarounds** — none needed now. While it lasted: merge `origin/main`,
keep both sides' rows newest first, push, and check that CI actually ran
on the new head. A branch cut before the table went away still re-adds
its row the first time it merges main. `just lint-defects` names that
row; delete it, because the entry's frontmatter already is the row.

**Incident log**
- 2026-09-01 → 2026-10-08: 85 combined merges of the README across
  branches (the wifi-link, OTA, relay and repartition branches merged
  main several times a day, each time re-combining the table). Counted
  from `git log --merges --cc`; never logged at the time.
- 2026-10-09: #1080 (the emulator's `sc.w` reservation) merged main and
  re-combined the index (c1ea313b9); #1064 (the board card) twice
  (13931815a, 04c50ef8e); #1074 (one tab holds a board) twice (f72df8b7e,
  b5c6eaf3b); #1062 (95e81a436); #1087 (1b617c81d).
- 2026-10-10: #1080 again (53cbeda6a), and its own merge (b4a9098e4);
  #1106 (063478b7b); #1122's own merge (4c2f85c8a);
  #1066 (pictures through the cloud, 673c861db, and its own merge
  559de6817); #1120 (the board card's work-fill clip) **twice**
  (be6281768, 9f6f6d1c3); #1121 twice (839dce103, fdd062c1e).

**Exit criteria** — met by PR #1128 (2026-10-10):
- no shared index line exists: `just defects-index` builds the table from
  each entry's frontmatter when someone reads it (`--by-class`, `--open`,
  `--class`), and nothing generated is committed, so there is nothing to
  regenerate after a merge either;
- every entry carries that frontmatter, and `just lint-defects` keeps it
  so: it runs in `check-lint`, and in CI's "Defect registry" job, which runs
  on a docs-only PR (a defect-only PR runs no Lint job). It also fails on a
  hand-written row re-added to the README;
- filing a defect is one new file, and closing one is an edit to that
  file.

The debt register's own index (this directory's README) has the same
shape at a lower rate: 7 of the 14 merges that touched it since
2026-09-01 combined rows by hand. If it starts costing real merges, the
same fix applies (a `cost:` frontmatter field, the same script shape).
