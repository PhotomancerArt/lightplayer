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
- 2026-09-25 — **the multi-pattern-projects PR (#827) re-blessed the S3 and
  the classic three times in one day**, with no memory budget moving: P4 in
  its main merge (DD8), P8 (`75899cd93`, both chips), and P8 again after its
  own main merge (`bcbcd2c7f` S3 stack 37,200 → 37,152, `0acb47bbd` classic
  main stack 45,288 → 45,240). Every one was the main stack moving with
  statics, carried in the boot-line figures. The engine records were also
  re-baselined twice (+32 B parked panel writers, +96 B after a main merge)
  and the C6 chip record three times (`dbad20254`, `20635e7f8`, `4b076bf10`,
  one of them the merge-as-text case above). Two things made each of these
  dearer than the paydowns assume:
  - **desk ≠ CI on positional figures.** The classic's
    `PATH_HIGH_WATER_GAP` read differently on the desk than in CI, so a desk
    bless could not produce CI's number and the v3 `--check` stayed red
    locally after a correct bless (`docs/chip-figures.md` says so; it still
    cost an agent a cycle to learn).
  - **a per-chip bless runs past the agent harness's ~590 s foreground
    limit.** `just bless-chips esp32c6` was moved to the background by the
    harness, and the agent that started it stopped there and never came back
    (P8's first run, twice). Workaround: bless one chip per command and
    prebuild its image first; on a PR prefer `just apply-ci-figures <pr>`,
    which needs no local build at all.

- 2026-09-25 — **a band move is still a desk bless** (learned wire
  dictionary, PR #835). The main-task stack high-water moved out of its band
  on the C6 (13,188 → 13,812 B) and S3 (12,432 → 13,008 B), with every heap
  figure byte-identical to `main`. Bisected to the lpc-wire/firmware change
  (the codec-only commit measured `main`'s figure); an in-place hash clear
  and out-of-line cold paths left it unchanged, and CI measured the same
  values, so it reads as the layout sensitivity this gate documents rather
  than a new deep frame. CI's figure jobs labelled both "not a figure move"
  (a value leaving its band is an ordinary failure), so no patch came back:
  it took a local `just bless-chips` of all three chips (≈25 min of
  sequential firmware builds), redone once after merging `main` because
  `main`'s own C6 change moved the C6 figures again. Workaround: merge
  `origin/main` BEFORE blessing.

- 2026-09-28 — **a static moved to the heap is a re-baseline** (BLE on
  lp-link, P3, PR #880). The radio link port left `.bss` (432 B) for the heap
  (its slots hold `RefCell`s), so the idle C6's `usedBytes` rose 196 B and
  `largestFreeBlock` fell 240 B: a net RAM saving the ratchet reads as growth,
  at margin 0. A first cut that held each slot's `Link` inline also pushed the
  stack high-water out of its band (14,308 B); boxing the slot and keeping the
  mux's pump `#[inline(never)]` brought it back to 12,868 B, and the heap
  record was re-baselined (the three `bless-chips esp32c6` steps run one at a
  time; ≈10 min). The same bless caught `hello.proto` 30 → 31, which the
  proto bump earlier on the branch had moved without re-recording.
- 2026-10-06 — Wi-Fi link on the C6 (PR #989, P03–P06): a real move, not
  churn — the `lp-net` thread's 8 KB stack, embassy-net's resources and the
  station took the C6's boot heap from 90,024 to 104,064 B used and its
  largest block from 132,233 to 118,152 B (nothing saved, never joins);
  wire 38 moved `hello.proto` and the S3/classic stack figures. Taken from
  CI's patch (`just apply-ci-figures 989`), no local bless.

- 2026-10-06 — **a merge took the record twice in one PR** (OTA update
  protocol Part B): the C6 record conflicted with #880's re-bless of the same
  chip (both at wire proto 37), so main's was taken and the C6 re-blessed again
  at proto 38 — `bless-chips esp32c6` in five 10-minute steps, again. The same
  PR also found that the C6 gate's 6.5 s window is a boot-time assumption:
  a first boot that hashes the engine and the core (~1.9 s emulated) reached
  its first heartbeat past it, which the gate reported as "no first
  heartbeat", not as a timing change (window now 8.5 s, with the reason).
- 2026-10-06 — **a stacked PR takes its bases' moves as well as its own**
  (Wi-Fi in the emulator, PR #993, stacked on #987 and #989): merging #989's
  main merge (wire 39, #986's OTA) moved `hello.proto` 38 → 39 and the
  S3/classic stack figures (−40 B / −48 B), and the C6 record moved by #989's
  network stack (90,208 → 101,608 B used) — none of it #993's own. #993's
  own move is the record's `configuration` (`lp-emu:esp32c6:t1+net=lan`: the
  network seam engages on every emulated run); with nothing joined the seam
  costs +32 B of heap, measured against `--seams none`. Taken from CI's patch
  (`just apply-ci-figures 993`); #989 will take the same base moves again.
- 2026-10-06 — **the C6 ratchet's first heartbeat moved 32 B between two
  boots of one image** (PR #993 after merging main's #997/#1000/#1001 and
  #989's latest, run 37514407160): the check measured 102,980 B used, CI's
  bless re-ran it and measured 103,012, so the bless could not hold
  ("not a figure move"); a desk run measured 102,988 twice. `net=lan`
  engaged in every boot, so the spread is the host link's timing reaching the
  first heartbeat (what the packed link has allocated by then), not the
  seam. The record was set by hand to CI's worst-seen boot (103,012 used /
  198,524 free / 119,360 largest), with the move's causes: +128 B of
  server-boot from the bases (the engine records moved by the same 128) and
  +32 B of the network seam's two boxes on a board that never joins. A gate at
  0 % margin over a figure that varies with the host is this entry's shape
  again; a band, as `stackHighWater` already has, is the paydown.
- 2026-10-06 — the same, once more after #989's main merge (`ec48b7afc`):
  #989's record (102,956 B used) is its own tree's, without the network
  seam's two boxes (+32 B on a board that never joins); #993's merged tree
  measured 102,980 then 103,012 on two CI boots (run 37530685444), so the
  record again takes the worst boot (103,012 / 198,524 / 119,384).

- 2026-10-06 — **a merge's "take theirs" dropped a branch's own figures**
  (Wi-Fi PR B, #989): merging main after #986, the C6 record conflicted and
  main's was taken, which was main's 90,208 B used without PR B's station.
  The next full run (the memory-gate push) then reported "usedBytes grew
  102,820 > 90,208", which reads as the new change costing 12.6 KB when it
  had saved 1.2 KB against PR B's own 104,064. The memory-gate change also
  moved the S3's and the classic's stack by 40-48 B (the exact probe) and the
  C6 emulator's `hello.proto` (wire 39, missed at the bump). Taken from CI's
  patch (`just apply-ci-figures 989`). Workaround: after taking main's record
  in a merge, re-bless or apply CI's patch in the merge's own push, before
  another change lands on top of it.

- 2026-10-06 — **CI's own C6 figure patch measures a dirty build** (Wi-Fi
  PR B, #989, run 37512040622): the ratchet failed at 102,948 B used, the
  `Figure moves` step re-baselined and re-ran, and the re-run read 102,980 B
  and failed again as `not-a-figure-move`. Writing the record dirties the
  tree, so the re-run's firmware is rebuilt with the app version
  `<sha>-dirty-<HHMMSS>PT` (`tools/lp-app-version`) instead of `<sha>`, and the
  longer string costs 32 B of heap. The patch CI offers for the C6 is
  therefore always 32 B above a clean build (that is why PR B's earlier
  applied patch, 102,852, read "improved 102,820" on the next clean run), and
  it can never pass its own re-check. The short sha's length also differs by
  machine (a local clone printed 9 characters against CI's figure 8 B lower).
  Workaround: re-baseline the C6 locally on a committed, clean tree
  (`just heap-budget-baseline-chips esp32c6`) rather than taking CI's C6
  patch. Paydown: pin `APP_VERSION` (as the deploy workflows already do) for
  the ratchet's builds, or rebuild the re-check from the pre-write tree.

- 2026-10-06 — **the C6's largest free block moves with the build, not
  only with the tree** (cloud relay PR A, #999, stacked on #989): CI's
  clean ratchet build of the merge commit (version `63e0a98`, job
  112523953992) read `largestFreeBlock` 119,424 against the record's
  119,440, with `usedBytes` 102,948 ("improved"). PR A's only
  firmware-linked change is a few lines of `lpa-server`'s access state (a
  `LinkTrust::Relayed` arm), and #989's own clean CI build read 119,440. A
  local clean bless of the same commit read 119,440 too, both with the
  local 9-character sha and with `APP_VERSION` pinned to CI's 7 characters
  (which did reproduce CI's `usedBytes`). So the 16 B is a layout effect
  that only CI's build shows, and no local bless can record it. Workaround
  taken: the record carries CI's clean figures (102,948 / 198,588 /
  119,424), transcribed from that job's first, clean step, not its dirty
  re-check. Paydown as above: pin `APP_VERSION` for the ratchet's builds,
  and grade `largestFreeBlock` as a band.

- 2026-10-07 — **the C6's `largestFreeBlock` differs between machines on
  one image** (Wi-Fi PR B, #989, run 37555989018). CI read 119,432 B. This
  Mac read 119,456 B on CI's own fetched image (`just fetch-ci-images`, so
  no build difference), and 119,440 B on a local build. `usedBytes` and
  `freeBytes` matched. The local clean re-baseline therefore failed CI by
  8 B. Workaround: record CI's (lower) figure, which passes on both. The
  ratchet's 0 % margin on a placement figure turns a host-side difference
  in the emulated run into a red check.

**Exit criteria** — a PR whose only memory effect is a few bytes of statics
passes the gate without touching the record, and two PRs that each
legitimately re-baseline different chips/projects do not conflict. Likely
shapes (a paydown ADR picks one): grade `stackTotal` against a derived
expectation (RAM − statics) or as a band; report layout figures without
gating on them; split the record per chip/project so re-baselines touch
disjoint files.
