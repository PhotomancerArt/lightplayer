---
status: fixed
found: 2026-09-10      # how: live-debugging (the classic emulator, M4 P1 bring-up)
fixed: this change
area: lp-emu/esp/lp-emu-esp32v3 (machine.rs `service_app_core_start`)
class: stand-in-divergence
related:
  - docs/adr/2026-08-04-rmt-isr-on-app-core.md
  - lp-emu/esp/lp-emu-esp32v3/tests/dual_core.rs
  - lp-emu/esp/lp-emu-esp32v3/tests/boot_idle.rs
  - lp-fw/fw-esp32v3/src/tests/appcore_rom_path.rs   # the bench harness that ruled, PR #695
  - planning lp2025/2026-09-10-0021-xtensa-emulator/m4/p1-two-cores-run.md
---
# The emulator ran the ROM's reset path on the APP core

**This entry was first filed against the firmware, and that was wrong.** It
is kept at its original found-date and rewritten in place, because the
mis-framing is half the lesson: an emulator that disagrees with silicon is
lying until proven otherwise, and this one was believed for a day.

**The firmware is not at fault.** `fw-esp32v3` registers heap region 0
before `start_app_core_isr` and that is safe on this part. Nothing in
`main.rs`, `lpvm-native`'s span constants or esp-hal changed to fix this,
and nothing should.

## Symptom

On the classic emulator with core 1 released (M4 P1, PR #692), the shipped
`fw-esp32v3` image died about 30,000 cycles after `start_app_core_isr`
released the core — on both boot paths, and on both a first (formatting) and
a second (mounting) boot of the same flash:

```
[INFO  lp_emu_esp32v3::machine] core 1: released by DPORT at cycle 3228994 — reset to _ResetVector at clock 3228995; appcpu_boot_addr = 0x400d5d34 (…start_core1_init…app_core_main…)
[WARN  lp_emu_esp_common::bus] UNMAPPED read4 at 0x00000008 from pc=0x401190b2
STRICT BUS STOP
  pc      = 0x401190b2 (…lp_fs…LpFsFlash…LpFs9read_file…+0x1e)
  cycle   = 3258896 (13578 us emulated)
  access  = Read Word at 0x00000008
```

A read through a null vtable pointer inside `LpFs::read_file`. Sixteen image
gates were red on it. **The desk board booted the same image bytes
dual-core, twice, and never crashed** (L0/L1, PR #690: `[INIT] RMT ISR on
APP core`). That disagreement is what this entry is about.

## Root cause

The emulator modelled DPORT's `appcpu_resetting` pulse as **a reset that
puts the APP core through the mask ROM**: on release, slot 1 came back as a
fresh hart at `_ResetVector` (`0x4000_0400`) and ran the vendored rev300
ROM's full reset path — `_start`, the unpack and bss tables, ROM `main` —
before reaching the address esp-hal had written into `appcpu_boot_addr`.

That model is internally coherent, and the M4 P1b investigation (X34)
confirmed every one of its parts is faithful to the ROM bytes: core 1 reads
`appcpu_ctrl_d = 0x400d5d34` at `0x40000462` with bit 31 clear, so the ROM's
fast-boot branch is not taken; the `flag = 1` unpack and bss entries cover
`0x3ffe0440..0x3ffe0858` (`.data_xtos_pro`) and `0x3ffe0860..0x3ffe1320`
(`.bss_xtos_pro`) and carry no per-core selection; the ROM never references
its `_app` sections at all. ESP-IDF's own `cpu_start.c` even says starting
the APP CPU lets its ROM corrupt that memory.

So the ROM code, read honestly, says the tables run. **The part does not run
them.** The one thing no disassembly can answer is what the
`appcpu_resetting` pulse actually does to a core that is already sitting in
ROM `main`'s `appcpu_boot_addr` poll, and that is precisely the question the
model had answered by assumption.

The consequence was 3,808 bytes of live allocator memory rewritten on every
release. `fw-esp32v3` hands `0x3ffe0440..0x3ffe3f20` to esp-alloc as heap
region 0 first, esp-alloc is first-fit in registration order, so the boot
residents — the `LpFs` objects among them — land at `0x3ffe0440+` and were
exactly what the emulator's phantom ROM path zeroed.

## The bench (lab task L2, PR #695) — verbatim

`test_appcore_rom_path`, a cargo feature on `fw-esp32v3`, fills the head of
heap region 0 with `0xA5`, calls the **product's** `start_app_core_isr`,
waits on the real `ISR_ON_APP_CORE` bind, and scans. On the DOM-Z-102, two
sittings, identical:

```
[APPCORE-CANARY] canary 0x3ffe0440..0x3ffe1440 (4096 B) fill=0xa5 region0=0x3ffe0440..0x3ffe3f20 (15072 B)
[APPCORE-CANARY] coverage rom_span=0x3ffe0440..0x3ffe1320 (3808 B) covered=3808 B overlaps=yes
[APPCORE-CANARY] pre_start w[0x3ffe0448]=0xa5a5a5a5 w[0x3ffe0548]=0xa5a5a5a5 w[0x3ffe0860]=0xa5a5a5a5 w[0x3ffe09a8]=0xa5a5a5a5
[APPCORE-CANARY] start_app_core_isr bound=true wait_us=210
[APPCORE-CANARY] post_start w[0x3ffe0448]=0xa5a5a5a5 w[0x3ffe0548]=0xa5a5a5a5 w[0x3ffe0860]=0xa5a5a5a5 w[0x3ffe09a8]=0xa5a5a5a5
[APPCORE-CANARY] scan changed=0 ranges=0 handler_words=0 data_xtos_pro=0 bss_xtos_pro=0 outside=0
[APPCORE-CANARY] verdict=B
[APPCORE-CANARY] reason=nothing_rewritten: the APP core's start touched no byte of the span — not even ROM main's exception-handler pairs, so ROM main did not re-run on it either
```

**Silicon rewrote not one byte.** Not the unpack table, not the bss table,
not even ROM `main`'s seven `_xtos_set_exception_handler` pairs — so ROM
`main` did not re-run either. The same harness on the emulator under the old
model, same commit:

```
[APPCORE-CANARY] post_start w[0x3ffe0448]=0x40000de8 w[0x3ffe0548]=0x40006840 w[0x3ffe0860]=0x00000000 w[0x3ffe09a8]=0x00000000
[APPCORE-CANARY] range 0x3ffe0440..0x3ffe0858 len=1048 first=00 00 00 00 00 00 00 00 shown=8
[APPCORE-CANARY] range 0x3ffe0860..0x3ffe1320 len=2752 first=00 00 00 00 00 00 00 00 shown=8
[APPCORE-CANARY] scan changed=3800 ranges=2 handler_words=56 data_xtos_pro=992 bss_xtos_pro=2752 outside=0
[APPCORE-CANARY] verdict=A
```

A control run on `main` (single-core, core 1 never released) changed nothing
on either side, so the scan measures the release and nothing else.

## The correction

`Machine::service_app_core_start` now models the release as **core 1
beginning at `appcpu_ctrl_d.appcpu_boot_addr` with no ROM code run** — no
`_ResetVector`, no reset handler, no unpack or bss table, no ROM `main`, no
`_xtos_set_exception_handler`. Slot 1 is still wiped to a fresh hart, and
then exactly five fields are seeded, each cited in the code:

| field | value | where it comes from |
|---|---|---|
| `pc` | `appcpu_boot_addr` | esp-hal's `start_core1` wrote it |
| `PS` | `0x0006_0020` | the ROM's post-`_start` word `0x40020` plus the `CALLINC(2)` of the `callx8` that reaches the entry |
| `a1` | `__stack_app` = `0x3ffe7e30` | the ROM ELF's symbol; `reserved_rom_stack_app`'s end in esp-hal's `memory.x` |
| `[a1-16, a1)` | a `BootFrame` save area | the window-overflow guard the direct load's seam documents |
| `CPENABLE` | `0xff` | measured on the desk board by M4 P1 |

`VECBASE` is the reset `0x4000_0000` and `PRID` is `PRID_APP`, both from the
part's reset state rather than seeded.

The `CALLINC(2)` is a correction to the ruling's own sketch, which said
`PS = 0x40020` alone. With `CALLINC = 0` the firmware entry's `entry a1, N`
does not rotate — it consumes the seeded frame in place — and the first deep
window overflow spills through a zero stack pointer:

```
StrictViolation { cycle: 3261708, pc: 1074266255, address: 4294967264, width: Word, access: Write, ... }
```

Reproduced twice on the shipped image. `machine.rs`'s `APP_CORE_RELEASE_PS`
carries the whole argument.

With the fix, the same bench harness on the emulator gives silicon's answer:

```
[APPCORE-CANARY] start_app_core_isr bound=true wait_us=3
[APPCORE-CANARY] scan changed=0 ranges=0 handler_words=0 data_xtos_pro=0 bss_xtos_pro=0 outside=0
[APPCORE-CANARY] verdict=B
```

## ⚠️ What is still owed

**Why** the `appcpu_resetting` pulse does not put the APP core back through
the ROM is **not established**, and the code says so where it is modelled.
The most likely reading is that the APP core has been sitting in ROM
`main`'s `appcpu_boot_addr` poll since power-on and the pulse resumes rather
than re-resets it — but that is a hypothesis, and the bench measured the
*observable*, not the mechanism. Nor is it known what silicon's APP core did
at power-on before the firmware ever released it: the PRO core's boot
re-unpacks the same spans afterwards, so it is unobservable from either
side. This machine runs no ROM there and does not pretend to know.

The model is therefore pinned to a measurement, not to a derivation, and a
future part revision or a different release sequence is exactly the kind of
thing that would break it silently. The regression tests below are what make
that loud.

## Regression coverage

- `lp-emu-esp32v3/tests/dual_core.rs::the_released_core_writes_nothing_into_the_rom_pro_span`
  — L2's canary on the hand-built fixture, where core 0 provably never
  touches the span and every byte that moved is core 1's. Prints an
  `[APPCORE-CANARY/emu]` line beside the bench's.
- `…::the_shipped_image_runs_no_rom_code_on_core_one` — the same fact on the
  shipped image, pinned as the *cause*: core 1 starts at the address DPORT
  holds and executes no mask-ROM instruction on the way to the bind. (The
  memory scan cannot be run on the shipped image: that span is live
  allocator memory, and a scan of it measures the PRO core's own
  allocations.)
- `…::the_release_resets_core_one_and_seeds_its_entry_state` — the five
  seeded fields, one assertion each.
- The sixteen image gates in `dual_core.rs`, `boot.rs`, `boot_idle.rs` and
  `rom_up_boot.rs`, all green on both boot paths with `unmapped = 0`.
- `lp-fw/fw-esp32v3 --features test_appcore_rom_path` — the bench harness
  itself, kept on main as a permanent diagnostic, runnable on the emulator
  as well as the board.

## Lesson

Two, and they pull in opposite directions.

**An emulator's disagreement with silicon is the emulator's claim to
prove.** Every part of the wrong model was traceable to real ROM bytes, and
that is exactly why it was convincing enough to get a firmware defect filed
against innocent code. Reading the ROM tells you what the ROM *would* do if
it ran; it cannot tell you whether a given hardware event makes it run. When
those two are confused, the disassembly becomes a very persuasive way of
being wrong.

**And the cheap bench is worth more than the careful derivation.** The
question sat open through a full investigation that got everything right
except the one thing it could not read. It took one cargo feature, one
flash, and twenty minutes on the desk to settle — and the answer was
`changed=0`, which no amount of further reading would have produced.
