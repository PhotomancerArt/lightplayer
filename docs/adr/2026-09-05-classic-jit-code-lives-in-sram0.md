# ADR: Classic ESP32 JIT code lives in SRAM0, and the SRAM1 tail is heap

- **Status:** Accepted
- **Date:** 2026-09-05
- **Deciders:** Photomancer
- **Plan:** `lp2025/2026-09-05-1635-classic-ram-track-b-jit-sram0` (PR #522)
- **Supersedes:** the SRAM1 placement paragraphs of
  `2026-08-01-esp32v3-flash-budget.md` ("RAM is the real constraint")
- **Superseded by:** None
- **Related:** `docs/reports/2026-09-04-classic-ram-budget.md` (lever 1);
  track A, `2026-09-05-1635-classic-ram-track-a-stack-rom-tables` (the ROM
  stack reclaim whose chunk fuses with this tail)

## Context

The classic ESP32 has three internal SRAMs. SRAM2 (`dram_seg`) is where
`.data`, `.bss`, `.stack` and the 110 KB heap arena fight over 192 KB. SRAM1
(128 KB) is dual-mapped — byte-addressable on the data bus, and fetchable
through a word-mirrored instruction-bus alias — so it can be heap *or* code.
SRAM0 (128 KB of IRAM at `0x4008_0000..0x400A_0000`) hangs off the
instruction bus only: it holds `.vectors` and `.rwtext` (~15 KB), and the
other ~112 KB sat idle because nothing that needs byte access can live there.

Since the 2026-07-28 bring-up the on-device JIT installed shader code into a
fixed region of SRAM1 (`CodeRegion::ESP32_DEFAULT`, D-bus `0x3FFE_8000`,
92 KiB → 32 KiB → 24 KiB as the corpus test measured it down) through the
word-mirrored D-bus walk, and the rest of SRAM1's tail was the heap's second
region (72 KiB). The 2026-09-04 RAM budget report ranked "move the JIT
region to SRAM0" as lever 1: the JIT is the one consumer that wants exactly
what SRAM0 offers (word-aligned writes, instruction fetch), and every byte of
SRAM1 it occupied was a byte the heap could have had on a chip whose binding
constraint is heap residency.

What nobody had measured on this chip with this toolchain: whether code
written word by word into SRAM0 executes, whether a barrier is needed, where
the linker's `.rwtext` actually ends, and whether byte access really faults.

## Decision

1. **The JIT code region is 64 KiB of SRAM0 at `0x4008_8000..0x4009_8000`.**
   `CodeRegion` gains a `Placement`: `Sram0 { ibus_base }` (no D-bus view —
   the write address *is* the I-bus address, aligned word stores, the walk
   ascends) beside the existing `Sram1Mirrored { dbus_base }`. The old SRAM1
   region survives as `ESP32_SRAM1_LEGACY` so the mirrored path stays tested.
   `write_addr()` replaces `dbus_write_addr()`; `install`, `CodeArena`,
   `global` and the `[JIT]` telemetry are unchanged.
2. **The heap gets the whole SRAM1 tail.** `reclaimable_heap_span()` keeps
   its name and meaning ("the SRAM1 bytes the JIT does not use") and under
   SRAM0 placement returns `(0x3FFE_8000, 0x1_8000)`: 98,304 B, 24,576 more
   than before. Const-asserts pin the default region, the legacy region, the
   heap span and the IRAM containment.
3. **Boot asserts the link layout.** `.rwtext` shares SRAM0 with the region
   and grows upward from `0x4008_0400`; the compiler cannot see where it
   ends, so `fw-esp32v3` refuses to install the arena unless the decoded
   `.rwtext` end is at or below the region base (panic with both numbers —
   a link-layout bug, not a runtime condition). Today's app image ends at
   `0x4008_3E00`, 16,896 B of headroom.
4. **The emulator models SRAM0 faithfully.** `lp-xt-emu`'s
   `BoardProfile::esp32()` is now an identity-aliased, **word-only** window
   (`AccessRule::WordOnly`: sub-word or misaligned loads/stores fault with
   EXCCAUSE 3, fetch unaffected); `esp32_sram1_legacy()` keeps the mirror.
   `lpvm-native`'s parity test checks both (region, profile) pairs word by
   word.

### Measured facts (dig2go, `/dev/cu.wchusbserial120`, `test_sram0_exec`, 2026-09-05)

Verbatim from the capture (two boots — the byte-access fault resets the chip
and the second boot reports it from an RTC-RAM marker):

```
[SRAM0] rwtext_end: _rwtext_len=0x400825b8 iram_origin=0x40080400 | if_plain_length: len=1074275768 end=0x801029b8 | if_section_relative: len=4316 end=0x400814dc | region_base=0x40088000 (compare against readelf -S .rwtext)
[SRAM0] rwtext_probe_fn: a #[ram] fn lives at 0x40080ae4 (in [0x40080400, end)) consistent with section_relative
[SRAM0] word_rw[region]: 0x40088000+0x10000 words=16384 mismatches=0 PASS
[SRAM0] word_rw[spare_above]: 0x40098000+0x8000 words=8192 mismatches=0 PASS
[SRAM0] encoding: template@0x400d574c bytes=[36, 41, 00, 22, a5, a5, 1d, f0] table=[36, 41, 00, 22, a5, a5, 1d, f0] template()=0x5a5 MATCH
[SRAM0] exec: called 0x40088000 on PRO core -> 0x5a5 (expected 0x5a5) PASS
[SRAM0] exec_top: called 0x40097ff8 -> 0x321 (expected 0x321) PASS
[SRAM0] barrier[none]: iterations=1000 stale=0 other=0 PASS
[SRAM0] barrier[fence]: iterations=1000 stale=0 other=0 PASS
[SRAM0] barrier[fence+isync]: iterations=1000 stale=0 other=0 PASS
[SRAM0] app_core: SKIPPED — the JIT compiles and executes on the PRO core only; the APP core runs the RMT ISR and never fetches JIT code
[SRAM0] byte_access: attempting s8i/l8ui at 0x40088000 — a LoadStoreError (EXCCAUSE 3) reset after this line IS the expected result; the next boot reports it
Exception occurred on ProCpu 'LoadStoreError'   (EXCCAUSE: 3, EXCVADDR: 0x40088000)
[SRAM0] byte_access: FAULTED on the previous boot (this boot's reset_reason=Some(CoreSw); the exception dump printed before this boot is the record) — SRAM0 is word-only
```

Read as facts:

| fact | result |
|---|---|
| aligned word stores across `0x4008_8000..0x400A_0000` (96 KiB) | all 24,576 words read back, 0 mismatches |
| byte store at `0x4008_8000` | `LoadStoreError`, EXCCAUSE 3, EXCVADDR `0x4008_8000` — SRAM0 is word-only |
| word-written `entry a1,32; movi a2,imm; retw.n` at region base and at `0x4009_7FF8` | executes on the PRO core, returns the constant |
| barrier after the writes, 1,000 rewrite-then-call iterations each | none: 0 stale; `fence(SeqCst)` (= `memw`): 0 stale; fence + `isync`: 0 stale — **no barrier needed**; `DeviceCodeSink` stays as it was |
| APP core | not measured, by design: the JIT executes on the PRO core only |
| `_rwtext_len` | **not a length**: esp-hal's `rwtext.x` assigns it inside the `.rwtext.wifi` output section, and GNU ld resolves it section-relative, so its address = `.rwtext` end + length = `ORIGIN + 2·len`. Decode `len = (sym − 0x4008_0400) / 2`, `end = 0x4008_0400 + len`, which reproduced `readelf -S` exactly (harness: `0x4008_14DC`; app image: `0x4008_3E00` with `_rwtext_len = 0x4008_7800`) |

A fact measured by accident on the first run: a byte load from the
flash-mapped `.text` window (`0x400D_xxxx`) faults the same way — the IROM
window is on the instruction bus too. The probe reads its template function
with word loads.

### On the device after the change

```
[INIT] chip=esp32 arch=xtensa heap=112640+98304 (dram_seg arena + SRAM1 tail)
[INIT] JIT code region: sram0 ibus 0x40088000..0x40098000 (64 KiB, placed, rwtext_end=0x40083e00); SRAM1 heap tail dbus 0x3ffe8000+98304 B
[MEM] free=194892 used=16052 largest_free=98302 retry_saves=0
[JIT] used=0 peak=0 cap=65536 spans=0 peak_spans=0 allocs=0 frees=0 fails=0 largest_free=65536
```

Idle heap total 210,944 B (was 186,368). The image's RAM layout is unchanged
(`.data` 22,140 / `.bss` 138,864 / `.stack` 35,600 B; `.rwtext` +16 B for the
boot guard's floor function) — track A measured the main stack at ~2.9 KB of
headroom, and this change adds nothing to it. The resident project
(`zook-dome`, 1,500 LEDs over five wires) compiled into SRAM0
(`[JIT] used=2144 … spans=1`) and rendered.

The bit-exact gate: `examples/shader-oracle` auto-loaded at boot, compiled
into SRAM0 (`[JIT] used=2444` — exactly the corpus figure), and its lit frame
dump equals the host wasmtime oracle byte for byte (384/384 hex chars,
`[OUT] dump frame=31 … crc=0x55772254`). The walk script itself needed two
detours on this board, both pre-existing: its backgrounded `espflash flash`
stalls on the classic (a foreground write under `script` goes through), and
with `zook-dome` resident the upload's stop-all-then-load hits the open
`2026-09-04-unload-leaves-classic-unloadable-until-power-cycle` defect
(largest free block after unload 57,792 B — better than the 39,655 B measured
before this change, still under the 64 KiB gate); the project files land
regardless, so the oracle was made the startup project for one boot and
zook-dome restored afterwards.

Track A's rebase hook: `CodeRegion::sram1_claim_base()` — the lowest SRAM1
D-bus address the JIT or its heap span claims, `0x3FFE_8000` under either
placement of the two named regions — is what its ROM-APP-stack chunk ends
at (it used to read the region's `dbus_base`, which an SRAM0 region does
not have).

## Consequences

- Heap: +24,576 B now (region 1 = `0x3FFE_8000..0x4000_0000`, 98,304 B, a
  single block larger than the arena's largest). With track A's ROM-stack
  chunk, which ends at `reclaimable_heap_span().0`, the SRAM1 tail becomes
  one `0x3FFE_4350..0x4000_0000` region (113,840 B) with no change here.
- JIT capacity: 64 KiB (was 24), with 32 KiB of SRAM0 spare above for growth.
  It is generous because it is free, not because the corpus asked: the
  keep-last-good peak model is still 16,776 B and the corpus test still
  guards it.
- `.rwtext` growth ceiling: `0x4008_8000`. An image whose IRAM code grows past
  it panics at boot with both numbers; the fix is to move the region (one
  constant, plus the emulator profile and the parity test) or trim IRAM code.
- The `_rwtext_len` decode is a dependency on ld's section-relative
  treatment. The boot guard refuses a decode that lands below a
  `.rwtext`-placed function, so a linker that changes the symbol's meaning
  stops the boot rather than passing vacuously; re-measure with
  `just fwtest-sram0-esp32v3` when esp-hal or the toolchain moves.
- The emulator's classic profile no longer models SRAM1 at all; the mirrored
  window lives on in `esp32_sram1_legacy()` for the legacy install-walk test.
- `lpc-shared`'s classic backtrace window already spanned both SRAMs, so a
  fault inside a JIT'd shader is still attributed; its comment now says
  SRAM0.

## Alternatives Considered

- **Stay in SRAM1** (24 KiB region, 72 KiB heap tail). Rejected: it spends
  byte-addressable memory on the one thing that does not need it, on the
  chip where the RAM budget report measured heap residency as the binding
  constraint.
- **SRAM0 as heap instead.** Rejected by the same measurement that enabled
  this: byte access faults (EXCCAUSE 3). A word-only data pool is lever 7 of
  the report, parked.
- **Take the region base from a linker symbol** (`_rwtext_len`, or an
  `.rwtext.zzz_end` sentinel) so it floats above `.rwtext`. Rejected: the
  emulator profile and the const-asserts want a constant, LTO does not
  guarantee sentinel ordering, and `_rwtext_len`'s semantics turned out to
  be a linker artefact — safer to fix the base and assert at boot.
- **A firmware-side `CodeSink` with `isync`.** Not needed: measured 0 stale
  results with no barrier at all; `lpvm-native` keeps its posture of never
  enabling `asm_experimental_arch`.

## Follow-ups

- Track A's rebase: keep `reclaimable_heap_span()`'s name and meaning; its
  ROM-APP-stack chunk ends at `.0` of that span and fuses automatically.
- If `.rwtext` ever approaches `0x4008_8000` (the app image is at
  `0x4008_3E00` today), move the region up: `CodeRegion::ESP32_DEFAULT`,
  `BoardProfile::esp32()`, and the two pinned const-asserts, in one commit.
