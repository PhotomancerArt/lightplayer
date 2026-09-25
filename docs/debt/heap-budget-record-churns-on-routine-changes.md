---
status: retired
since: 2026-09-06
logged: 2026-09-23
area: scripts/heap-budget-check.sh + scripts/heap-budget-record.json (the heap ratchet, both arms)
related:
  - docs/heap-budget-gate.md
  - docs/adr/2026-09-23-heap-budget-record-split-and-derived-stack.md
  - PR #798
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
- 2026-09-23 — **paid down** by PR #798
  (`docs/adr/2026-09-23-heap-budget-record-split-and-derived-stack.md`):
  `stackTotal` leaves the record and is graded against the ELF's own
  `_stack_start − _stack_end` (with `stackTop` recorded exact in its place,
  and a check that nothing sits between the statics and the stack); the
  record is split into `scripts/heap-budget-record/engine/<project>.json`
  and `scripts/heap-budget-record/chips/<chip>.json`, each with its own
  stamp, rewritten only when its figures move. Both exit criteria are met —
  a statics-only change on the C6 passed with the record untouched (proof in
  the PR), and re-baselines of different chips/projects touch disjoint
  files. The workarounds above describe the one-file record and no longer
  apply.
- 2026-09-24 — while #798 was open, main re-baselined the one-file record
  five more times, and #798 conflicted on it (modify/delete) when it merged
  main. The churn this entry describes, in one day:
  - #805 (`e92197125`): the S3 IN-endpoint gate added statics; S3
    `stackTotal` 37,296 → 37,272, heap figures identical.
  - #804 (`14b358121`): lean-wire follow-ups; engine `retained`/`alloc_bytes`
    +128 B on all three projects' `project-load` (a real budget move), and
    every `largest_free_at_close` −128 B beside it.
  - #804 (`aeabfe8b6`): a 16 B statics move shifted all three chips' stack —
    C6 71,152 → 71,136, classic 45,360 → 45,344, S3 37,296 → 37,280 — plus
    the emulator tests that pin those numbers.
  - #810 (`f1bed47e6`): BLE on the C6 + the heap cut; C6 `totalBytes`
    325,536 → 301,536, `stackTotal` 71,136 → 62,664, high-water band
    11,100..12,200 → 12,400..13,500 (a real budget move).
  - #805 (`c2239e8e0`, its merge of main): both S3 moves together → S3
    `stackTotal` 37,256, and `boot_idle.rs`'s pin with it.
  Resolved in #798's merge by deleting the monolith and regenerating the
  split files that moved with `just heap-budget-baseline` /
  `heap-budget-baseline-chips` — no numbers hand-carried. Four of those
  five moves were `stackTotal`-only, which the split record no longer
  stores: the classic's and the S3's files needed no edit at all, which is
  the exit criterion met on real traffic.
- 2026-09-25 — **the same churn, one layer out.** The split fixed the heap
  record, but the chip emulator tests pinned the same figures as literals:
  over the week to 2026-09-25 the classic's boot tests went red 16 times
  (`the_single_core_prefix_is_unchanged` 8,
  `the_heartbeats_memory_figures_are_the_desk_boards` 5 — already paid down
  on 2026-09-23 by pinning it to the `75486b114` reference image, and it
  stays an EXACT pin, `the_init_chain_is_the_golden_bytes` 3), the S3's 11
  (`the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire` 8), against 10
  C6-chip and 8 engine heap-ratchet failures — mostly `[INIT] main stack` and
  `of <n> B` moving with statics, in a boot chain's sha, a cycle count, or a
  literal `37_256`. Each chip re-recorded a different way, some by hand.
  Paid down by `docs/adr/2026-09-25-chip-figures-live-in-records.md`: those
  pins move to `lp-emu/esp/figures/<chip>.json` (exact as before; the failure
  names each figure old → new) and `just bless-chips [chip…]` re-records them
  and every heap record in one command. Found on the way: the classic's
  `PATH_HIGH_WATER_GAP` reads −64 on a desk worktree against CI's −96 at one
  commit (positional; `docs/chip-figures.md`), so that test was already red
  on a desk.
- 2026-09-25 — **the accept step, paid down** by PR #829. After #826 a moved
  figure still cost a local firmware rebuild, a bless, a push and another
  ~20-minute CI wait. Now each figure job (`emu-c6`, `emu-esp32v3`,
  `emu-esp32s3`, `heap-budget-chips`, the engine ratchet in `validate-x64`)
  re-runs exactly what failed as a bless against the images it already
  built, uploads `figures-patch-<job>` only when that bless passed, and
  `figures-comment` posts one sticky PR comment (each figure old → new);
  `just apply-ci-figures <pr>` applies every job's patch at once. The jobs
  stay red and nothing is pushed for you; an EXACT pin or ordinary failure
  gets no patch and is labelled "not a figure move". Proof, on the PR
  itself: 64 B of `.bss` in `fw-esp32s3` + `fw-esp32v3` → run
  36117736744 red with both patches (bless steps 10 s and 4 s — no rebuild),
  applied in one command with no local build, then green. Workaround now:
  **on a PR, `just apply-ci-figures <pr>`; `just bless-chips` is for a desk
  change you have not pushed.** Positional figures, which a desk bless could
  never write, now arrive the same way.
- 2026-09-25 — **a clean 3-way merge wrote the wrong figures.** Merging
  `origin/main` into the multi-pattern-projects branch (plan P6) conflicted
  only on `chips/esp32c6.json`'s `commit` stamp; git merged the figures
  themselves without a conflict and kept main's (`freeBytes` 216,624), but
  the merged tree boots at the branch's own figures (216,604, `usedBytes`
  +20, `largestFreeBlock` 196,033), so `heap-budget-check-chips-c6` went red
  on a tree with no firmware change of its own. Workaround: after a merge
  that touched a chip record, run the chip's check before trusting the
  merged numbers, and re-baseline to what the merged tree measures
  (`just heap-budget-baseline-chips esp32c6`). A record merged as text is
  not a measurement.

**Exit criteria** — a PR whose only memory effect is a few bytes of statics
passes the gate without touching the record, and two PRs that each
legitimately re-baseline different chips/projects do not conflict. Likely
shapes (a paydown ADR picks one): grade `stackTotal` against a derived
expectation (RAM − statics) or as a band; report layout figures without
gating on them; split the record per chip/project so re-baselines touch
disjoint files.
