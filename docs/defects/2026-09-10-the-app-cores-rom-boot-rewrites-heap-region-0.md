---
status: open
found: 2026-09-10      # how: live-debugging (the classic emulator, M4 P1 bring-up)
area: lp-fw/fw-esp32v3 (main.rs heap regions; lpvm-native codemem_esp32 span constants)
class: assumed-context
related:
  - docs/adr/2026-08-04-rmt-isr-on-app-core.md
  - lp-emu/esp/lp-emu-esp32v3/tests/dual_core.rs
  - lp-emu/esp/lp-emu-esp32v3/tests/boot_idle.rs
  - planning lp2025/2026-09-10-0021-xtensa-emulator/m4/p1-two-cores-run.md
---
# The APP core's ROM boot rewrites heap region 0

**Symptom** — On the classic emulator with core 1 released (M4 P1), the
shipped `fw-esp32v3` image dies about 30,000 cycles after
`start_app_core_isr` releases the core, on both boot paths and on both a
first (formatting) and a second (mounting) boot of the same flash:

```
[INFO  lp_emu_esp32v3::machine] core 1: released by DPORT at cycle 3228994 — reset to _ResetVector at clock 3228995; appcpu_boot_addr = 0x400d5d34 (…start_core1_init…app_core_main…)
[WARN  lp_emu_esp_common::bus] UNMAPPED read4 at 0x00000008 from pc=0x401190b2
STRICT BUS STOP
  pc      = 0x401190b2 (…lp_fs…LpFsFlash…LpFs9read_file…+0x1e)
  cycle   = 3258896 (13578 us emulated)
  access  = Read Word at 0x00000008
```

A read through a null vtable pointer inside `LpFs::read_file`. The console
ends mid-line at `[INIT` — the `flash filesystem mounted` line is still in
the FIFO; the core-1 release happens before the UART has drained the
`[INIT]` chain.

**Root cause** — The mask ROM's reset path runs on **both** cores, and its
unpack and bss tables carry per-entry flags: `flag = 1` entries are copied
(`unpackloop` → `alwaysunpack`) and cleared (`_start`'s loop) on the APP
core too. Read out of the vendored rev300 ROM ELF by the emulator, the
`flag = 1` entries are:

| table | range | section |
|---|---|---|
| unpack (`_data_start` @ `0x4000d4f8`) | `0x3ffe0440..0x3ffe0858` ← ROM `0x4000f0e0` | `.data_xtos_pro` |
| bss (`_bss_start` @ `0x4000d5d0`) | `0x3ffe0860..0x3ffe1320` | `.bss_xtos_pro` |
| both | `0x3ffe0010..0x3ffe0440` (three spans) | `.bss_gpio_pro`, `.bss_uart_pro`, `.data/.bss_etsc_pro` |

So **every APP-core reset re-initialises `0x3ffe0440..0x3ffe0858` and zeroes
`0x3ffe0860..0x3ffe1320`** — 3,808 bytes — plus `main`'s
`_xtos_set_exception_handler` calls write the PRO xtos tables at
`0x3ffe0448..` and `0x3ffe0548..`, and `main` runs on `__stack =
0x3ffe3f20` (`.stack_pro`). The ROM never references its `_app` sections
(`.data_xtos_app` at `0x3ffe4350`, `.stack_app` at `0x3ffe5230`): the full
disassembly has no literal into them and only eight `rsr.prid` sites, none of
which selects a per-core data base. The `_app` sections are vestigial.

The firmware registers `SRAM1_ROM_PRO_STACK_SPAN = 0x3ffe0440..0x3ffe3f20`
as heap **region 0, first, before `start_app_core_isr`**
(`main.rs::add_rom_pro_stack_region`, "the span is dead from the first Rust
instruction … nothing has run on the ROM's stack since before `main`"), and
esp-alloc is first-fit in registration order, so the boot residents land at
`0x3ffe0440+`. `add_rom_app_stack_region` then reserves
`SRAM1_ROM_APP_STACK_BASE = 0x3ffe4350` until after the bind on the belief
that *that* is where the APP core's ROM boot runs. It is not: the APP core
boots on the PRO core's ROM data and stack. The assumption was about the
ROM's per-core sections, and the ROM was never asked.

Emulator evidence (M4 P1 diagnostic, `lp-emu-esp32v3` at PR #692): a diff of
`0x3ffe0000..0x3ffe4000` across core 1's ROM path shows exactly the ROM's
values landing — `0x3ffe0448 ← 0x40000de8` (`_xtos_c_wrapper_handler`),
`0x3ffe0548 ← 0x40006840` (`ets_fatal_exception_handler`), the powers-of-two
table at `0x3ffe07cc..0x3ffe0854` over a `0xff`-filled buffer, and live
pointers at `0x3ffe08f0..0x3ffe09c4` (`0x400d32d0`, `0x400d3174`, …) zeroed.
Restoring `0x3ffe0440..0x3ffe1320` from the pre-release copy removes the
`read_file` crash; a firmware built with ESP-IDF's ordering (region 0
registered **after** `start_app_core_isr`) boots to the idle heartbeat with
`[INIT] RMT ISR on APP core`, `unmapped = 0`, on both boot paths.

ESP-IDF avoids this by ordering: `call_start_cpu0` runs `start_other_core()`
— which waits for the APP core to be up — before `heap_caps_init()` adds
`0x3ffe0440..0x3ffe3f20` to its heap, and it reserves only the two
`rom_*_data` blocks permanently. esp-hal's own `memory.x` reserves
`reserved_rom_stack_pro` (`0x3ffe1320+11264`) and
`reserved_rom_stack_app` (`0x3ffe5230+11264`) and says "in theory both of
these can be reclaimed once both cores are running".

**Open question — why the desk board survives.** L1 (PR #690) booted the
same image bytes on the DOM-Z-102 to the idle heartbeat with core 1 running.
Every write above is the ROM's and happens on silicon too; the residents at
`0x3ffe0440+` are the same allocations. Either the victims on the bench are
benign by luck of layout, or something about the APP core's reset on silicon
is not what the ROM code says. The bench check is small: after a dual-core
boot, read `0x3ffe0448` (expect `0x40000de8` if the ROM wrote it) and a few
words of `0x3ffe0860..` against what the allocator put there. Until that
reading exists this entry is filed on the emulator's evidence alone, and the
emulator is stated to be the finder, not the judge.

**Fix** — none yet. The shape: hand the PRO span to the allocator only after
`start_app_core_isr` has returned (ESP-IDF's ordering; the diagnostic
firmware did exactly this), or register only the part the APP core's ROM
boot does not touch — `0x3ffe1320..0x3ffe3e00`-ish, below `main`'s frames —
early, and the rest late. Either moves the residents-first packing PR #516
relied on, so the memory figures G2/L1 compare will move with it and need
re-pinning (`tests/boot_idle.rs`: `HEAP_USED_GAP`, `STACK_HIGH_WATER_GAP`).

**Regression coverage** — `lp-emu-esp32v3/tests/dual_core.rs`
(`the_firmware_reports_the_dual_core_deployment` and siblings) and
`tests/boot_idle.rs`'s heartbeat gates are red on the shipped image for
exactly this reason and name this entry in their failure text; they go green
with the fix. The fixture test `the_doorbell_reaches_core_one` proves the
machine's side (the ROM path, the park, the doorbell) without the firmware.

**Lesson** — A span is not "dead once the ROM is out of it" on a chip whose
ROM runs on both cores: the second core re-enters the ROM's reset path
whenever it is started, on the *first* core's data and stack. Ask the ROM
(its unpack and bss tables carry the per-core flags) before reclaiming
anything it initialises, and register reclaimed ROM memory in the order
ESP-IDF does — after every core that will ever boot through the ROM has.
