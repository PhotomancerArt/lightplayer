# Classic ESP32 RAM budget: where 520 KB goes, and how to get more of it

Date: 2026-09-04
Branch: `claude/esp32v3-ram-analysis-940c42`
Related: `docs/adr/2026-08-01-esp32v3-flash-budget.md` (§"RAM is the real
constraint"), PR #516 (classic heap fragmentation research — the allocator
side of the same problem; this report is the physical-ceiling side).

## Summary

The classic ESP32 has 520 KB of SRAM. The LightPlayer image gives the
allocator **186,368 B (182 KB)** of it, and at idle roughly 175 KB of that is
free. The rest is not "used" in any interesting sense — most of it is either
unreachable by construction or reserved by a conservative default:

| where the other 338 KB is | bytes | reclaimable? |
|---|---:|---|
| SRAM0 flash cache (PRO + APP, 32 KB each) | 65,536 | APP half only if the APP core never runs flash code; word-only |
| SRAM0 IRAM that nothing uses | 115,216 | word-access only; **ideal home for the JIT code region** |
| SRAM0 IRAM in use (`.vectors` + `.rwtext`) | 15,856 | no |
| SRAM1 reserved for the ROM by esp-hal | 32,304 | **30,608 B yes** (IDF reclaims the same span); 2,160 B ROM data stays |
| SRAM1 JIT code region | 24,576 | **yes, if the JIT moves to SRAM0** |
| SRAM1 gap between the ROM reservation and the JIT region | 464 | yes |
| SRAM2 reserved for ROM data | 8,192 | no |
| SRAM2 `.data` | 22,140 | **17,896 B is anonymous constants an esp-hal default puts in RAM** |
| SRAM2 `.bss` excluding the heap arena | 26,224 | partly (16,656 B static frame buffer) |
| SRAM2 `.stack` (PRO core main stack) | 35,600 | unknown — never measured on this chip |
| alignment slack | 4 | |

Ranked levers, each independently landable:

| # | lever | heap gained | effort | risk |
|---|---|---:|---|---|
| 1 | Move the JIT code region to idle SRAM0 IRAM | +24,576 | medium | medium (new install path, emulator parity) |
| 2 | Reclaim the ROM stacks and gaps in SRAM1 as heap regions | +30,608 | low | low-medium (IDF precedent; order after APP-core start) |
| 3 | Constant pools and switch tables to flash, except the ISR path's | +17,896 (as stack, then arena) | low | medium (3 tables land in the WS281x refill ISR if done bluntly) |
| 4 | Measure the main stack, then hand the surplus to the arena | unknown, up to ~tens of KB | low to measure | none until the number exists |
| 5 | RTC fast RAM tail as a heap region | +7,216 | low | low |
| 6 | Static 16 KB serial frame buffer off SRAM2 | up to 16,656 | medium | medium (its placement is a recorded lesson) |
| 7 | SRAM0 as a word-only data pool (sample buffers, packed frames) | up to ~90,000 | high | high (byte access faults) |

Levers 1 + 2 + 3 + 5 together take the heap from 182 KB to about **260 KB**
without touching the stack, and lever 4 is the one that says how much further
it can go. That is the "more headroom" the question asked for; the number the
old S3 memory refers to (≈300 KB of heap being hard) was the S3's 341,760 B
`dram_seg`, which the classic has never had.

## The physical map

Three SRAM banks; only two have a data bus.

| bank | size | data bus | instruction bus | notes |
|---|---:|---|---|---|
| SRAM0 | 192 KB | none | `0x4007_0000..0x400A_0000` | first 64 KB is the flash cache when the cache is on; the rest is IRAM. Word-aligned 32-bit loads/stores work (IDF's `MALLOC_CAP_32BIT` heap is exactly this memory); byte access faults |
| SRAM1 | 128 KB | `0x3FFE_0000..0x4000_0000` | `0x400A_0000..0x400C_0000` (word-mirrored, reversed) | the JIT region and the second heap region live here |
| SRAM2 | 200 KB | `0x3FFA_E000..0x3FFE_0000` | none | esp-hal's `dram_seg` = the top 192 KB; `.data`, `.bss`, `.stack` |

Plus 8 KB RTC fast RAM (`0x3FF8_0000`, both buses) holding the 976 B crash
ledger, and 8 KB RTC slow RAM, unused.

## The image, measured

Built from this worktree at `a10bafa3c` (main), `--profile release-esp32v3`,
default features. The ELF is byte-for-byte the same size as the LLFF bench
ELF PR #516 built (2,964,704 B), so its numbers apply to that work too.

| section | address | bytes | what |
|---|---|---:|---|
| `.vectors` | `0x4008_0000` | 1,024 | SRAM0 |
| `.rwtext` | `0x4008_0400` | 14,832 | SRAM0: esp-rtos scheduler/timer (3,533), RMT ISR + wire pusher (2,842), esp-hal handlers (1,181), lp-ws281x refill (773), patched ROM spiflash routines (~2,000) |
| `.data` | `0x3FFB_0000` | 22,140 | SRAM2: only 4,296 B are named symbols (esp-rtos `SCHEDULER` 1,352, RMT `DRIVER` 1,220, `MAILBOXES` 544 …). The other 17,844 B are anonymous |
| `.bss` | `0x3FFB_5680` | 138,864 | SRAM2: heap arena 112,640; serial `FRAME_BUF` 16,656; `APP_CORE_STACK` 4,096; embassy main-task `POOL` 3,832; io-task pool + mailboxes ~1,400 |
| `.stack` | `0x3FFD_74F0` | 35,600 | SRAM2: whatever `dram_seg` has left |
| `.rtc_fast.persistent` | `0x3FF8_0000` | 976 | crash ledger |
| `.rodata` | flash | 278,656 | |
| `.text` | flash | 1,859,077 | |

`dram_seg` is exactly full: 22,140 + 138,864 + 35,600 + 4 = 196,608. The stack
is the residual, and it has been shrinking as `.data`/`.bss` grew: 42,888 B
in the 2026-08-01 ADR, 35,600 B today.

The heap is two `esp_alloc` regions:

| region | span | bytes |
|---|---|---:|
| 0 (arena, in `.bss`) | `0x3FFB_5A54 + 112,640` | 112,640 |
| 1 (SRAM1 tail) | `0x3FFE_E000..0x4000_0000` | 73,728 |
| total | | **186,368** |

SRAM1 in full, low to high:

| span | bytes | owner |
|---|---:|---|
| `0x3FFE_0000..0x3FFE_0440` | 1,088 | ROM PRO-CPU data — must stay |
| `0x3FFE_0440..0x3FFE_1320` | 3,808 | nothing (gap esp-hal does not name) |
| `0x3FFE_1320..0x3FFE_3F20` | 11,264 | ROM PRO-CPU stack — used only before the app's entry |
| `0x3FFE_3F20..0x3FFE_4350` | 1,072 | ROM APP-CPU data — must stay |
| `0x3FFE_4350..0x3FFE_5230` | 3,808 | nothing |
| `0x3FFE_5230..0x3FFE_7E30` | 11,264 | ROM APP-CPU stack — used only while the APP core boots through ROM |
| `0x3FFE_7E30..0x3FFE_8000` | 464 | `dram2_seg` head, unused |
| `0x3FFE_8000..0x3FFE_E000` | 24,576 | JIT code region (`CodeRegion::ESP32_DEFAULT`) |
| `0x3FFE_E000..0x4000_0000` | 73,728 | heap region 1 |

## Why the ceiling is where it is

1. **SRAM0 has no data bus.** 128 KB of IRAM, and the image needs 15.9 KB of
   it. The remaining 115 KB can only be touched as aligned 32-bit words, so
   `esp_alloc` cannot serve it as ordinary heap. It is, however, exactly what
   executable code needs — and the JIT region is currently spending 24 KB of
   byte-addressable SRAM1 on something SRAM0 could hold for free.
2. **esp-hal reserves the ROM's stacks forever.** Its own `memory.x` says "in
   theory both of these can be reclaimed once both cores are running, but for
   now we play it safe". esp-idf does reclaim them
   (`heap_caps_enable_nonos_stack_heaps`, after the scheduler starts) and
   reserves only the two ROM data blocks. This image starts the APP core once
   at boot through the ROM (which uses the ROM APP stack until esp-hal's
   `start_core1_init` switches to `APP_CORE_STACK`), and never restarts it.
3. **esp-hal's `place-switch-tables-in-ram` defaults to on.** It routes
   `.rodata..Lswitch.table.*`, `.rodata.cst*` (constant pools: float
   constants, lookup tables) and the interrupt-handler tables into `.data`.
   Measured by rebuilding with `ESP_HAL_CONFIG_PLACE_SWITCH_TABLES_IN_RAM=false`:

   | | `.data` | `.rodata` | `.stack` |
   |---|---:|---:|---:|
   | default | 22,140 | 278,656 | 35,600 |
   | off | 4,244 | 296,512 | 53,488 |
   | delta | **−17,896** | +17,856 | **+17,888** |

   (`place-anon-in-ram` is already off; this is the only knob in play.)

   A link map of the default build (`cargo rustc -- -C link-arg=-Wl,-Map=…`)
   says what the 17,844 anonymous bytes are:

   | input sections | bytes | owners |
   |---|---:|---|
   | `.rodata..Lswitch.table.*` (match-statement jump tables) | 12,787 | `lpc_model` 2,508 in 83 tables; `lps_builtin_ids` 2,048 in 2; `fw_esp32v3` 1,179; `lps_glsl` 1,000; `xtensa_lx_rt` 656; `lps_shared` 598; `lpvm` 568; `esp_hal` 552; `snoise3_f32` 620 in 5 copies; the rest in sub-500 B slices across 15 crates |
   | `.rodata.cst4/8/16/32` (constant pools, LTO-merged into one object) | 5,080 | not attributable by crate after LTO — float constants and small lookup tables, plausibly the `f32` builtins' |
   | named `.data` | 4,248 | `SCHEDULER`, `DRIVER`, `MAILBOXES`, `CLOCK_TREE`, wakers … |

   Nothing here is one big table someone can move by hand; it is 250-odd
   small jump tables from the model, the compiler and the engine, which is
   why the linker-script route in lever 3 is the right shape.
4. **`dram_seg` is zero-sum with the stack**, so every static byte added to
   `.data`/`.bss` since the ADR came straight out of `.stack`, and the stack
   has never been measured on this chip. The C6 has a paint-and-scan
   `stack_probe`; the classic does not.

## Levers in detail

### 1. JIT code region → SRAM0 (+24,576 B heap, and a bigger JIT region)

Today `codemem_esp32` links each shader at a span of the SRAM1 region and
installs it through the word-mirrored D-bus walk, because that is the only
way to write SRAM1's I-bus image. SRAM0 needs no mirror: aligned 32-bit
stores to `0x4008_xxxx` land directly (this is how IDF's IRAM heap and
`heap_caps_malloc(MALLOC_CAP_32BIT)` work), followed by the same
`isync`/memw discipline the current installer already does.

- Place the region above `.rwtext`'s end (`0x4008_3DF0` today; take a
  linker symbol, not a constant) — at least 64 KiB fits with 48 KiB to spare,
  so the 24 KiB region that was "2.06× the keep-last-good peak" can more than
  double for nothing.
- `reclaimable_heap_span()` then covers all of `0x3FFE_8000..0x4000_0000`
  (96 KiB) and the const-asserts that pin the split move with it.
- `lp-xt-emu`'s `BoardProfile::esp32()` models the SRAM1 mirror
  (`code_dbus_base`, `alias`); it needs a second executable window with plain
  word semantics for parity.
- Verify on the desk board with one probe before the port: write a word to
  `0x4008_4000`, read it back, execute a `ret` placed there.

### 2. Reclaim the ROM stacks and gaps (+30,608 B, three regions)

Add as `esp_alloc` regions **after** `start_app_core_isr` returns (the APP
core's ROM boot is the last user of the ROM APP stack):

| span | bytes |
|---|---:|
| `0x3FFE_0440..0x3FFE_3F20` | 15,072 |
| `0x3FFE_4350..0x3FFE_7E30` | 15,072 |
| `0x3FFE_7E30..0x3FFE_8000` | 464 (fold into lever 1's span instead) |

Two 15 KB regions are awkward for large blocks but fine for the hundreds of
small allocations the project read makes (PR #516: the read is volume, not
contiguity). With esp-alloc's first-fit order being region-add order, add
them **before** the arena so small residents land there and the arena stays
whole — that is the residents-first packing PR #516 found to be the top
allocator lever, obtained by placement rather than by allocator changes.

What this lever does **not** do: the heartbeat's `largest_free_block` is a
trial-allocation bisection across all regions, and both the 32 KiB read gate
and the 64 KiB `LoadProject` gate compare against that one number. Two
15 KB regions therefore raise packing headroom (and keep the arena's big
block whole for longer) without ever satisfying a gate on their own — the
gates' fallible-path rework is PR #516's follow-up. Levers 1 and 4 are the
ones that grow a single contiguous block.

Keep-out: the two ROM data blocks (1,088 + 1,072 B) are read by ROM routines
the image still calls (the patched `esp_rom_spiflash_*` family calls into
ROM). esp-hal's `software_reset` re-enters ROM on the ROM stack, but by then
the heap is gone anyway.

### 3. Constant pools to flash — targeted, not blanket (+17,896 B)

The blunt flag works and is measured above, but three of the moved constants
belong to `Ws281xDriver::fill_half`, the RMT refill path (`isr-in-ram`), and
one each to `__level_1_interrupt`/`__level_3_interrupt`. A flash-resident
lookup table in the refill ISR is exactly the cache-miss stall the
`isr-in-ram` feature was added to remove, and a fault if it coincides with a
flash write (cache off). So:

- either keep the flag on and use `ESP_HAL_CONFIG_USE_RWDATA_LD_HOOK` with a
  `rwdata_hook.x` that routes only `*lp_ws281x*`, the `rmt` objects and
  `*_esp_hal_internal_handler*` tables into `.data`, sending everything else
  to `.rodata`;
- or turn the flag off and mark the specific tables `#[ram]` /
  `#[unsafe(link_section = ".data")]` (needs the by-crate map below to find
  them; `llvm-nm` will not show anonymous symbols).

Expected cost: switch-heavy host code (the GLSL compiler, serde) reads its
tables through the flash cache. Measure a compile and a `zook-dome` frame
before/after; the engine already runs from flash so the effect should be
small.

Side finding worth its own look: even in the default build, IRAM functions
already read 63 flash-resident constants — esp-rtos's `timer_tick_handler`
(19) and `cross_core_yield_handler` (16), `rmt_isr` (3), `wire_pusher::run`
(2). Most are likely panic/format strings that never execute, but the
ISR-in-RAM rule says verify, not assume.

### 4. Measure the stack, then right-size it

Port the C6's `stack_probe` (paint at boot, scan at the heartbeat; on Xtensa
read `a1` instead of `sp`, and mind the windowed ABI's spill area under the
guard word) and run: boot, largest-shader compile, `zook-dome` load, project
read/write from Studio. Whatever the high-water leaves above a margin
(fw-esp32s3 runs at 52,896 B and has never been the constraint) converts
directly into `HEAP_SIZE`. Levers 2 and 3 land as stack first, so this is
also what turns them into heap.

### 5. RTC fast RAM tail (+7,216 B)

`0x3FF8_03D0..0x3FF8_2000` after the ledger; both buses reach it and IDF
serves it as 8-bit heap. Tiny, cheap, and a natural home for the io task's
static pools if they need to leave SRAM2.

### 6. The 16,656 B serial frame buffer

`server_msg::FRAME_BUF` is sized `PROJECT_READ_FRAME_MAX_BYTES` (16 KiB) +
margin and is static by recorded decision: it must not ride the
loaded-project heap and must not be an interrupt executor's stack frame.
Options that respect both: halve the wire frame budget (cross-cutting, the
Studio batcher shares the constant), or place it in a reclaimed SRAM1 chunk
once lever 2 exists (it does not fit a 15,072 B chunk — the budget would have
to drop to 12 KiB or the ROM APP data block would have to move). Lowest
priority of the near-term set.

### 7. SRAM0 as a word-only data pool (parked)

After lever 1 there are still ~90 KB of SRAM0 that nothing can byte-address.
Q32/f32 sample buffers and u32-packed frames are word-only by nature and
could come from a dedicated bump/pool allocator there — but `compiler-builtins`'
`memcpy`/`memset` and any `[u8]` view (serde, slices of bytes) will byte-access
and fault. IDF makes IRAM byte-accessible only by trapping and emulating.
High payoff, high effort; a plan of its own if the first five levers are not
enough.

## What the headroom buys (device numbers from PR #516)

The fragmentation research session measured the same 186,368 B heap on the
desk DOM-Z-102 over the serial heartbeat (2026-09-05, first-fit build; log in
its planning dir under `bench/bench-llff-reload.csv.log`):

| state | used | free | largest block |
|---|---:|---:|---:|
| idle, no project | 16,036 | 170,332 | 94,780 |
| `/projects/studio` loaded | ~57,900 | | |
| … after its shader compile, at rest | ~152,900 | ~33,400 | ~25,500 |
| after `stopAllProjects` | | ~166,000 | ~39,700 (4.6 KB of leftovers pin it) |

The resting state is what refuses Studio's reads (`largest free block
25511 B < 32768 B`), and the post-unload state is what refuses the 64 KiB
`LoadProject` gate until a power cycle
(`docs/defects/2026-09-04-unload-leaves-classic-unloadable-until-power-cycle.md`).
Both are ceiling problems as much as fragmentation problems: the studio
project's own residency is ~137 KB after compile, against a ceiling of 182.
Levers 1–3 and 5 add ~80 KB, which would leave that project resting with
~113 KB free rather than 33 — and, since the new SRAM1 regions are separate
first-fit regions, the arena's largest block is no longer the only one that
counts. Region-add order decides the packing, so lever 2 should be planned
together with PR #516's residents-first finding rather than after it.

## Not levers

- The APP-core half of the flash cache (32 KB at `0x4007_8000`) is freed by
  IDF only in unicore mode. The wire pusher runs on the APP core from IRAM,
  but esp-hal enables that core's cache at start and nothing proves flash is
  never fetched there. Word-only memory in any case.
- `RESERVE_DRAM` / BT reservation: not in play, the image links no radio.
- Shrinking `.rwtext`: 14.8 KB total, all of it there on purpose.
