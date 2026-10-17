# Time to first light on the C6: CX1, littlefs and the tree store

**Date** 2026-10-16 · **Plan** `lp2025/2026-10-16-0848-tree-store-adoption`,
milestone A1 (decision AD9) · **PR** #1152

**The question.** How long from plugging a board in to its first lit LED, and
what each part of that costs. Yona's goal is **under 1 s**, less is better, and
the tree store must not make it worse than littlefs.

**The answer, on silicon.** The shipped split image (littlefs) lights the basic
project at **876 ms** with no host on the port and at **916 ms** with one
connecting during the boot. The `fs-tree` image lights it at **898 ms**. With a
c40-sized project it is **920 ms** (littlefs) against **1,013 ms** (`fs-tree`).
On a written-out tree store it is **1,615 ms**, and **1,703 ms** nearly full.
Today's littlefs boards are just under the 1 s goal. The tree store adds the
mount: +93 ms at c40, and +700–780 ms on a used or full store. That is milestone
A2's job.

**The biggest costs** (littlefs, basic project, no host, of 876 ms):

| part | ms | what it is |
|---|---:|---|
| `split` | 188 | the core hashes its own 1.4 MB from flash for the hello's manifest (183 ms), every boot |
| `lit` | 188 | the frame that compiles the shader: 121 ms of compile and ~67 ms of frame |
| `boot` | 166 | power-on to the app: mostly the IDF bootloader printing ~22 log lines at 115,200 baud |
| `load` | 145 | the project's parse and set-up (`ProjectManager` load; the shader compiles later) |
| `radio` | 85 | ESP-NOW/Wi-Fi bring-up, BLE start, the network thread |
| `frame1` | 73 | the server loop's first (black) frame |
| `init`, `server`, `fs` | 31 | the board, recovery, USB link, boot records; RMT and the server; the mount |

## The line

Every power-on boot of the shipped image now says, once:

```text
[perf] first light at 876 ms: boot 166 init 21 fs 3 radio 85 split 188 server 7 load 145 frame1 73 lit 188 (lp-sys 948/-1597 us)
```

`lp-fw/fw-esp32c6/src/first_light.rs` is the code. The parts sum to the
total. Each part ends at one mark:

| part | from the mark before to … |
|---|---|
| `boot` | the app's entry (top of `core_boot`), counted from power-on |
| `init` | the start of the filesystem mount |
| `fs` | the mount done (or the format) |
| `radio` | the end of `core_boot`: manifest, quirks, status LED, ESP-NOW/Wi-Fi, BLE, the network thread |
| `split` | the engine's entry: `split_boot`, the core's own hash, the engine guard on a first boot |
| `server` | the start of the boot project's load: RMT, the hardware system, the server |
| `load` | the load returned |
| `frame1` | the end of the server loop's first frame |
| `lit` | the first lit frame sent: the compile window's frame, so the shader compile is in here |

- A boot that lights nothing in 64 frames says `first light none by <ms>`
  (no project, safe mode, a black pattern), with `lit` 0.
- The line rides the log channel. The board holds it until a host drains the
  link, so a host that opens the port late still reads it.
- It is said **only on a power-on reset**: the LP timer is not reset by a
  software, USB or watchdog reset.

**Its cost in the image:**

| | before (`3fc9aa987`) | after | |
|---|---:|---:|---:|
| core | 1,417,488 B | 1,418,688 B | **+1,200 B** (still 23,104 B short of its next 32 KiB page) |
| engine | 1,866,134 B | 1,866,288 B | **+154 B** |
| gated headroom (`just fw-esp32c6-size-check`) | 67,178 B | 67,024 B | −154 B; the gate is 65,536 B |

- The report's code is named from `core_boot`, so the split places it in the
  core, which has room before its page boundary. The engine is where headroom
  is gated, and it pays only the call sites.
- The marks are atomics in `.bss`, nine words and a flag. Nothing is
  allocated.

### The clocks, checked

- **`embassy_time` is the system timer.** esp-rtos's time driver reads
  `esp_hal::time::Instant`, which on the C6 is SYSTIMER unit 0 at 16 MHz, off
  the crystal. TIMG0, handed to `esp_rtos::start`, is only the scheduler's
  alarm.
  - esp-hal keeps SYSTIMER enabled and never resets it (`KEEP_ENABLED`), so it
    counts from the chip's reset.
  - `Instant`'s tick-to-µs scaler is set inside `esp_hal::init`, and the app's
    entry is before that. Read through `Instant` there, the count comes back
    as raw ticks, 16x too big.
  - The emulator showed it: its first line said `boot 2540` where the console
    put the entry at 159 ms. The marks read the raw count and divide by 16.
- **The LP timer counts from power-on and is reset only by power-on.**
  - It is the slow-clock counter in the LP domain. A CPU or core reset leaves
    the LP system alone; esp-hal documents it as "time since the last
    power-on reset".
  - On every CX1 power-on boot, the LP timer led the system timer by
    **0.9–1.0 ms** at the app's entry. That is the time before the CPU's reset
    was released. A count carried over from before the cut would be minutes.
  - The line is gated on `ChipPowerOn`, so a boot that printed it was a
    power-on. Every one of the records' 93 cuts printed it.
- **The slow clock runs ~0.2 % slow.** Over the boot, the LP timer fell
  1.3–3.0 ms behind the crystal-driven system timer (`lp-sys`'s second
  figure). So the line takes the LP timer's lead once, at the app's entry,
  and keeps every stamp on the system timer.

## Method

**Board:** CX1 `c6-expendable`, MAC `14:C1:9F:E6:54:90`, a generic C6 (not a
XIAO), leased as `direct: adoption A1` and dropped at the end. No other board
was touched.
- Power went only through `board power-cycle`.
- The port was resolved by MAC through the registry.
- It has no `/hardware.json`, so it runs the default manifest (XIAO C6, its
  output on GPIO18). The basic project drives 241 LEDs (723 B a frame).
- No network is saved, so Wi-Fi is on but does not join; BLE advertises.

**Images:** split images built at this branch's `b92b524f2` (the final
firmware). They were flashed whole (`merged.bin`, `lpfs` blank) with
`espflash write-bin`:
- **littlefs**: the shipped set (`just fw-esp32c6-split`);
- **fs-tree**: `just fw-esp32c6-split esp32c6,server,radio,fs-tree`.

**Driver:** `lp-cli/tests/board_first_light.rs`
(`time_to_first_light_on_cx1`, `#[ignore]`d). For each case it:
1. puts the store in the case's state over the board's USB lp-link;
2. ten times: cuts the power at idle with `board power-cycle`, opens the link
   the moment the port is back, and reads the line. The line itself proves
   the boot was a power-on.

The cases, in order, each on the store the one before left:
- `none`: the formatted, empty filesystem;
- `basic`: `projects/test/basic`, pushed the way Studio pushes;
- `c40`: replaced by a c40-sized project (`projects/test/basic` plus 124
  documents; 132 files, ~216 KB);
- `used` (`fs-tree` only): 20 more c40 pushes, each different, ~4.3 MB
  written;
- `full` (`fs-tree` only): 4 KiB files until the store said no space (127
  files).

`late host`: the littlefs basic case again, with the link opened 3 s after the
port came back. That boot runs with nobody draining the port, as on a USB
charger.

Records: `docs/reports/2026-10-16-c6-time-to-first-light/`. `python3
summarize.py docs/reports/2026-10-16-c6-time-to-first-light` prints every
table below from them.

## The tables (silicon, CX1, median (range) ms, n = 10)

| image | project | **total** | boot | init | fs | radio | split | server | load | frame1 | lit | compile |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| littlefs | none | **470** (469–470) | 166 | 21 | 1 | 84 | 188 | 7 | 2 | 2 | — | — |
| littlefs | basic | **916** (901–920) | 166 | 21 | 3 | 85 | 188 | 7 | 145 | 82 (72–88) | 218 (214–223) | 123 |
| littlefs | basic, late host | **876** (876–884) | 166 | 21 | 3 | 85 | 188 | 7 | 145 | 73 | 188 | 121 |
| littlefs | c40 | **920** (905–925) | 166 | 21 | 8 | 85 | 188 | 7 | 140 | 85 (74–92) | 218 | 123 |
| fs-tree | none | **479** (478–480) | 166 | 22 | 7 | 84 | 190 | 7 | 1 | 2 | — | — |
| fs-tree | basic | **898** (895–900) | 166 | 22 | 12 | 84 | 190 | 7 | 123 | 76 | 218 | 126 |
| fs-tree | c40 | **1,013** (1,012–1,015) | 166 | 22 | **100** | 84 | 190 | 7 | 146 | 104 | 195 | 126 |
| fs-tree | used (c40 + 20 pushes) | **1,615** (1,614–1,615) | 166 | 22 | **692** | 92 | 191 | 9 | 144 | 104 | 195 | 126 |
| fs-tree | full (c40 + 127 × 4 KiB) | **1,703** (1,701–1,704) | 166 | 22 | **779** | 93 | 192 | 9 | 144 | 104 | 195 | 126 |

- `compile` is the shader node's own `elapsed=` (`[shader-node] compilation
  succeeded`), and it sits inside `lit`. `none` has no line for it.
- The no-project boots say `none by`: their total is to the 64th frame, not to
  a light.
- **Spread is tiny.** Every boot of a case is within a few ms of the others,
  except where a host's handshake lands. That shifts ~10 ms between `frame1`
  and `lit`.
- **One-off boots** (in the records, not in the table):
  - The `fs-tree` image's first measured boot formatted the blank `lpfs`:
    **4,682 ms**, `fs` 3,909.
  - Its `split` was 432 ms: the engine guard hashed the engine on that boot,
    since the flash's first boot had been cut before it confirmed.
  - littlefs's first boot formats in 1 ms.

### What dominates, read off the table

- **The core's own hash, ~185 ms, every boot.** It is in `split`.
  - `ota::running_hook::install` builds the update session's manifest before
    the engine starts, and the manifest carries the running core's SHA-256.
  - That hash is 1.4 MB read from flash and hashed: `[OTA] core sha in 183 ms
    (1418688 B, cache)` on CX1.
  - It is there on purpose, so the server loop's first hello does not stall
    for it, but it sits on the path to first light.
- **The compile window's frame, ~190–220 ms.**
  - The shader compile is 121–126 ms of it, the same on both filesystems.
  - The rest (~65–95 ms) is the frame around it: the memory-pressure
    broadcast, rebuilding what it dropped, the render, the 723-byte send, and
    a host's handshake when one is there.
- **The bootloader, ~145 of `boot`'s 166 ms.**
  - The emulator's console, timed in guest time (`emu-t3-timeline.log.gz`),
    shows it. The ROM hands over at 9 ms. The IDF second-stage bootloader
    then prints ~22 lines (banner, flash settings, the partition table, each
    segment) at 115,200 baud, ~7 ms a line, until 144 ms.
  - The split loader then reaches the core at 157 ms.
  - Silicon's 166 ms matches the emulator's 158–160 ms, so the printing is
    the time.
- **The project load, 123–146 ms.**
  - Emulated, ~105 of its ~120 ms is one step, `project new after core
    project`: building the core project from its files.
  - It is faster on the tree store for basic (123 against 145 ms; reads are
    cheaper) and the same at c40.
- **The radios, ~85 ms**, and **the first frame, 73–104 ms.**
  - The first frame is longer at c40 on `fs-tree` (104 against 76 ms). It was
    not split further.
- **The mount**:
  - littlefs: 1–8 ms;
  - tree store: 7 ms empty, 12 ms basic, 100 ms c40, **692 ms used, 779 ms
    full**.
  - The full figure matches M7's 781 ms. The used figure is above the soak's
    560 ms: 20 c40 pushes wrote more than the soak's mix had.
- **A host on the port costs ~40 ms.** With a host opening the link during
  the boot, littlefs basic lit at 916 ms; with none, at 876 ms.
  - The difference is in `lit` (218 against 188 ms). The server answers a
    tick's requests before it renders (`messages_first`), and the host's
    hello and readiness requests land in that frame.
  - A board on a USB charger is the 876 ms case.

## Candidate savings, ranked

These are estimates from the parts above, not measurements of any change.
AD10's own plan (A8) decides which to do. Silicon figures, littlefs basic with
no host (876 ms) unless named.

| # | change | saves (est.) | notes |
|---|---|---:|---|
| 1 | **Mount a used tree store fast** (A2: header-chain walk for cold sectors, CRC only roots and newest sectors, whole-sector reads, fewer pass-2 scans) | **~500–700 ms on a used/full `fs-tree` board**; ~90 ms at c40 | The only part where the tree store is worse than littlefs, and the largest single cost on a used board. At A2's ≤ 200 ms, a used board is ~1.1 s; at littlefs parity, ~0.9 s. |
| 2 | **Take the core's hash off the boot path**: hash after first light, in slices on idle frames, or keep the digest beside the boot record when the core is written | **~185 ms** | Every boot pays it today. A host that asks for the manifest before the hash is done would wait for it (the reason it moved to install). A stored digest is a persisted-format change (the boot records). |
| 3 | **A quiet second-stage bootloader** (log level warn/none) | **~120 ms** | The bootloader is espflash 3.3.0's bundled one (or a build def's override). A bootloader is written only by a USB flash, never over the air, so fielded boards keep the loud one until reflashed. |
| 4 | **Bring the radios up after the first lit frame** | **~80 ms** | ESP-NOW/Wi-Fi, BLE and the network thread move behind the light. Constraints: M2 Run G's bring-up order (Wi-Fi before BLE), the XIAO RF-switch quirk before any radio, and a radio-only board staying reachable with no engine (core-only). |
| 5 | **Compile in frame 1 at boot** (no compile window for the boot project: nothing to drop yet) | **~65–95 ms** | Saves the window frame's overhead, not the compile. The window exists for edits on a full heap (ADR 2026-08-03). At boot nothing else holds memory yet, but that should be measured on the 128–512 LED design target. |
| 6 | **A faster project load** ("hot-file" fast path or a cached, pre-resolved form of the core project) | **~50–100 ms** | ~105 ms emulated is building the core project from its JSON. Needs a profile before a design. |
| 7 | **The first frame** | ~20–40 ms? | Not split by this line. It holds the unsolicited hello, the fixture's first mapping and the output open. A mark at the first tick's start would split it. |
| 8 | **Skip the `[mem]` and boot log lines** | a few ms | ~25 info lines before first light, each formatted under a critical section. Small. |

- **Taken together**, 2 through 6 are ~500–580 ms off 876 ms: a basic boot
  near **300–380 ms**.
- **1 is what keeps a used tree-store board under 1 s at all.**

## The emulator's figure (report only, never a gate)

`just report-first-light-emu [t1|t2|t3] [dir]` runs
`lp-cli/tests/emu_first_light.rs`. It boots the same two split images ROM-up
(the mask ROM, the IDF bootloader, the loader) on the emulated C6, power-on
reset, with the host attached from power-on. Each case boots twice. The
emulator models no LP timer, so its lines are from reset.

Emulated time, `lp-emu` at this branch (`b92b524f2`):

| image | project | t2 total | t3 total | t3 first lit frame on the pad (`--dump-frames` `start_us`) |
|---|---|---:|---:|---:|
| littlefs | none | 315 | 459 | — |
| littlefs | basic | 538 | 819 | 814 ms |
| littlefs | c40 | 540–575 | 819–860 | 814–855 ms |
| fs-tree | none | 320 | 468 | — |
| fs-tree | basic | 540 | 818 | 812 ms |
| fs-tree | c40 | 612 | 884 | 878 ms |

- **`t3` is within ~7 % of silicon** for a basic boot (819 against 876 ms).
  - Its parts track silicon's: `boot` 160 against 166, `split` 200 against
    188, `lit` 172 against 188.
  - It under-reads `load` (120 against 145) and the radios (65 against 85).
- **`t2` is ~40 % fast.**
- **The pad's first lit frame starts ~5 ms before the line.** The line is
  said once the frame has gone out.
- The emulated mount at c40 (64–66 ms) is under silicon's 100 ms.

None of these numbers is a hardware number. The silicon table above is.

## Caveats

- One board, one generic C6, at a desk temperature. The spread across boots
  is a few ms, but board-to-board variation (flash part, slow-clock
  calibration) was not measured.
- The `used` store is one shape of "used" (repeated c40 pushes). A store
  written out by saves and panel writes may mount at a different cost.
- The basic project is small: one shader, 241 LEDs.
  - A heavier shader moves `lit` (its compile).
  - More LEDs move `frame1`, `lit` and `load`.
- The radio was idle (no saved network). A board that joins Wi-Fi at boot does
  that on the network thread, beside the boot. Its effect on first light was
  not measured.
