# The S3's radio memory: where Bluetooth's and Wi-Fi's RAM comes from

- Status: accepted
- Date: 2026-10-21
- Plan: `lp2025/2026-10-20-2335-s3-radio-and-ota` (M3 #1251, Bluetooth; M4, Wi-Fi)
- Related:
  - `2026-09-02-esp32c6-ram-split.md` (the C6's split, the model for this one)
  - `2026-10-07-c6-wifi-link.md` (the network this RAM is for)
  - `2026-07-30-esp32s3-partition-floor.md` (the S3's flash, which has room)
  - `docs/defects/2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md`,
    `docs/defects/2026-10-17-radio-heap-overflow-reached-the-split-loaders-ram.md`,
    `docs/defects/2026-09-24-ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut.md`
    (the three radio-heap defects this design answers)

## Context

On the ESP32-S3, `dram_seg` (`0x3FC8_8000..0x3FCD_B700`, 341,760 B) holds
`.data`, `.bss` (the main heap arena included), the RAM-resident code that
shares SRAM1 with it (`.rwtext`, mirrored by `.rwdata_dummy`) and, as whatever
is left, the main stack. Every byte one of them takes is a byte another
loses. Before the radio the S3 split it as a 240 KB arena and a 51,976 B main
stack (high water 11,456 B, `lp-emu:esp32s3:t1`), with `dram2_seg`
(73,744 B, the IDF bootloader's RAM, `crate::dram2_heap`) as a second,
untagged heap region: heap total 319,504 B, 287,940 B free at idle.

The radio brings three costs into that space: its blobs' code that must run
from RAM (IRAM), its statics, and the heap its blobs allocate at run time
through C `malloc`. It also brings two hazards the C6 found on silicon:

- **Fragmentation.** The BLE link layer allocates whenever advertising
  restarts, and blocks landing in the main arena above a project split the
  space the next project needs.
- **Warm-reset DMA.** A radio keeps running across a warm reset of the
  system, and its DMA keeps writing into buffers the previous app gave it.
  On the C6 those writes crashed the second-stage bootloader (`dram2_seg`)
  and could reach the split loader's RAM. On the S3 the boot stages'
  RAM is `lp_bootctl::S3_BOOT_STAGE_RAM`: the split loader's 16 KiB
  (`0x3FCC_5000..0x3FCC_9000` since the loader's L4 amendment, #1255; it was
  `0x3FCB_0000..0x3FCC_0000`, mid-arena, when this was written), which sits
  inside the core's `.bss`, and the
  IDF bootloader's (`0x3FCD_9700..0x3FCE_D710`), which is `dram2_seg` and the
  top 8 KB of `dram_seg`, where the main stack is.

The S3's flash is not the constraint: 6 MB `factory`, gate headroom 3.7 MB
before any radio. RAM is.

## Decision

1. **The radio's C heap is a region of its own** (`fw-esp32s3/src/c_heap.rs`,
   the C6's design): a 48 KB `.bss` arena on an image with the Wi-Fi blob
   up (`espnow`, `wifi`; a Bluetooth image, `ble` alone, keeps M3's 32 KB), tagged `Internal | External`
   (`External` borrowed as a tag nothing else asks for; this firmware maps
   no PSRAM). The blobs' `malloc` tries it first, then the arena's `Internal`
   pieces; never an untagged region. esp-alloc's `compat` is off (its
   `malloc` asks for no capability, so it could land anywhere). Rust's
   global allocator reaches the radio region last, so its spare bytes are
   still Rust's. 48 KB is the C6's figure (joined high water 47,308 B, the
   same blob classes and the same lean Wi-Fi driver counts); on the S3 the
   blobs reach 42,144 B with both radios up and nothing joined (emulated).
2. **No radio DMA target in `dram2_seg` or a boot stage's RAM — enforced
   twice.** By capability: the main arena is cut around every range of
   `S3_BOOT_STAGE_RAM` (pieces outside are `Internal`, the piece inside the
   loader's RAM is untagged), and `dram2_seg` is untagged, so no request that
   asks for `Internal` — the blobs' C heap, esp-radio's own buffers,
   esp-rtos's thread stacks — can land there. And by check: `c_alloc`
   asserts every block it hands the radio is clear of the boot stages' RAM
   (two range compares), and the boot asserts the radio region is. A
   violation fails loudly in the emulator's boot gates rather than becoming
   a bootloader crash on a warm reset months later.
3. **The Wi-Fi blobs' optional IRAM classes link into flash** (the esp-hal
   fork's `place_wifi_iram_in_flash` and `place_wifi_rx_slp_iram_in_flash`,
   now active for `esp32s3` as for the C6; set in
   `fw-esp32s3/.cargo/config.toml`): `.wifi0iram`, `.wifiextrairam`,
   `.coexiram`, `.wifirxiram`, `.wifislprxiram`, `.wifislpiram`. ESP-IDF 5.5.3
   links the same classes to flash under `ESP_WIFI_IRAM_OPT=n` and friends.
   26,220 B of `dram_seg` comes back on this chip (48,540 B on the C6, whose
   blobs carry more of these classes). What stays in RAM regardless: every
   archive's `.iram1` (the Wi-Fi interrupt top half in `libpp`, the PHY, the
   BTDM controller), `.phyiram`, `.coexsleepiram` and the rest the fork's
   README lists.
4. **The BTDM controller's IRAM stays in RAM** (22,584 B). The fork's BLE
   switch names the C6's NPL archive (`libble_app.a`) and pairs with an NPL
   blob call (`esp_ble_controller_flash_only_param_config`); BTDM is a
   different blob. Moving it is research, not this decision.
5. **The main arena is 136 KB on an image with the Wi-Fi blob up**
   (`espnow`, `wifi`), so the main stack keeps its share: 51,684 B with both
   radios, 55,756 B with BLE and ESP-NOW, against 51,976 B before. A
   Bluetooth image (`ble`, the default since the S3 Bluetooth walk) keeps
   M3's 168 KB arena and 32 KB radio region; a radio-less image is unchanged
   (240 KB).
6. **The threads**: `lp-net` 12 KB (the C6's 8 KB plus half again, for
   Xtensa's windowed frames — unmeasured here), priority 1, pinned to core 0
   (`NetChip::pin_to_core`, which this chip needs and the C6 does not);
   esp-radio's BTDM controller task 8 KB on core 0 (`ProCpu`, its default);
   the link thread as before (4 KB, core 0). Thread stacks come from the
   arena's `Internal` pieces.
7. **No PSRAM** (the plan's Q5): `dram_seg` closes without it. *Superseded
   2026-10-23: see the amendment below.*

### Why moving Wi-Fi code to flash is safe on Xtensa

Code in flash cannot run while a flash write has the cache off. The C6's
argument is that `esp-storage` clears `mstatus.MIE` around every ROM flash
call, so no code of any kind runs during one. On the S3 the same holds by a
different instruction:

- `esp-storage` (the S3 build, feature `critical-section`, on in
  `fw-esp32s3/Cargo.toml`) wraps every ROM flash routine — read, erase,
  write, unlock — in `esp_sync::RawMutex::lock`, whose Xtensa `enter` is
  `rsil 5`: interrupt levels 1 to 5 are masked for the call. The radio's
  interrupts (BTDM's `rwbt`, `bb`, `rwble` and Wi-Fi's MAC, all priority 1),
  esp-rtos's tick and context switch (priority 1) and every esp-hal priority
  (1 to 3) are inside that. Only levels 6 (debug) and 7 (NMI) stay open, and
  nothing in this image runs code at either.
- With interrupts masked on core 0 no thread switch can happen, so no task
  (esp-radio's Wi-Fi task, `lp-net`) runs during the call either.
- Core 1 is never started (the S3 plan's convention; `io_thread`'s pin), so
  no other core executes anything. If core 1 is ever started this argument
  must be redone: `esp_sync::RawMutex` is cross-core for the lock word, not
  for what the other core is executing from flash.
  **Redone 2026-10-24** (`two-core`, `docs/adr/2026-10-24-s3-second-core-renders.md`):
  core 1 now runs render jobs, and every flash writer stalls it with
  RTC_CNTL's stall key for the call (`FlashStorage::multicore_auto_park`, and
  the split image's raw ROM calls through `core1::with_core1_parked`). So
  during a flash call core 1 executes nothing, and the moved classes still
  run on core 0 only, where interrupts are masked. The argument holds.

The moved classes are code ESP-IDF already runs from flash under the same
condition (its flash writes disable the cache and mask non-IRAM interrupts).

## The ledger (2026-10-21)

Plain images, `xtensa-esp32s3-elf-size`, against the image before the radio:

| | before | `espnow` (BLE + ESP-NOW) | `wifi` (both radios) |
|---|---:|---:|---:|
| IRAM (`.rwtext` + `.rwtext.wifi`) | 12,052 | 37,644 | 37,648 |
| `.data` + `.data.wifi` | 3,596 | 15,296 | 15,600 |
| `.bss` (arena and radio region included) | 273,096 | 232,036 | 235,804 |
| main arena | 245,760 | 139,264 | 139,264 |
| radio region | — | 49,152 | 49,152 |
| main stack | 51,984 | 55,756 | 51,684 |

The `espnow` column is BLE with the Wi-Fi blob up for ESP-NOW. The default
image, `ble` alone, is M3's (arena 168 KB, radio region 32 KB, main stack
54,392 B; heap used 69,620 B idle, emulated), and its PR measured it against
the image before the radio: IRAM +24,732 B, `.data` +7,968 B, `.bss` statics
+5,848 B. Wi-Fi's own statics, against M3's BLE-only image, are +18,240 B (`.data` +4,036, `.bss`
+14,204, the network on `lp-net` included); its IRAM was +27,084 B before the
switches moved 26,220 of it.

Heap at idle, both radios up, no network joined, no project
(`lp-emu:esp32s3:t1`, the first heartbeat): total 262,160 B, used 130,608,
free 131,552, largest free block 73,744 (`dram2_seg` whole). The radio region
at 42,144 of 49,152 B, nothing overflowed. Before the radio: total 319,504,
free 287,940. For comparison, the C6's total is 372,344 B.

## Consequences

- **The S3 with both radios has a smaller heap than the C6**: 262,160 B
  total against 372,344, and about 131 KB free at idle without a project,
  against 288 KB on the S3 without a radio. Projects that fit the C6 may not
  fit an S3 running both radios; the project load gates refuse rather than
  crash, and a load is tried and recovered. A board that needs more is the
  PSRAM question (Q5), which this decision does not open.
- Rust's arena is three pieces and esp-alloc holds five regions: the arena's
  three, `dram2_seg` and the radio region. A link that moved the arena across
  a second boot stage would need a sixth; `c_heap::install_main` panics at
  boot rather than drop one.
- The radio region's size and the main stack's are emulated figures until
  FS3 has an antenna: joined, connected and editing high water, and the warm
  reset run (AC7: no radio DMA target in `dram2_seg`, 0 bootloader crashes in
  20 warm resets), are owed with the desk walk.
- **Split images**: with the radio linked, GNU ld's Xtensa literal coalescing
  lets a blob instruction load a constant stored in an engine function's
  literal pool, and `lp-fw-split`'s reachability then keeps that function and
  what it calls in the core (`btdm_controller_task` → `rw_pre_main` → the
  `.literal` of `LpServer::handle_secure_link_events`): the `wifi` split core
  is 2.67 MB, the engine 0.64 MB. The compiler stays in the engine. This is
  the S3 split tool's question (the updates plan), recorded here because the
  radio is what triggers it.

## Alternatives Considered

- **No IRAM switch, a smaller arena** (about 110 KB): the heap would be
  26 KB smaller still, for no gain the switch's argument does not already
  give.
- **A smaller main stack** (about twice its high water) would hand ~25 KB to
  the heap. Not taken: the stack's share is this chip's standing rule, and
  its high water with the radio running is not measured on silicon yet.
- **PSRAM for the project heap** (Q5): off the table unless `dram_seg` cannot
  close; it closes.
- **The BTDM controller's IRAM to flash** (22,584 B): needs ESP-IDF's
  `BT_CTRL_RUN_IN_FLASH_ONLY` equivalent for BTDM and a silicon run; the
  lever to try next if the heap is the problem.

## Amendment (2026-10-22): ESP-NOW is its own tier, off by default

`ble` became a default feature with the S3 Bluetooth walk (#1254). This
ADR's first cut made `radio` the C6's tier — the Wi-Fi blob up and the
ESP-NOW driver — so `ble` on by default would have started the Wi-Fi blob on
every S3: about 37 KB more heap in use at idle (69,620 → 106,544 B,
emulated). Lane E's 620-pixel project with BLE on leaves 63 KB free and a
49.5 KB largest block (#1261), so that 37 KB is the difference between a
project that fits and one that does not. **ESP-NOW is its own feature,
`espnow`** (`radio` is the bare radio, `wifi` implies `espnow`), and it is not
in `default`. Whether it starts by default — or only when a project has a
Radio node or a network is saved — is Yona's call. The C6 runs it by default.

## Amendment (2026-10-23): PSRAM and ESP-NOW on by default

Point 7 ("No PSRAM") no longer holds. Yona ruled on M10's walk
(`docs/reports/2026-10-22-s3-psram-walk.md`): the S3 brings its 8 MB of octal
PSRAM up in the core by default and puts every Rust allocation there first
(`psram`, policy `all`; `docs/adr/2026-10-22-s3-psram.md`). With Rust's heap
out of internal RAM, the 2026-10-22 amendment's question is answered the same
day: **`espnow` is in `default`**, as on the C6.

What changes here:

- **Point 1's tag.** On a `psram` image `External` is PSRAM's, so the radio's
  region is `Internal | Reserved` (the esp-alloc fork's third diff), not
  `Internal | External`. The radio's C heap and its DMA buffers still never
  reach PSRAM; thread stacks stay internal.
- **The default image is the `espnow` column of the ledger**, with PSRAM's
  3 KiB taken out of the arena for its init's IRAM: arena 136,192 B, radio
  region 49,152 B.
- **The figures** (`lp-emu:esp32s3:t1`, the heap-budget gate's idle sample,
  memory FS): internal free 208,604 → 187,540 B, largest block 121,688 →
  107,560 B, internal total 278,544 → 259,088 B. At idle the Wi-Fi blob's
  ~37 KB outweighs what PSRAM takes, because an idle board's Rust heap is
  small; the gain is under a project.
- **On FS3** (XIAO ESP32-S3 Plus, 16 MB, antenna attached; 2026-10-23, the
  split default image, `projects/test/skirt-620` on 2 × 310 px, Bluetooth
  connected from Mac Chrome, 326 s from a reboot): internal free
  **185,668–187,580 B**, largest block **105,704–107,576 B**, against main's
  image on the same board and project in M10's walk (`ble` alone, no PSRAM:
  50,780 B free, 39,584 B largest). 21.01 fps median against main's 22.15
  (−5.1 %; M10's `all` without ESP-NOW was −3.2 %), frame p99 ≤ 75 ms in
  every 5 s window, radio region high water 42,976 of 49,152 B, no panic,
  one boot. Inside the radio frame-rate budget's 10 % idle ceiling.

## Follow-ups

- The desk sitting (S3 radio plan, M3/M4 owed): the radio region and stack
  high water joined and connected, AC7's warm resets, the frame-rate cost.
- The S3 split image's literal-pool coupling (the updates plan, lane O).
