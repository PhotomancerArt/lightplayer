# ADR: Pinned Firmware Figures Live in Per-Chip Records, Re-recorded by One Command

- **Status:** Proposed
- **Date:** 2026-09-25
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The three chip emulators' test suites and the heap-budget gate pin values read
off the shipped firmware image. In the seven days to 2026-09-25 most red PRs
were not bugs: an exact pinned figure moved because the firmware changed. The
tally: classic (v3) boot tests 16 (`the_single_core_prefix_is_unchanged` 8,
`the_heartbeats_memory_figures_are_the_desk_boards` 5,
`the_init_chain_is_the_golden_bytes` 3), S3 boot tests 11
(`the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire` 8), the C6 chip heap
ratchet 10, the engine heap ratchet 8.

One firmware change could move figures on three chips, and they lived in three
kinds of place, each re-recorded differently:

- the heap records (`scripts/heap-budget-record/`), by per-chip `just`
  recipes — already records since
  `2026-09-23-heap-budget-record-split-and-derived-stack`;
- literals in test source (`37_256` three times in one S3 test,
  `"[INIT] main stack 45344 B"`, cycle counts), by hand;
- sha256 + length pairs of boot chains, by hand, after reading the stream off
  a failing run's output.

The literal and digest pins churned for one dominant reason: the main stack is
the residual of RAM after `.data`/`.bss`, so every boot chain that prints
`[INIT] main stack <n> B` moves with every byte of statics — the same premise
the 2026-09-23 ADR corrected for `stackTotal` in the heap record.

Deriving those values instead (as that ADR did for `stackTotal`) was the other
option. It would have removed checks: a byte-for-byte boot chain and an exact
cycle count are strong regression tripwires for the *emulator*, and the ask was
to keep every check exactly as strict.

## Decision

1. **Two kinds of pin.** A **FIGURE** is what this tree's image measures; any
   image change can move it. An **EXACT** pin proves identity — equal to
   silicon, to a transcript, to a pinned reference commit's image, or a
   structural constant. FIGUREs live in a record; EXACT pins stay literals in
   the test, where a bless cannot reach them. A golden that *carries* a figure
   (a boot chain printing the stack size) is a FIGURE; a golden with none in it
   (the S3's `HELLO`) stays EXACT.
2. **Test figures live in `lp-emu/esp/figures/<chip>.json`**, read through a
   new MIT crate, `lp-emu-esp-figures` — inside the fence, so the emulator
   tests never read a file on the AGPL side. Checks stay exact; a failure names
   every moved figure old → new and prints the command that accepts it.
3. **Byte streams are recorded as text, line by line**, not as a digest:
   compared byte for byte (non-UTF-8 fails rather than being compared
   lossily), so the failure and the record's diff name the line that moved.
4. **`just bless-chips [chip…]`** is the one command: per chip, the heap record
   through its existing recipe and the chip's boot suite under
   `LP_EMU_BLESS=1`, one chip at a time; `engine` for the engine heap records;
   `--check` for the same gates without rewriting.
5. **Positional figures hold CI's value.** A figure that depends on where the
   build put the code (a stack high-water) is not reproducible between a desk
   and a runner. It is checked exactly, but a desk bless does not write it; a
   bless under `GITHUB_ACTIONS=true` does.
6. **One writer, one layout** (sorted keys, one figure per line, text as one
   JSON line per line), so a later CI job can run the gates under
   `LP_EMU_BLESS=1` and publish the records' diff as a patch.

## Consequences

- A statics-only firmware change now fails with a message naming each moved
  figure and `just bless-chips <chip>`; accepting it is one command and a
  reviewable JSON diff, not edits in three crates.
- The bless is only as honest as the reviewer: a bless after an *emulator*
  change would hide a regression. The failure message says so, and the record
  diff is where a reviewer sees it. Nothing mechanical distinguishes "the image
  changed" from "the machine changed" yet.
- Stale keys (a figure no test observes any more) are not pruned automatically.
- `bless-chips` runs each chip's whole boot suite, which is slower than running
  only the tests that observe figures. It keeps the recipe free of a list that
  could drift from the tests.
- The classic's `boot_idle.path_high_water_gap` fails on a desk worktree today
  (−64 against CI's −96 at `5e864de73`), as the literal it replaced did. It is
  now labelled `[positional]` in the failure, and a desk bless cannot overwrite
  CI's value with it.

## Alternatives Considered

- **Derive the values from the ELF** (`_stack_start − _stack_end`, as the heap
  gate does for `stackTotal`). Rejected for the test pins: it removes the boot
  chain's byte-for-byte check and the cycle count's emulator-regression value,
  and the ask was to keep every check.
- **Band the figures.** Rejected: loosens checks, and a band sized to make runs
  pass is what the heap gate forbids.
- **Put test figures in `scripts/heap-budget-record/chips/`.** Rejected: MIT
  test code would read AGPL-side files, and the heap records carry
  directions/bands the test figures do not.
- **Keep sha256 digests in the record.** Rejected in favour of text: equally
  strict, and a digest cannot say what moved.
- **Blessing through `insta`-style snapshot files.** Rejected: a third-party
  dependency for ~200 lines, and per-test snapshot files would scatter the
  figures a reviewer wants in one place per chip.

## Follow-ups

- A CI job that runs the three chips' gates with `LP_EMU_BLESS=1` under
  `GITHUB_ACTIONS=true` and uploads the records' diff as a patch artifact, so
  an author accepts a move without building any firmware — and the only way to
  bless positional figures without copying them from a log. **Done in PR
  #829** — as steps of the existing figure jobs rather than a job of its own,
  run only when a figure check failed, plus a sticky PR comment and
  `just apply-ci-figures <pr>` (docs/chip-figures.md, "When CI hands back the
  patch").
- Record an image identity beside the figures (a digest of the ELF's loadable
  sections), so a failure can say "the image is unchanged — this is the
  emulator" instead of asking the reader to know.
- `emu_esp32v3` / `emu_esp32s3` are deliberately not path-gated on the shared
  product crates (`lp-core/**` and friends; DD77 in `pre-merge.yml`), yet those
  crates move the classic's and the S3's figures. Such a change first fails on
  the main push. `bless-chips` makes the fix one command; whether the filters
  should widen now that the fix is cheap is a separate call.
