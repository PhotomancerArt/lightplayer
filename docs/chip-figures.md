# Chip figures: pinned firmware numbers, and the one command that re-records them

The three chip emulators' test suites (`lp-emu/esp/lp-emu-esp32{c6,v3,s3}/tests`)
and the heap-budget gate pin numbers that are read off the **shipped firmware
image**: a cycle count to a boot line, the main stack's size, a boot chain that
prints that size, a heap total, the wire protocol version. Those numbers are
right to pin exactly — a move is either a firmware change someone should see or
an emulator regression — but most red PRs in the week to 2026-09-25 were the
first kind: a routine firmware change moved a pin, on up to three chips, in
three different places, each re-recorded a different way.

So:

```bash
just bless-chips                  # re-record everything: the three chips, then the engine
just bless-chips esp32v3          # one chip (esp32c6 | esp32v3 | esp32s3 | engine)
just bless-chips --check esp32s3  # run the same gates, rewriting nothing
```

It runs one target at a time (each builds its own firmware, sequentially — never
two chips' builds in parallel on the shared desk), and ends with the records'
`git diff --stat`. Commit the diff **with the change that moved it**, and say in
the commit what moved it.

On a pull request you usually do not need to run it: when a figure moves, CI
has already blessed it against the images it built and posted the patch
([below](#when-ci-hands-back-the-patch)):

```bash
just apply-ci-figures <pr>        # download CI's figure patches and apply them; then commit and push
```

## What a figure is, and what is not

Every pinned number is one of two kinds, and the kind decides where it lives.

| kind | what it proves | where it lives | re-recorded by |
|---|---|---|---|
| **FIGURE** | nothing about identity: it is what *this tree's* image happens to measure, and any change to the image can move it | a record file the test reads | `just bless-chips` |
| **EXACT** | identity: equal to silicon, to a committed transcript, to a pinned reference image (a fixed commit's bytes cannot move), or a structural constant | a literal in the test | nothing. A move is a finding. |

A bless can only reach the first kind, by construction: EXACT pins are literals
in test source, so `LP_EMU_BLESS=1` has nothing to write. Transcripts under
`lp-emu/transcripts/` are never re-recorded by a bless either (they are
recorded through `lp-cli validate record`, and never edited).

### Where FIGUREs live

- **Test figures:** `lp-emu/esp/figures/<chip>.json`, read through the
  `lp-emu-esp-figures` crate. Inside `lp-emu/`, so inside the MIT fence: the
  emulator tests never read a file from the AGPL side.
- **Heap figures:** `scripts/heap-budget-record/chips/<chip>.json` and
  `scripts/heap-budget-record/engine/<project>.json`, unchanged
  ([heap-budget-gate.md](heap-budget-gate.md)). `bless-chips` re-records them
  through their own recipes (`heap-budget-baseline-chips*`,
  `heap-budget-baseline`); it adds no measurement of its own.

## How a test reads a figure

```rust
use lp_emu_esp_figures::Figures;

let mut figures = Figures::new("esp32v3", "determinism::the_single_core_prefix_is_unchanged");
figures
    .utf8("init_chain.prefix", &m.uart0().bytes())          // a byte stream, line by line
    .int("determinism.single_core_prefix.cycles", m.cycles());
figures.verify();
```

`verify()` compares every observed figure with the record and fails naming
**every** one that moved, not only the first:

```text
1 pinned firmware figure moved (esp32s3, boot_idle::the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire):
  stack_total_bytes: 37256 → 37192 (-64)
recorded in lp-emu/esp/figures/esp32s3.json
These are figures of the firmware IMAGE, not of the machine. If the firmware changed on purpose, accept them with:

    just bless-chips esp32s3

and commit the record with the change that moved it. If only the emulator changed, a moved figure is a finding: do not bless it.
```

(That is a real run: 64 B of `.bss` added to `fw-esp32s3`. A text figure that
moved reports its lines — `line 4: "[INIT] main stack 45344 B" → "…"`.)

With `LP_EMU_BLESS=1` (which `bless-chips` sets) `verify()` writes the observed
values into the record instead, under the file's own lock (tests in one binary
run in parallel and several share a key). Everything else in the test still
asserts, so blessing a broken tree still fails.

Rules the crate holds:

- A text figure is compared **byte for byte** — as strict as the sha256 it
  replaced — and a stream that is not UTF-8 fails rather than being compared
  lossily. Recording the text instead of its digest is what lets the failure,
  and the record's diff, name the line that moved.
- A `Figures` dropped without `verify()` panics, so an observation cannot go
  unchecked.
- A figure observed twice in one test with two values panics.

### Positional figures

Some figures depend on **where the build put the code**, not only on what the
code does: a stack high-water is the deepest point an interrupt happened to
land. The tree's image is not the same bytes on two hosts (host paths, the host
rustc's own build;
[the debt entry](debt/reference-images-are-not-reproducible-across-hosts.md)),
so a desk worktree and a CI runner can read different values off one commit —
the classic's `boot_idle.path_high_water_gap` reads −96 on CI and −64 on a desk
at `5e864de73`.

Those are observed with `positional_int`. They are checked exactly like any
other figure, against a record that holds **CI's** value; a failure tags them
`[positional]`; and a bless **on a desk does not write them** (it would record
a number CI cannot reproduce). A bless under `GITHUB_ACTIONS=true` does. On a
desk, take the new value from CI's failure message. Memory-class figures
(heap bytes, the main stack's *size*, boot text) are byte-equal across hosts
and are ordinary figures.

## The record format (machine-writable)

One flat JSON object per chip, written by exactly one writer
(`lp_emu_esp_figures::Record::render`):

- keys sorted by byte order; two-space indent; one figure per line;
- integers are integers; a digest or single line is a string;
- a text figure is an array of its lines split on `\n` (so a trailing newline
  is a trailing `""`), one line per JSON line — a moved boot line is a
  one-line diff;
- keys starting with `_` are prose, kept and never checked.

Identical figures always render identical bytes, and a bless that changes
nothing does not touch the file. That is what lets CI hand back a patch
instead of a log: the next section.

Record keys are `<test file>.<figure>` when one test file owns them, and bare
(`main_stack_bytes`, `init_chain.prefix`) when several tests read the same
fact. A key no test observes any more is not pruned automatically — delete it
in the change that stopped observing it.

## When CI hands back the patch

Every job that checks figures — `Emulator C6 (x64)`, `Emulator ESP32v3 (x64)`,
`Emulator ESP32-S3 (x64)`, `Heap budget (esp32c6 chip)`, and the engine ratchet
in `Validate (x64)` — tees its figure checks' output, and when one of them
fails it runs one more step, `Figure moves — bless what failed, diff the
records` (`scripts/ci/figures-patch.py`):

1. **Is this even a figure failure?** A test step whose output names no
   `pinned firmware figure`, or a heap step with any `::error::heap-budget:`
   line that does not offer a re-baseline (the band, the derived `stackTotal`,
   a missing window), is recorded as **not a figure move** and nothing is
   re-run.
2. **Bless exactly what failed, against what was built.** The failed tests are
   re-run by name, under `LP_EMU_BLESS=1` and `GITHUB_ACTIONS=true` (so
   `[positional]` figures are written — CI is the one place they should be),
   against the same images: the C6 through `test_support`'s cache, the classic
   and the S3 through the copies their boot recipes name in
   `target/lp-emu-esp32{v3,s3}/images.env`. No firmware is rebuilt; a heap
   re-baseline rebuilds incrementally, as its ratchet just did. A failed heap
   ratchet is re-baselined and then **checked again**.
3. **Only a clean bless is a patch.** A bless rewrites figures and asserts
   everything else, so a bless that passes proves every other assertion in
   those tests held; the heap re-check proves the same for the ratchet. If
   either fails — an EXACT pin, a transcript, a structural check, an ordinary
   test failure — the step is *not a figure move*, and the job gets **no
   patch at all**, even for its other steps' figures. A clean bless that
   changed no record means the failure did not reproduce: also no patch.
4. The job uploads `figures-patch-<job>` (`figures.patch`, a
   `git diff --full-index` of the records; `summary.json`, each moved figure
   old → new; the bless logs' tails) and **stays red**. Nothing is committed or
   pushed.

The `figures-comment` job then posts or rewrites **one** sticky comment on the
PR (marker `<!-- lp-figures-patch -->`): per job, a table of chip / record /
figure / old / new, the command that applies it, and a line for every failure
that was *not a figure move*. A later run with no figure failures rewrites it
to say the figures match. Fork PRs get a read-only token, so no comment — the
artifacts are still there, and the apply command reads them the same way.

To accept:

```bash
just apply-ci-figures <pr>    # every figures-patch-* of the PR's latest run on its head, one `git apply`
git commit -m "chore(figures): accept the moves from <what moved them>"   # then push
```

It stages the records and prints the stat; if `main` moved a record since the
run, a plain apply fails and it retries with `--3way`. The same rule as a desk
bless holds: **if only the emulator changed, do not apply** — the comment says
so too. Heap records CI re-baselined are stamped with the PR's head commit
(`LP_HEAP_BUDGET_COMMIT`), not the merge ref CI actually ran on.

Cost on a green run: nothing but `tee` and one ~15 s comment job that starts
after the figure jobs finish (and does not run at all when none of them ran).
The recipes' figure suites now run `--no-fail-fast`, so one red run reports
every test binary's moved figures, not the first binary's.

Proven on PR #829: 64 B of `.bss` in `fw-esp32s3` and `fw-esp32v3` failed two
jobs, whose bless steps took 10 s and 4 s (nothing rebuilt) and produced the
S3's `stack_total_bytes` 37,256 → 37,192 and the classic's six moves (the
`[INIT] main stack` line in three boot chains, `main_stack_bytes`, the
single-core prefix's cycles and instructions); `just apply-ci-figures 829`
applied both patches with no local build, and the next run was green.

## The inventory (2026-09-25)

What was pinned, where, and which kind it is. "Tree" means the image built from
this checkout; "ref" a pinned reference commit.

### Moved to a record (FIGURE)

| chip | key | was | test(s) | moved by |
|---|---|---|---|---|
| v3 | `init_chain.prefix` (8 lines) | `PREFIX_BYTES`/`PREFIX_SHA256` in `boot_idle.rs` **and** `determinism.rs` | `the_init_chain_comes_out_of_the_wire_byte_for_byte`, `the_single_core_prefix_is_unchanged` | `[INIT] main stack` (statics) |
| v3 | `boot.init_chain.blank`, `boot.init_chain.merged` | `INIT_CHAIN_{BLANK,MERGED}_{SHA256,LEN}` in `boot.rs` | `the_init_chain_is_the_golden_bytes` | the same line |
| v3 | `main_stack_bytes` | `"[INIT] main stack 45344 B"`, `" of 45344 B "` in `boot_idle.rs` | the prefix test; `the_two_paths_report_the_same_memory_figures` | statics |
| v3 | `determinism.single_core_prefix.{cycles,instructions,idle_skips}` | `PREFIX_CYCLES`/`_INSTRUCTIONS`/`_IDLE_SKIPS` | `the_single_core_prefix_is_unchanged` | boot-path code; the `stack_probe::paint` loop walks `.bss` |
| v3 | `boot_idle.path_high_water_gap` (positional) | `PATH_HIGH_WATER_GAP` | `the_two_paths_report_the_same_memory_figures` | layout |
| s3 | `stack_total_bytes` | `37256` ×3 in `boot_idle.rs` | `the_ledger_triple_is_elicited_by_a_stop_all_on_the_wire` | statics |
| c6 | `hello.proto` | `"proto":24` in `usb_attached.rs` ×2, `usb_control.rs` | `g2_1_…`, `g3_1_…` (`g2_3_…` no longer sees a hello: behind the IN-endpoint gate nothing past `[INIT]` is tried) | `WIRE_PROTO_VERSION` bumps |
| c6 | `heartbeat.total_bytes` | `"totalBytes":301536` in `usb_attached.rs` | `g2_1_…` | a heap-region change |
| all | chip heap records | already records | the heap-budget gate | [heap-budget-gate.md](heap-budget-gate.md) |
| — | engine heap records | already records | the heap-budget gate | engine allocations |

Migrated verbatim: the recorded texts reproduce the old pins' lengths and
sha256s exactly (543 B `05b27095…`, 804 B `92c857b2…`, 703 B `c0951fb6…`).

### Stayed a literal (EXACT)

| where | what | why |
|---|---|---|
| v3 `boot_idle.rs` `the_heartbeats_memory_figures_are_the_desk_boards` | `HEAP_USED_GAP` 84, `STACK_HIGH_WATER_GAP` 640, every line against the silicon transcript | silicon vs emulator on the **pinned `75486b114` reference image**: only an emulator change moves them |
| v3 `boot_idle.rs`, `boot.rs` | `heap=15072+112640+98304+15536=241552` | byte-identical to silicon's boot banner; heap configuration |
| v3 `rom_up_boot.rs` | the bootloader log, partition rows, `SILICON_SEGMENTS` | the committed silicon capture; `partitions.csv` |
| v3 `boot.rs` | app entry `0x4008_0844`, 6 segments / 1 relocated, first strict stop at cycle 29, ROM symbol addresses | layout identity; move only with a linker-script change, which is a finding |
| s3 `boot_idle.rs` | `HELLO` (253 B, `da070ac0…`) | a golden with no figure in it: heap size and RWDT config, changed only on purpose |
| s3 `boot_idle.rs` | the all-zero `[JIT]` line, `retry_saves=0` | structural constants |
| c6 `boot_idle.rs`, `upload_walk*.rs`, `rom_up_boot.rs`, `harness_parity.rs` | heartbeat memory, stack totals, load/compile lines, flash census | pinned reference images (`d6cfaa205`, `735af98ae`) and silicon transcripts |
| c6 `boot.rs`, `boot_no_radio.rs`, `usb_control.rs`, `rmt_chase*.rs` | 7 app segments, RWDT config, 64-byte packets, payload configs | layout identity, firmware configuration, protocol constants |
| all | `lp-emu/transcripts/**`, `walks/*.script` | recorded evidence; walks are re-captured on a project-format bump, never blessed |

## A finding from the proof run: `totalBytes` is not purely a memory map

The heap gate grades `totalBytes` **exact**, as "the chip's memory map, not a
budget". Adding 64 B of `.bss` to `fw-esp32s3` moved the S3's `totalBytes`
245,756 → 245,760 (and `freeBytes`/`largestFreeBlock` by the same 4 B): the
heap is a fixed-size static, but where `.bss` puts it changes the alignment
padding `esp_alloc` loses at its start. So a statics-only change can move it by
a few bytes. `bless-chips` re-records it with everything else; the grade is
unchanged here, and whether it should become "exact modulo alignment" is a
question for the heap gate, not this doc.

## Blessing against CI's images (no firmware build)

`bless-chips` builds each chip's firmware first. It does not have to: fetch the
images CI built for this commit and point the run at them.

```bash
just fetch-ci-images <pr>             # prints: export LP_CI_IMAGES=…/target/ci-images/<commit12>
export LP_CI_IMAGES=…
just bless-chips esp32v3              # both halves, on CI's bytes, in seconds
just bless-chips --check              # or just check every chip
```

The heap records and the figure records are both read off CI's images. The
fetched set is refused if its firmware sources differ from this checkout
(a firmware edit you have not pushed yet is exactly that), so this is for an
emulator-side change, or for re-running what CI ran. `[positional]` figures
are still written only by CI. Everything else is in [ci-images.md](ci-images.md).

## When not to bless

- **Only the emulator changed.** A figure is a figure of the image; if the image
  did not change, the machine did, and that is a finding to explain (the v3
  history in `determinism.rs` shows what explaining one looks like).
- **An EXACT pin failed.** Nothing to bless — read it.
- **A positional figure failed on a desk.** CI's value is the record's; see above.
  On a PR, `just apply-ci-figures` carries it.
