---
status: retired
since: 2026-09-06
logged: 2026-09-23
area: scripts/heap-budget-check.sh + scripts/heap-budget-record.json (the heap ratchet, both arms)
related:
  - docs/heap-budget-gate.md
  - docs/adr/2026-09-23-heap-budget-record-split-and-derived-stack.md
  - PR #798
  - PR #1126
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

- 2026-10-07 — **CI's dirty re-check fails again; the net seam moves the C6
  record** (emulated Wi-Fi PR C, #993, run 37584523250). After the merge of
  main took main's C6 record, PR C's net seam moved `usedBytes` +24 B and
  `largestFreeBlock` -32 B (`freeBytes` -24 B) on the clean first step
  (version `14b39e7`, `lp-emu:esp32c6:t1+net=lan`). CI's "Figure moves"
  re-check ran on a dirty tree (`<sha>-dirty-…` version, +32 B) and failed
  as "not a figure move" again. Workaround as on 2026-10-06: the record
  carries CI's clean figures (103,024 / 198,512 / 119,400), transcribed
  from the first clean step, not a local bless.

- 2026-10-07 — the cloud relay on the C6 (Wi-Fi relay PR B, #1019, P8): a
  real move, not churn — the network slots' parked-handshake buffers and
  per-edge signals, two more embassy-net socket slots and its DNS socket
  took the C6's boot heap from 103,000 to 105,560 B used and its largest
  block from 119,432 to 116,840 B (nothing saved, so no relay buffers);
  the main stack's high water 11,940 → 12,200 B, measured on a local
  bless before main's net-seam record landed. The merge of main took
  main's record (the director owns the figures); CI's clean figures on
  the merged tree replace it.

- 2026-10-07 — **the dirty re-check again, and CI's own images are not the
  ratchet's image** (OTA M7 Bluetooth updates, PR #1005, run 37631551382,
  merge commit `f3035c68c858`). The heap job's clean first check
  (`target/fw-split/shipped/p2.elf`, version `f3035c6`) read `usedBytes`
  103,088 / `freeBytes` 198,448 / `largestFreeBlock` 119,344 against a record
  of 103,096 / 198,440 / 119,360, and failed on `largestFreeBlock` alone
  ("shrank"; the other two read "improved"). The `Figure moves` step then
  re-baselined and re-checked on a tree its own write had dirtied: the log's
  pass 1 reads `version f3035c6-dirty-071620PT` (against `f3035c6` for the
  two clean builds), the re-check read 103,120 / 198,416 / 119,288, and the
  step answered "not-a-figure-move" — the 2026-10-06 entry's mechanism,
  confirmed in the log, the second time on this branch. Found on the way:
  `just fetch-ci-images 1005 esp32c6` + `heap-budget-baseline-chips` /
  `heap-budget-check-chips-c6` reads the record's own 103,096 / 198,440 /
  119,360 and passes with no diff, because the `Emulator C6 (x64)` job's
  image is stamped with a 9-character version (`f3035c68c`, `commit=f3035c68c858`
  in its hello) while the heap job's build stamps 7 (`f3035c6`). 2 characters
  x the stamp's copies is the 8 B the heap job reads lower, so the fetched
  image reproduces the record, not the failing job. Two builds of one commit
  inside one CI run therefore carry different C6 figures, and `docs/chip-figures.md`'s
  "the heap ratchet's image is the same bytes the boot suite reads" does not
  hold for the version string. Workaround as on 2026-10-06/07: the record
  takes the heap job's clean first-step figures (103,088 / 198,448 / 119,344),
  transcribed from the log, not a local or fetched-image bless. Paydown as
  above and now with a second reason: pin `APP_VERSION` for every build the
  figure checks use, so a build's stamp length stops being a figure.
  Applied on PR #1005 as `chore(figures): record CI's clean C6 heap figures for 4cad23fdb` (the json's `commit` field left at `e922ceca5`).

- 2026-10-07 — **the cloud relay's final figures (PR #1019, head `a303512e4`):
  a real move plus the dirty re-check, again.** The C6 record
  (`scripts/heap-budget-record/chips/esp32c6.json`) was transcribed from CI's
  clean first-step figures, 105,580 B used / 195,956 B free / 116,840 B
  largest block, because the "Figure moves" re-check ran on its own dirtied
  tree and could not pass (the 2026-10-06 and 2026-10-07 entries above, the
  same mechanism a third time in a day). The S3 and the classic moved **16 B
  of stack** (S3 `stack_total_bytes` 32,448 → 32,432; the classic's
  `main_stack_bytes` and the boot lines 37,088 → 37,072, with the determinism
  prefix counts), taken from CI's own patch with `just apply-ci-figures 1019`
  with no local build. The C6 move is the relay's (network slots'
  parked-handshake buffers and signals, two socket slots and the DNS socket,
  +2,580 B used at boot, nothing saved); the 16 B stack moves have no
  attributed cause. Workaround as before: the director takes CI's figures,
  never a local bless.

- 2026-10-10 — **the dirty re-check a fourth time, and a fetched image
  reads differently again** (relay pictures PR #1066, head `e7d96922f`, run
  38082107209, merge commit `2cbecd05f3e1`). The heap job's clean first step
  (version `2cbecd0`) read 104,652 B used / 196,884 B free / 116,656 B
  largest block / 12,168 B stack high water against main's record of
  105,708 / 195,828 / 116,704, failing on `largestFreeBlock` alone (-48 B;
  the other two "improved"). Its `Figure moves` bless agreed with that
  boot to the byte; the re-check, built as `2cbecd0-dirty-134625PT`, read
  104,684 / 196,852 / 116,672 and answered "not-a-figure-move", so
  `just apply-ci-figures 1066` found no patch. A local
  `heap-budget-baseline-chips esp32c6` on the fetched `Emulator C6` image of
  the same run read 104,660 / 196,876 / 116,688 / 12,164 — the 2026-10-07
  entry's two-builds-in-one-run gap. Workaround as before: the record takes
  the heap job's clean first-step figures, transcribed from its log.

- 2026-10-10 — **the dirty re-check, paid down** by PR #1126. The C6
  heap ratchet's image is now a **figure build** (`LP_FIGURE_BUILD=1`,
  `tools/lp-app-version`): stamped with the fixed version `0000000` and
  `LP_BUILD_DIRTY=false` whatever the tree's state, and built into
  `target/fw-split/figures` (`target/fw-split/shipped` keeps the tree's real
  version). The emulator suite's shipped split image
  (`lp_emu_esp32c6::test_support`) is a figure build too, so the image CI
  uploads for `just fetch-ci-images` is the ratchet's bytes again. Product
  builds (release, deploy, the packager, `just fw-esp32c6-split`, flashing)
  never set the variable. Seven characters is CI's own short-sha length, so
  the record taken from #1066's clean first step held without a re-baseline
  (#1126's own run read 104,652 / 196,884 / 116,656 / 12,168, the record to
  the byte). Proof:
  - **CI** — PR #1127 (#1126 plus 64 B held at boot, never merged): the
    heap job failed on `usedBytes` 104,652 → 104,716 and `freeBytes`
    196,884 → 196,820; its `Figure moves` step re-baselined, the re-check
    rebuilt on the tree the re-baseline had just written (`version
    0000000`), read the same figures and passed, and the job uploaded
    `figures-patch-heap-budget-chips` (`verdict: figure-move`, run
    38095029199). `just apply-ci-figures 1127` applied it with a plain
    `git apply`, and the next run's heap job was green.
  - **Desk** — on one commit, the figure build of a clean tree and of a tree
    with the record rewritten are the same bytes (`p2.elf`, `loader.elf`,
    `merged.bin`) and read the same 104,652 / 196,884; the emulator suite's
    feature spelling (`esp32c6,server,radio`) builds the same bytes as the
    ratchet's (`esp32c6,server`); the old unpinned build of the dirty tree
    (`841831e2b-dirty-170734PT`) read 104,688 / 196,848 and failed. This desk
    now reads CI's `usedBytes` and `freeBytes` exactly (its 9-character sha
    used to cost 8 B).
  What it does not cover: `largestFreeBlock` still differs between hosts on
  one image (116,720 on this desk against CI's 116,656 — the 2026-10-07
  entry's placement effect, not the version); and the classic's and the S3's
  images are not figure builds, because their ratchet boots their boot
  suite's own image, whose build script stamps the version once per HEAD, so
  CI's re-check there was never dirty — a desk bless of those two can still
  differ from CI by its sha's length. Workaround now: **take CI's C6 patch**
  (`just apply-ci-figures <pr>`); the transcribe-from-the-log workaround of
  the 2026-10-06 – 10-10 entries above is retired.

**Exit criteria** — a PR whose only memory effect is a few bytes of statics
passes the gate without touching the record, and two PRs that each
legitimately re-baseline different chips/projects do not conflict. Likely
shapes (a paydown ADR picks one): grade `stackTotal` against a derived
expectation (RAM − statics) or as a band; report layout figures without
gating on them; split the record per chip/project so re-baselines touch
disjoint files.
