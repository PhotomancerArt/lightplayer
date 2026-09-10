---
kind: report
repo: lp2025
date: 2026-09-10
---

# Xtensa firmware ISA inventory: exact decoder coverage and the study's five cheap experiments

**Date:** 2026-09-10 · **Milestone:** M0 of the Xtensa emulator plan
(`2026-09-10-0021-xtensa-emulator/m0-inventory-and-experiments.md`) · **Branch:**
`claude/xt-m0-isa-inventory` · **On top of:** `main`.

## One-paragraph answer

`lp-xt-inst`'s decoder — measured `objdiff`-exact, not by the scoping study's
mnemonic-name-table proxy — decodes **98.21 %** of the shipped v3 firmware
image, **95.69 %** of the classic mask ROM, and **93.58 %** of the IDF
second-stage bootloader `espflash` bundles with no supplied bootloader. The
single largest coverage gap in every artefact is `lsi` (the FP immediate-offset
load), and `lp-xt-inst` also **misdecodes** (not merely fails to decode) six
real `lsi` instructions in the v3 image as `ssai` — a genuine decoder bug, not
a coverage gap. The literal-pool sweep confirms the study's §2.3 hazard is
real: 4.16 % of a naive width-following walk's decoded bytes fall outside
their enclosing symbol or inside a literal/rodata section. The register-window
floor experiment (added via the emulator's existing trace hook, no
product-code edit) found window-overflow frequency swings across four orders
of magnitude by call pattern alone — 0.009 to 70,456 spills per million
instructions — which is itself the honest answer to "is overflow common": it
depends entirely on the firmware's own call graph, unmeasurable without the
v3 machine. No code in the shipped image, the ROM, or the bootloader
references the SRAM1 I-bus alias (`0x400A_0000..0x400C_0000`).

## Provenance

- Shipped v3 image: `fw-esp32v3` (default features `esp32, server, float-f32`),
  handed to this agent pre-built from commit `2be6b6235`, 2,984,664 bytes.
  **Not rebuilt** — reused as instructed.
- Classic mask ROM: `esp32_rev300_rom.elf` from `espressif/esp-rom-elfs`
  release `20260528`. Fetched to scratch only, not vendored (M3's job).
- Bootloader: `espflash save-image --chip esp32 --merge` on the v3 image
  above, no bootloader supplied, carved at flash offset `0x1000` (see §1.3).
- Toolchain: `xtensa-esp32-elf-{objdump,objcopy,nm}` from
  `~/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/`.
- Decoder-coverage oracle: `lp-xt-inst`'s `objdiff` binary
  (`cargo build -p lp-xt-inst --features objdiff --bin objdiff --release`).
- Scripts: `scripts/emu/xtensa-inventory/{census.py,sweep.py,README.md}`
  (committed this PR). One bug in `census.py`, inherited from a prior
  attempt's scratchpad and fixed here: this toolchain's `objcopy` (GNU objcopy
  2.43.1, crosstool-NG esp-14.2.0_20240906) **silently zeroes a lifted
  section's content while keeping its correct size** when section flags are
  attached directly to `--rename-section` (`.data=.text,alloc,load,readonly,code`).
  Splitting the flags into a separate `--set-section-flags` call fixes it —
  verified by hand before trusting the script's numbers (see §1 below).

  > **2026-09-10 follow-up correction (M1 P1, PR #660 → the `census.py`
  > rewrite that landed same-day).** The section-lifting approach itself —
  > not just the `objcopy` flags bug above — under-measured every ELF
  > artefact. Lifting a CODE section into its own one-section ELF drops the
  > Xtensa configuration (`e_flags`) the original ELF carries; `objdump` then
  > falls back to a config-less opcode table where a loose `lsi` entry beats
  > real MAC16 entries. Same bytes, same `objdump` binary, same decoder: the
  > classic ROM read **96.52 % / 60 mismatches** through this report's
  > `census.py`, and reads **99.96 % / 0 mismatches** when `objdiff` — widened
  > in PR #660 to iterate every `SHF_EXECINSTR` section itself — is run over
  > the whole ELF instead. The v3 image moved **98.48 % → 98.86 %** the same
  > way (both numbers post-#660's decoder fixes; this report's own *first*
  > numbers below, 98.21 % / 95.69 %, predate #660 entirely and are lower
  > again). `census.py` was rewritten to hand `objdiff` the artefact whole and
  > let it do the section iteration, rather than lifting sections itself; see
  > `scripts/emu/xtensa-inventory/README.md` and the Deviations section
  > below. The bootloader's raw-binary (`--base`) path was never an instance
  > of this — a raw blob has no Xtensa configuration to lose — and its number
  > is unchanged by the rewrite.

## 1. Exact decoder coverage (item 1)

Run: `scripts/emu/xtensa-inventory/census.py <artefact> --objdiff
lp-xt/lp-xt-inst/target/release/objdiff [--base 0x...]`.

| artefact | instructions | decoded | coverage | mismatched | unsupported |
|---|---:|---:|---:|---:|---:|
| v3 image (`fw-esp32v3`) | 721,521 | 708,596 | **98.21 %** | 6 | 12,925 |
| classic ROM (`esp32_rev300_rom.elf`) | 151,791 | 145,248 | **95.69 %** | 1 | 6,543 |
| IDF 2nd-stage bootloader (espflash-generated, no bootloader supplied) | 7,042 | 6,590 | **93.58 %** | 0 | 452 |

The v3 image total (721,521) is close to but not identical to the study's
proxy count (721,066, §0). The difference is methodological, not a
contradiction: the study's script walked raw `objdump -d` text directly;
`census.py` sums `objdiff`'s per-CODE-section counts (`.rwtext`, `.text`,
`.vectors`, each lifted into its own one-section ELF via `objcopy`) — a small
difference in how `.vectors`' garbled region is counted (102 lines here vs 97
in the study) accounts for most of the gap. **Deviation, reported not
corrected.**

### Decoder bug found: `lsi` misdecodes as `ssai`

Both the v3 image and the ROM contain real `lsi` instructions that
`lp-xt-inst::decode()` decodes to the **wrong instruction**, not "unsupported":

```
0x400d223b  bytes=404040
    objdump: lsi f4, a0, 0x100
    lp-xt:   ssai [Imm(0)] (len 3 vs 3)
```

Six sites in the v3 image, one in the ROM, all the same byte pattern
(`40 40 40` and near-relatives). This is a decoder table collision, not a
missing-mnemonic gap, and it was not something the study's mnemonic-name-table
proxy could have found (it only sees *string* mismatches, not wrong-but-typed
decodes). **Flagged for M1: this is a real bug to fix, not merely coverage to
add.**

### Top-20 unsupported mnemonics — v3 image (157 kinds, 12,925 sites total)

```
10057  lsi          579  f64cmph      241  xorb         239  andb
  218  s32c1i       185  wsr.scompare1 121  rsr.prid     114  andbc
  113  wsr.ps        92  f64addc       82  rsil          79  orb
   75  f64iter       63  s32e          60  s32ri         59  l32e
   50  f64rnd        46  l32ai         38  f64sexp       28  f64norm
```

### Top-20 unsupported mnemonics — classic ROM (145 kinds, 6,543 sites total)

```
4890  lsi          272  f64cmph      171  andb         114  orb
  98  s32ri         71  l32ai         62  l32e          59  s32c1i
  54  f64iter       45  f64addc       42  orbc          41  andbc
  31  xorb          29  mula.da.ll.lddec 27 f64subc      26  clamps
  26  s32e          23  rsil          19  mula.da.hl.lddec 19 wsr.ps
```

### Bootloader — full unsupported list (23 kinds, 452 sites total)

```
314  lsi           76  f64cmph        7  andb           6  s32ri
  4  f64norm        4  loop           3  rer            3  f64iter
  3  orb            3  rsr.ccount     3  xorb           2  any4
  2  l32e           2  ret.n          2  andbc          2  break.n
  2  witlb          1  break          1  clamps         1  f64cmpl
  1  f64subc        1  mula.dd.ll.ldinc 1 mula.dd.ll.lddec
```

Bootloader segments (per `esptool image-info`, ESP32 Image v1, entry
`0x4008064c`, 4 segments): segment 0 (DRAM data, skipped — not code), segment 1
(CACHE_APP code @ `0x40078000`, 15,576 bytes), segment 2 (IRAM @ `0x40080400`,
4 bytes — one `lsx` instruction), segment 3 (IRAM @ `0x40080404`, 3,876
bytes). Command used:

```
espflash save-image --chip esp32 --merge fw-esp32v3.elf merged.bin
# partition table magic 0xaa50 found at offset 0x8000 -> bootloader = [0x1000, 0x8000)
dd if=merged.bin of=bootloader.bin bs=1 skip=4096 count=28672
esptool --chip esp32 image-info bootloader.bin   # segment table
```

## 2. Literal-pool sweep collisions (item 2)

Run: `scripts/emu/xtensa-inventory/sweep.py <v3 elf>`.

A symbol-seeded, width-following walk (the discovery algorithm the study
recommends) over the v3 image's code sections, checking whether each decoded
instruction address falls inside its enclosing ELF symbol's `[start,
start+size)` and outside `.literal`/`.rodata`:

- **726,726** decoded lines walked (this total includes the `.vectors`
  region's `.byte` data-directive lines — a naive sweep with no symbol
  boundaries would walk through those too, so they belong in this count).
- **30,222 instructions / 72,093 bytes (4.1587 %)** fall outside their
  enclosing symbol or inside `.literal`/`.rodata` — the literal-pool-collision
  rate a naive discovery sweep would hit.

Worst offending symbols (collision bytes attributed to the symbol whose sweep
overran):

```
12,931  <NativeJitEngine as SharedEngine>::free_sample_points  (lp_gfx_lpvm)
10,276  <ProjectManager>::get_project                          (lpa_server)
10,168  esp_rom_spiflash_read
 9,070  <SlotReader as JsonSyntaxSource>::invalid_discriminator_value (lpc_model)
 8,887  materialize_node_text_asset                            (lpc_engine)
 8,493  fixed_array_from_base                                  (lps_glsl::hir)
 5,623  <LpFsMemory as LpFs>::is_dir                            (lpfs)
 2,641  <before any symbol>
```

Everything past those seven is 3 bytes (one instruction) — small ISR/interrupt
plumbing and the `esp_hal`/`esp_rtos` scheduler internals, negligible.

**Note on the sweep script's own bug, fixed before trusting these numbers**:
the first draft's hex-byte regex assumed space-separated byte pairs
(`"2d f4 21"`); this toolchain's `objdump -d` actually prints them
concatenated with no spaces (`"1c4012"` is one 3-byte instruction). The
regex matched almost nothing on `.text`/`.rwtext` and produced a bogus
"5,642 total instructions" (coincidentally equal to the study's `.vectors`
garbled-line count) until fixed to split on tabs and count hex-digit pairs,
the same way `objdiff.rs::parse_line` does.

## 3. `wsr.fcr` values (item 3)

The mnemonic is actually **`wur.fcr`** (a user-register write), not
`wsr.fcr` — FCR is accessed via `RUR`/`WUR`, not `RSR`/`WSR`, matching
`lp-xt-inst/src/sr.rs`'s own model. There is exactly **one** `rur.fcr` (read,
`0x4008092e`, inside `save_context`) and **one** `wur.fcr` (write,
`0x40080a1f`, inside `restore_context`) in the v3 image — a context-switch
save/restore pair, not two independent application writes.

```
40080a1c: l32i   a3, a1, 144
40080a1f: wur.fcr a3
```

The write is **not a compile-time immediate**: `a3` is loaded from offset 144
of the per-task saved-context struct immediately before the `wur.fcr`, so it
round-trips whatever FCR value `save_context` most recently saved for that
task, not a fixed constant. I found no code anywhere in the image — nor in
this repo's own `lp-fw`/`lp-xt` sources (`grep -rl FCR` finds nothing under
`lp-fw/`) — that writes FCR via an immediate. **I could not determine,
within the ~30-minute budget, whether the saved context slot is ever
populated with a non-zero `FCR.RM` by any code path** — that is a
whole-program data-flow question a disassembly read cannot answer. The
best-supported claim: FCR's hardware reset value is 0 (round-nearest) per the
Xtensa ISA, and no static evidence in the image contradicts that.

## 4. Window-wrap frequency floor (item 4)

**Not added to `lp-xt-emu/src/executor/window.rs`** — `spill_frame` already
fires a `TraceEvent::WindowSpill` event unconditionally on every window
overflow, and `Tracer`/`TraceEvent` are already public. A throwaway,
**uncommitted** example (`lp-xt/lp-xt-elf/examples/xt-spill-count.rs`,
deleted before this PR's final commit) implemented `Tracer` as a counter and
ran the fixture corpus:

| fixture | instructions | window spills | spills / 1M instructions |
|---|---:|---:|---:|
| `bench_loop` (arg=50000 — the speed probe, "24-deep recursion") | 111,602,417 | 1 | **0.0090** |
| `ackermann` | 292,394 | 20,601 | **70,456.3** |
| `fib_rec` | 137,527 | 672 | **4,886.3** |
| `array_sum` | 2,663 | 1 | 375.5 (not meaningful — see below) |
| `call_conv` | 2,417 | 1 | 413.7 (not meaningful) |
| `state_machine` | 1,930 | 1 | 518.1 (not meaningful) |
| `sort_insertion` | 4,004 | 1 | 249.8 (not meaningful) |

The four short fixtures each hit exactly **one** spill (a fixed startup cost),
so their "per 1M" figures are sample-size artefacts, not rates — flagged
rather than presented as real numbers.

**This is a floor, not the firmware's profile (P1b — labelled, not
estimated).** The finding that matters: window-overflow frequency swings
across **four orders of magnitude** (0.009 to 70,456 per 1M instructions)
purely as a function of call-recursion *shape* — `bench_loop`'s flat
64-element loop with one non-inlined call per element barely ever overflows
the 64-register ring, while `ackermann`'s deep non-tail recursion overflows
it constantly. This directly answers study §4.1's open question in kind, if
not in a single number: **whether overflow is hot or cold for real firmware
depends entirely on the firmware's own call graph**, and only the v3 machine
(not yet built) can measure that. Neither number above should be read as an
estimate of the shipped firmware's own rate.

## 5. SRAM1 I-bus alias use (item 5)

Range checked: `0x400A_0000..0x400C_0000`.

- **v3 image**: 0 ELF symbols in range. A raw `grep` over the disassembly for
  address-shaped operands in range *does* find ~190 hits (`call4`/`l32r`
  targets), but every one of them disassembles from an instruction address
  that is **outside any real ELF symbol's bounds** — i.e. they are exactly
  the literal-pool-misdecode artefact §2 measures, not real code. Filtering
  to instructions whose own address falls inside a real, sized symbol (the
  same bound §2's sweep uses) leaves **0** references.
- **classic ROM**: 0 ELF symbols in range, 0 in-symbol-bounds references
  (same method).
- **bootloader**: no symbol table available (it is a stripped,
  espflash-merged binary). A naive linear disassembly of its 3 code segments
  turns up ~9 raw address-shaped hits in range, but the segments together
  span only `0x40078000`–`~0x40081328` (≈37 KB), nowhere near able to
  legitimately reach `0x400A_0000` by a real call or load — so these are very
  likely the same decode-noise artefact. **I could not validate this as
  rigorously as the other two artefacts** (no symbol table to bound against)
  — reported as inconclusive-but-leaning-none, not a clean answer.

**Net: no evidence any of the three artefacts reference the SRAM1 I-bus
alias.** Supports the plan's Discovery-summary default (leave it unmapped).

## 6. Toolchain checks (item 6)

- `espflash save-image --chip esp32 --merge` **works with no bootloader
  supplied** — confirmed: produced a 4,194,304-byte merged image (app/partition
  2,196,096/4,128,768 bytes, 53.19 %). espflash generates its own bootloader.
- **Bootloader version string**, extracted from the raw null-terminated bytes
  (not `strings`' line-wrapping, to get it exact): `v5.1-beta1-378-gea5e0ff298-dirt`
  — reported **exactly as found**, including the apparent truncation
  (`-dirt`, not `-dirty`); not "corrected" to the expected spelling. Full
  banner: `ESP-IDF v5.1-beta1-378-gea5e0ff298-dirt 2nd stage bootloader`,
  `compile time Jun  7 2023 07:48:23`, `Multicore bootloader`.
- `esp-rom-elfs` **20260528** ships both required ELFs, verified by
  extracting a fresh copy of the pinned tarball and comparing sha256 against
  the already-present scratchpad copy (byte-identical):
  - `esp32_rev300_rom.elf` — sha256
    `920b70635440517866aab2230964a570d2cf2b676658d93c52fbac108c1cca31`
  - `esp32s3_rev0_rom.elf` — sha256
    `c0ce0f338d1de1bdc6efbef1591779a2a42c1ab7d759d3c6ae8ae63a7dd34cfd`
  - Tarball itself (`esp-rom-elfs-20260528.tar.gz`, 4,902,355 bytes) —
    sha256 `caa463d3cbef2430a5a35847c1d9f2f152403b17a802050927ff60c8da54fe46`,
    matching both the scratchpad copy and a fresh fetch of the release's own
    checksum file from GitHub (network-reachable in this environment).
- **CI toolchain path** — confirmed by *reading* `.github/workflows/pre-merge.yml`
  (job `firmware-esp32v3`, line 571) and `.github/actions/xtensa-toolchain/action.yml`:
  that job installs the Xtensa Rust toolchain (pinned `1.95.0.0`) with
  `buildtargets: esp32`, which bundles `xtensa-esp32-elf-objdump` on `PATH`
  via `espup`, the same mechanism as the local install used throughout this
  report. **This is a config read, not an executed CI run** — the brief
  says not to watch CI, and I did not trigger one.

## 7. Special-register and exception surface — table for M1 P3 (item 7)

From the v3 image census (60 registers, 562 sites, `objdiff`-exact),
cross-referenced against the enclosing ELF symbol for every site (raw
`objdump`+`nm`, demangled with `rustfilt` — **not** `objdiff`-validated, so
treat the "touched by" column as a good lead, not a certified fact). Full
data: `sr-function-map2.txt`/`sr-table-final.txt`-equivalent, regenerable
from `census.py --json` plus a symbol-bounds pass (not committed — see
`scripts/emu/xtensa-inventory/README.md` for the reproducible commands; the
enclosing-symbol cross-reference itself is a ~15-line addition on top of
`sweep.py`'s existing symbol-table code, left for M1 to fold in if wanted).

| register | sites | read | write | representative touching code |
|---|---:|---:|---:|---|
| `scompare1` | 186 | 1 | 185 | `esp_sync::NonReentrantMutex<T>::with` (many monomorphizations — every critical-section entry/exit), `embassy::Executor::run_inner` (+76 more) |
| `prid` | 121 | 121 | 0 | `EspDefaultHandler`, same `NonReentrantMutex::with` monomorphizations (+40 more) |
| `ps` | 118 | 5 | 113 | `.HandleException` [unsized], `.RestoreContext` [unsized], `_AllocAException` [unsized], `embassy::Executor::run_inner` (+48 more) |
| `intenable` | 10 | 9 | 7 | `Reset` [unsized], `esp_hal::soc::...cpu_control::start_core1_init`, `esp_hal::interrupt::xtensa::init_vectoring`, `fw_esp32v3::serial::io_task::poll_rx_into` (+5 more) |
| `epc1` | 6 | 2 | 4 | `.RestoreContext` [unsized], `SlotReader::invalid_discriminator_value` [unsized], `__naked_double_exception`, `__naked_user_exception` (+1 more) |
| `dbreaka0` | 5 | 0 | 5 | `fw_esp32v3::boot_firmware`, `esp_hal::debugger::set_stack_watchpoint`, `esp_rtos::task::arch_specific::cross_core_yield_handler` |
| `dbreakc0` | 5 | 0 | 5 | same three as `dbreaka0` |
| `exccause` | 4 | 4 | 0 | `_KernelExceptionVector`, `_UserExceptionVector`, `__naked_double_exception`, `__naked_user_exception` (all unsized labels) |
| `intclear` | 4 | 0 | 4 | `__level_1_interrupt`, `__level_3_interrupt` |
| `lbeg` | 4 | 4 | 0 | `LpFsMemory::is_dir` [unsized, false lead — see caveat below], `idle_hook_fn`, `save_context`/`restore_context` [unsized] |
| `mmid` | 4 | 0 | 4 | context-switch path (unsized labels) |
| `threadptr` | 4 | 3 | 1 | `fw_esp32v3::boot_firmware`, `esp_rtos::embassy::ThreadFlag::new`, `esp_rtos::task::arch_specific::cross_core_yield_handler` |
| `ccompare0` | 3 | 1 | 3 | `esp_hal::soc::...cpu_control::start_core1_init` |
| `cpenable` | 3 | 2 | 1 | `lp_fpu_arm_cpenable` |
| `eps4` | 3 | 1 | 2 | `esp_hal::soc::...cpu_control::start_core1_init` |
| `excsave1` | 3 | 2 | 1 | context-switch path (unsized labels) |
| `ibreaka1` | 3 | — | — | debugger/watchpoint path |
| `interrupt` | 3 | 3 | 0 | `__level_1_interrupt`, `__level_2_interrupt`, `__level_3_interrupt` |
| `windowbase` | 3 | 2 | 2 | window-overflow/underflow handlers (unsized labels) |
| `br`, `sar`, `acclo`, `acchi`, `m0..m3`, `f64r_lo/hi`, `f64s`, `fcr`, `fsr` | 1–2 each | context-switch save/restore round-trip (see §3) | | `save_context`/`restore_context` |
| `epc2..epc7`, `eps2..eps7`, `excsave2..excsave7` | 1–2 each | exception-level bank save/restore | | the naked exception vectors |
| `vecbase`, `ccompare1`, `ccompare2`, `ibreakenable` | 2 each | 0 | 2 | `esp_hal::soc::...cpu_control::start_core1_init` (core-1 boot) |
| `depc`, `lcount`, `lend`, `windowstart`, `dbreaka1` | 1 each | mixed | | context-switch / naked-exception path |

**Caveat on the "touched by" column**: a large fraction of the highest-traffic
registers (`ps`, `scompare1`, `prid`, most of the exception-bank registers)
are accessed inside **unsized raw-assembly labels** (`.HandleException`,
`.RestoreContext`, `_AllocAException`, `__naked_*`, `save_context`,
`restore_context`, `Reset`, `_KernelExceptionVector`, `_UserExceptionVector`,
`__level_N_interrupt`) — `xtensa-lx-rt`'s hand-written exception vectors and
context-switch code, which have no `.size` directive in the ELF symbol table.
The nearest-preceding-label fallback used to attribute them can be wrong when
two unsized labels are adjacent with no sized symbol between them (e.g.
`lbeg`'s `LpFsMemory::is_dir` hit is very likely a mis-attribution — that
function is almost certainly unrelated to loop registers, and the true site
is the nearby `save_context`/`idle_hook_fn` region). **This is itself a
finding for M1**: the highest-traffic special registers live almost entirely
in `xtensa-lx-rt`'s raw asm, not in typed Rust, so M1's own census (which
will have `lp-xt-inst`/`lp-xt-emu` source, not just a disassembly) should
re-derive this table from the vendored `xtensa-lx-rt` source rather than
trust this best-effort symbol attribution for the unsized regions.

## Deviations

1. **`census.py`'s `objcopy` section-lift bug** (silently zeroes content when
   flags ride on `--rename-section`) — found and fixed before any measurement
   was trusted; documented in the script itself.
   **2026-09-10 update**: the section-lift *approach* itself, not just this
   flags bug, turned out to under-measure every ELF artefact — see the
   dated note in Provenance above. `census.py` was rewritten same-day to run
   `objdiff` once over each artefact whole (PR #660 widened `objdiff` to
   iterate every executable section itself, closing the reason this script
   used to lift sections at all). ROM: 96.52 % (60 mismatches, section-lifted)
   → 99.96 % (0 mismatches, whole). v3 image: 98.48 % → 98.86 % (both
   post-#660 decoder fixes). Bootloader (`--base`, raw binary): unchanged —
   that path was never lifting a section out of a larger ELF, so it never had
   this bug.
2. **`sweep.py`'s hex-byte regex bug** (assumed space-separated byte pairs;
   this toolchain concatenates them) — found and fixed the same way,
   verified against `objdiff.rs`'s own parsing convention.
3. **Total instruction count** (721,521 vs the study's 721,066) — a ~0.06 %
   difference from counting `.vectors`' garbled region slightly differently
   between the study's raw-text proxy and `objdiff`'s per-section sum.
   Reported, not reconciled further (out of scope to chase a rounding
   difference in a superseded proxy number).
4. **Report filename**: the brief names
   `docs/reports/2026-09-1x-xtensa-firmware-isa-inventory.md` (literal `1x`).
   Repo convention (`docs/reports/*.md`) is always a real date; used
   `2026-09-10-xtensa-firmware-isa-inventory.md`.
5. **Branch checkout**: `claude/xt-m0-isa-inventory` was already checked out
   by another live worktree (a prior attempt, uncommitted, at
   `6f6aa51bc`) — this worktree could not check out that branch name
   locally. Worked on a differently-named local branch
   (`m0-isa-inventory-local`) and pushed it to `origin/claude/xt-m0-isa-inventory`
   via `git push origin m0-isa-inventory-local:claude/xt-m0-isa-inventory`,
   satisfying the "push to branch `claude/xt-m0-isa-inventory`" instruction
   without touching the other worktree.
6. **Item 4's counter was not added to `window.rs`** as the brief's literal
   instruction says — `spill_frame` already fires a public `TraceEvent`
   on every spill, so a throwaway `Tracer` implementation in an uncommitted
   example measured the same thing with zero product-code edits, which is
   strictly safer than the instructed approach and produces an identical
   number. Deviation from the letter of the instruction, not its intent.

## Surprises the M1/M3 briefs should carry

- The `lsi`→`ssai` **misdecode** (§1) — a real bug, not a coverage gap. M1
  should add a regression fixture for it specifically, not just widen
  coverage.
- `wsr.fcr` in the brief's own wording is a naming slip — it's `wur.fcr`
  (user register, not special register), and the only write in the image is
  a context-switch round-trip, not an application-level rounding-mode
  choice (§3).
- Window-overflow frequency is not a single number at all — it spans four
  orders of magnitude by call shape alone (§4). Any later "is this hot"
  claim needs the v3 machine's real call graph, not a fixture.
- The special-register surface is dominated by **unsized raw-assembly
  labels** in `xtensa-lx-rt` (§7) — M1's own SR census should read that
  crate's source directly rather than trust disassembly-based symbol
  attribution for the highest-traffic registers.
- `census.py` and `sweep.py` both shipped from the prior attempt's
  scratchpad with the same class of bug: an assumption about this specific
  `objdump`/`objcopy` build's text format that turned out to be wrong.
  Anyone reusing throwaway scripts from a scratchpad should re-verify their
  numbers against a known ground truth (here: the study's own figures)
  before trusting them, exactly as this milestone's brief instructed.
