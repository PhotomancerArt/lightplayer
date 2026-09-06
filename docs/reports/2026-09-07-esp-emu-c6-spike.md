# fw-esp32c6 under Espressif's binary emulator (esp-emu 0.42.0): what runs, where it lies

Date: 2026-09-07 (overnight spike, 2026-09-06 23:30 → 2026-09-07 ~04:30)
Branch: `claude/emulator-strategy-qemu-esp32-028763`
Brief: `~/.photomancer/planning/lp2025/2026-09-06-2330-esp-emu-c6-spike/brief.md`
Companion: `docs/reports/2026-09-07-esp-emu-c6-peripheral-inventory.md` (the register inventory — the spec an in-house C6 emulator would need)
Scripts: `scripts/spike/esp-emu/`

Host-only. No board was touched, no serial port opened; every desk number
below is quoted from `docs/adr/2026-09-02-esp32c6-ram-split.md` and its
neighbours. The firmware is `fw-esp32c6` at `d6cfaa205` (main), default
features `esp32c6,server,radio`, built with the repo's own recipe.

## Verdict

| subsystem | verdict | evidence |
|---|---|---|
| Mask ROM → IDF 2nd-stage bootloader → app | **works** | ROM banner `ESP-ROM:esp32c6-20220919`, bootloader lists our 5-entry table, `Loaded app from partition at offset 0x10000` (§3) |
| esp-hal init, esp-rtos scheduler, embassy timers | **works** | heartbeats every 5,001–5,003 ms of emulated time; io_task's 1 ms `Timer::after` is what the sampled PCs sit in (§3, §7) |
| USB-Serial-JTAG (our shipped host link) | **lies** | SOF permanently asserted, EP1 always "data free", `INT_CLR` ignored: the firmware believes a draining host is attached and every frame vanishes; nothing ever arrives (§4) |
| UART0 (esp-hal async `Uart`, spike feature) | **works** | full wire protocol: hello, `stopAllProjects`, 10 filesystem writes, `loadProject`, `projectRead` stream, heartbeats — `lp-cli upload` exits 0 in 3 s (§5) |
| SPI flash + littlefs (`lpfs` partition) | **works** | blank flash → "Mount failed, formatting" → 7 project files written and read back; second boot in the same run not tested (`--save-state` exists) |
| eFuse / chip identity | **works, synthetic** | `baseMac 24:0a:c4:00:00:01`, `chipRevision 0.3`, `eui64 24:0a:c4:00:00:01:00:00` |
| esp-radio WiFi blob init + ESP-NOW driver | **works (does not hang)** | `WiFi RX config: enabled=true` in the emulator trace; firmware logs `ESP-NOW radio ready: device_id= channel=11` — note the **empty `device_id`**, unverified against the desk |
| On-device shader JIT (`lpvm-native rt_jit`) | **works** | examples/basic: 573 LPIR → 2,048 native insts, 8,192 B code, 52 ms; meteor compute+shader 53 + 27 ms; renders every frame after |
| RMT WS281x output | **not observable** | driver opens (`gpio=/gpio/18 ws281x_ch=0 rmt_slot=0 bytes=723`), frames are produced at ~100 fps, but esp-emu shows nothing for RMT unless an offset is unknown (none was). `--rmt-loopback TX:RX` exists and is the lever to observe the waveform — not exercised |
| Heap ledger (`[mem]`, heartbeat `memory`, `largestFreeBlock`) | **works, faithful** | meteor: after-load 216,056 B vs 220,384 B on the desk (−2.0 %); after both compiles 148–152 KB vs ~150 KB (§6) |
| Stack probe | **works, faithful** | meteor steady state 35,768 B vs 36,936 B on the desk (−3.2 %) |
| Frame rate / cycle timing | **lies** | meteor 100 fps (tick 8 ms) vs 26 fps on the XIAO C6: no cache, no flash wait states, ~1 insn/cycle (§7) |
| Wall-clock speed | **slow** | 68.6 M insns/s idle (0.43× a 160 MHz core); 5 s of emulated meteor rendering took 36.4 s of wall time (§7) |
| Determinism | **works** | two identical runs: boot bytes identical through the hello; the only diffs are heap-count digits that follow the host's connect timing (§7) |
| Hardware harnesses (`test_rmt`, `test_dither`, `test_json`, `test_gpio`, `test_shader_compile_incremental`, `memory_fs`) | **all boot and run** | `test_shader_compile_incremental` completes to `=== DONE ===` with real numbers; the RMT/dither ones run their loops silently as designed (§5.3) |
| GDB | **exists** (the brief assumed not) | `--gdb PORT` + Apple's `lldb` (riscv32-aware): registers, backtrace, memory read/write on the live image — how §4 was proven |
| Peripheral trace as an inventory source | **partial** | esp-emu logs UART, PCR ("SYSTEM"), LP_* ("RTC_CNTL"), eFuse, SPI-mem, ext-mem MMU, WiFi MAC and every *unhandled* address; GPIO, RMT, TIMG, SYSTIMER, INTPRI/PLIC, GDMA and USB_JTAG are modeled but silent, so the inventory's second source is a static scan of the ELF (§8) |

Bottom line for the (a)/(b) decision: esp-emu runs the whole product path
— boot, flash filesystem, wire protocol, on-device JIT, render loop — with
a heap and stack picture within 2–3 % of silicon, **provided the host link
is UART0**. It cannot stand in for the C6 on anything cycle-shaped (fps,
refill deadlines, WiFi-scan truncation), it shows no pin, and its
USB-Serial-JTAG model is the one place it actively deceives the firmware.
The inventory in the companion report is what an in-house emulator would
have to cover to match it.

## 1. Install

Authorized asset only; checksum verified before anything ran.

```
$ shasum -a 256 esp-emu-0.42.0-aarch64-apple-darwin.tar.gz
69df1ad11fe7f3d315e2ce17924a1cfe7bc10c2f47fb887a50449f327091671a  esp-emu-0.42.0-aarch64-apple-darwin.tar.gz
$ grep aarch64-apple-darwin SHA256SUMS
69df1ad11fe7f3d315e2ce17924a1cfe7bc10c2f47fb887a50449f327091671a  esp-emu-0.42.0-aarch64-apple-darwin.tar.gz
```

The tarball holds one Mach-O arm64 binary (`esp-emu`, 2.7 MB compressed),
installed under the session scratchpad, not the repo. `esp-emu --version`
→ `esp-emu 0.42.0`. The CLI (full `--help` in Appendix A) has more than the
brief expected: `--timeout`, `--exit-on <string>`, `--inject/--inject-on`
(scripted UART RX), `--uart-tcp`/`--uart1-tcp`, `--gdb PORT`/`--gdb-halt`,
`--strap-mode` (ROM download mode so esptool can drive it), `--save-state`
(flash persists across runs), `--rmt-loopback TX:RX`, `--efuse <file>`,
`--rom <elf>`, `--skip-bootloader`, `--skip-rom`, `--batch-size`.

## 2. The merged image

```
$ just build-fw-esp32c6                                   # 41 s, release-esp32, features esp32c6 (default = esp32c6,server,radio)
$ espflash save-image --chip esp32c6 --flash-size 4mb --merge \
    --partition-table lp-fw/fw-esp32c6/partitions.csv \
    target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6 merged-default.bin
Chip type:         esp32c6
Merge:             true
Partition table:   lp-fw/fw-esp32c6/partitions.csv
App/part. size:    2,403,616/3,145,728 bytes, 76.41%
```

espflash 3.3.0 prints no offsets, so they were checked by hand: `0x0`
begins `e9 03 02 20` (image magic, bootloader), `0x8000` begins `aa 50`
(partition entries nvs/bootctl/phy_init/factory/lpfs, then the `eb eb`
checksum row), `0x10000` begins `e9 06 02 20` (the app), and `0x310000`
(lpfs) is `ff` — a 4,194,304 B padded image. No `--bootloader` was needed:
espflash bundles `esp32c6-bootloader.bin` from its own resources, and the
emulator identifies it as `ESP-IDF v5.1-beta1-378-gea5e0ff298-dirt 2nd
stage bootloader, compile time Jun 7 2023 08:02:08`.

## 3. Booting the shipped image

```
$ RUST_LOG=debug esp-emu --chip esp32c6 --firmware merged-default.bin --timeout 45s --log-color never
```

stdout, complete (32 lines):

```
ESP-ROM:esp32c6-20220919
Build:Sep 19 2022
rst:0x1 (POWERON),boot:0x8 (SPI_FAST_FLASH_BOOT)
SPIWP:0xee
mode:DIO, clock div:2
load:0x4086c410,len:0xd48
load:0x4086e610,len:0x2d68
load:0x40875720,len:0x1800
entry 0x4086c410
I (4) boot: ESP-IDF v5.1-beta1-378-gea5e0ff298-dirt 2nd stage bootloader
I (4) boot: compile time Jun  7 2023 08:02:08
I (4) boot: chip revision: v0.3
I (5) boot.esp32c6: SPI Speed      : 40MHz
I (5) boot.esp32c6: SPI Mode       : DIO
I (5) boot.esp32c6: SPI Flash Size : 4MB
I (5) boot: Enabling RNG early entropy source...
I (5) boot: Partition Table:
I (5) boot: ## Label            Usage          Type ST Offset   Length
I (5) boot: 0 nvs WiFi data 1 2 9000 5000
I (5) boot: 1 bootctl Unknown data 1 6 e000 1000
I (5) boot: 2 phy_init RF data 1 1 f000 1000
I (5) boot: 3 factory factory app 0 0 10000 300000
I (5) boot: 4 lpfs Unknown data 1 82 310000 f0000
I (5) boot: End of partition table
I (5) esp_image: segment 0: paddr=10020 vaddr=40800000 size=18h (24) load
I (5) esp_image: segment 1: paddr=10040 vaddr=42000040 size=467c4h (288708) map
I (26) esp_image: segment 2: paddr=5680c vaddr=40800018 size=980ch (38924) load
I (30) esp_image: segment 3: paddr=60020 vaddr=42050020 size=1f0978h (2034040) map
I (185) esp_image: segment 4: paddr=2509a0 vaddr=40809824 size=a168h (41320) load
I (189) esp_image: segment 5: paddr=25ab10 vaddr=40813990 size=1e0h (480) load
I (193) boot: Loaded app from partition at offset 0x10000
I (193) boot: Disabling RNG early entropy source...
```

Then nothing, for 45 s, while the emulator kept executing app-range PCs:

```
[DEBUG esp_emu] PC=0x40801D62 cycles=3102041652 insns=3086289140
[INFO  esp_emu] Timeout reached after 45.00057s
```

Not one of our `[INIT] …` lines (esp-println, feature `jtag-serial`,
FIFO at `0x6000_F000`) reached stdout. The 90 sampled PCs, symbolized
against the ELF (`scripts/spike/esp-emu/symbolize-pcs.py`), are all
`esp_rtos::timer::timer_tick_handler`, `TimeDriver::arm_next_wakeup`,
`handle_interrupts`, `Instant::now` — a system idling on its 1 ms timer,
not a hang. `RUST_LOG=trace` (8 s, 422 KB) shows the app getting far past
boot: `[TRACE periph::wifi_mac] WiFi RX config: enabled=true` (esp-radio's
`wifi::new` completed — the default radio build does **not** hang), and
the four RISC-V trigger CSRs `0x7A0/0x7A1/0x7A2/0x7A5` rewritten on every
context switch (esp-rtos's stack-guard watchpoint).

Where the USB-Serial-JTAG output went is §4. To see the firmware speak at
all, the spike had to move the link (§5).

## 4. The USB-Serial-JTAG finding

The brief expected the connection monitor
(`lp-fw/fw-esp32c6/src/board/esp32c6/usb_connection.rs`, gated on SOF +
write timeouts) to decide "cable unplugged". **It decides the opposite.**
Evidence from the live default image through the gdb stub:

```
$ esp-emu --chip esp32c6 --firmware merged-default.bin --gdb 3334 --timeout 70s &
$ lldb -b -s cmds        # target create <elf>; gdb-remote 127.0.0.1:3334; …
(lldb) register read pc sp
      pc = 0x4209eba0      sp = 0x4086d4e0
(lldb) memory read --size 1 --count 1 --force 0x40854c40      # esp_println::serial_jtag_printer::TIMED_OUT
0x40854c40: 0x00
(lldb) memory read --size 4 --count 8 --force 0x6000f000      # USB_SERIAL_JTAG: EP1, EP1_CONF, INT_RAW, INT_ST, INT_ENA, INT_CLR, CONF0, TEST
0x6000f000: 0x00000000 0x00000002 0x0000000a 0x00000000
0x6000f010: 0x00000004 0x00000000 0x00000000 0x00000000
(lldb) memory write --size 4 0x6000f014 0x00000002             # INT_CLR.sof
(lldb) memory read --size 4 --count 6 --force 0x6000f000
0x6000f000: 0x00000000 0x00000002 0x0000000a 0x00000000
0x6000f010: 0x00000004 0x00000000
```

Read together:

- `EP1_CONF = 0x2` — `SERIAL_IN_EP_DATA_FREE` is always set, so esp-println's
  `fifo_full()` is never true and `TIMED_OUT` stays `0`: every boot line was
  written "successfully" into a FIFO that delivers nowhere. The trace has no
  `usbjtag` line at all — the block is modeled and silent.
- `INT_RAW = 0xA` — bit 1 (`SOF`) and bit 3 (`SERIAL_IN_EMPTY`) set, and
  writing `INT_CLR` does not clear SOF. `UsbConnectionMonitor::poll` reads
  SOF, clears it, sees it again 2 ms later: `is_enumerated()` is
  permanently true.
- `INT_ENA = 0x4` — `SERIAL_OUT_RECV_PKT` enabled. Only the connected path
  in `io_task` (`if conn.is_connected() { read_serial(…) }`) arms that
  interrupt through esp-hal's async read, so the monitor also latched
  `host_draining = true` (its writes "succeed").

So under esp-emu the shipped firmware runs its normal serving loop
believing a host is attached and draining, sends the hello, the heartbeats
and every log line into the void, and waits forever for a packet that no
host will send. It cannot detect the situation; nothing in its logs would
say so even if the logs were visible. `--strap-mode 0x04` documents a
USB-Serial/JTAG *download* console for esptool, but there is no CLI switch
that bridges the USB-SJ data path at runtime — UART0 (`--uart-tcp`, or
stdout) is the only host-visible serial.

## 5. Talking to it over UART0

### 5.1 The firmware side: `spike_uart0_link`

Commit `e8d64eeff`. Feature on `fw-esp32c6`, off by default, every hunk
`cfg`'d out: `io_task` takes esp-hal's async `Uart` on UART0 (GPIO16 TX /
GPIO17 RX, 921600 8N1, peripherals `steal()`ed so `init_board`'s ownership
tuple is untouched — everything below the split was already generic over
`embedded_io_async::{Read, Write}`); `UsbConnectionMonitor::is_enumerated`
returns true; `Esp32UsbSerialIo::write` (the harness serial) tees each
write to UART0 through the mask ROM's `uart_tx_one_char` at `0x4000_0058`,
the address esp-println's own `uart` printer uses for this chip. The
`[INIT] …` esp-println lines still go to USB-SJ and stay invisible; every
`log::` line and every wire frame rides UART0.

The shipped image is unchanged. Section sizes of the `just
fw-esp32c6-size-check` configuration (`esp32c6,server`) at the base commit
and at HEAD:

```
.rwtext 8064  .rwtext.wifi 55060  .data 15540  .data.wifi 480  .bss 299816
.rodata 288452  .text_gap 38940  .text 2034040  .stack 71544        (identical)
App/part. size:    2,403,616/3,145,728 bytes, 76.41%                 (identical)
```

(`rust-size -A` Total differs by 9 B — the build manifest's `dirty` flag,
"true" at the base build because that tree was checked out over a dirty
worktree; it lives outside the loadable sections.)

Boot over UART0, no client yet (spike image, `--uart-tcp 127.0.0.1:5555`,
the bridge transcribing):

```
M!{"id":0,"msg":{"hello":{"proto":20,"build":{"features":["node.button","node.clock","node.fluid","node.fixture","node.playlist","node.radio","node.shader","node.texture","svc.button","svc.radio-espnow","gfx.lpvm"],"package":"fw-esp32c6","commit":"d6cfaa2051ae","dirty":true,"profile":"release-esp32"},"hardware":{"radio":true,"totalLedBudget":null,"button":true,"boardId":"seeed/xiao-esp32-c6","baseMac":"24:0a:c4:00:00:01","chipRevision":"0.3","eui64":"24:0a:c4:00:00:01:00:00"},"deviceUid":null}}}
[INFO] fw_esp32c6: [fw-esp32c6] Shader backend: native JIT (lpvm-native rt_jit)
[WARN] fw_esp32_common::lp_fs: [FS] Mount failed (filesystem corrupt), formatting partition...
[INFO] fw_esp32_common::lp_fs: [FS] Formatted and mounted fresh filesystem
[INFO] fw_esp32c6: [fw-esp32c6] Hardware manifest: seeed/xiao-esp32-c6 (XIAO ESP32-C6)
[INFO] fw_esp32c6::output::rmt::esp32c6_rmt_ws281x_driver: Esp32C6RmtWs281xDriver: 2 WS281x channels for 2 declared (ch0_blocks=1 ch0_window_words=48 ch0_half_words=24)
[INFO] fw_esp32c6: [fw-esp32c6] ESP-NOW radio ready: device_id= channel=11
[INFO] fw_esp32_common::boot: Boot: scanning /projects for projects
[WARN] fw_esp32_common::boot: Boot: failed to list /projects: Filesystem error: list_dir /projects: no such file or directory
[INFO] fw_esp32_common::server_loop: [RECOVERY] boot complete (first frame served)
M!{"id":0,"msg":{"heartbeat":{"fps":{"avg":491.60336,…},"frame_count":2459,"loaded_projects":[],"uptime_ms":5002,"memory":{"freeBytes":265392,"usedBytes":60144,"totalBytes":325536,"largestFreeBlock":199173},"recovery":{"level":"green","resetReason":"power-on","bootCount":1,"safeMode":false,…},"link":{"parseFailures":0,"rxErrors":0,"queueFullDrops":0,"stalePartialFlushes":0},"identity":{"baseMac":"24:0a:c4:00:00:01"}}}}
[INFO] fw_esp32_common::server_loop: [perf] frame=2459 fps=491 elapsed=5002ms recv=0ms tick=0ms send=0ms total=0ms responses=0
[INFO] fw_esp32c6::stack_probe: [stack] heartbeat: high-water 11844 B of 71328 B (59484 B headroom)
```

### 5.2 The host side: why a pty cannot work on macOS, and `serial:tcp://`

The brief's plan — a Python `os.openpty()` ↔ TCP bridge and `lp-cli upload
serial:/dev/ttysNNN` — fails before a byte moves, at any baud rate:

```
Error: Failed to connect to server
Caused by:
    failed to open device session: link connection failed: byte stream I/O error: Failed to open serial port /dev/ttys003: Not a typewriter
```

`serialport` 4.9 on macOS sets the rate with the `IOSSIOSPEED` ioctl
inside `set_termios` (`posix/termios.rs:94`, unconditional), and the pty
driver answers `ENOTTY`. There is no pty-based fake for lp-cli on this
platform; the bridge is kept in `scripts/spike/esp-emu/pty-tcp-bridge.py`
only to document that.

Commit `eb6330304` adds the shuttle the byte-stream seam's own doc
anticipated: `lpa_client::stream::TcpByteStream` (a non-blocking TCP
socket behind `DeviceByteStream`; `set_signals` is a no-op) and a
`tcp://host:port` branch in `HostSerialEsp32Provider::connect` that uses
it and skips the DTR/RTS reset. The readiness engine's periodic
`ClientRequest::Hello` establishes the session. Nothing changes for real
ports. A transcribing proxy (`uart-tcp-proxy.py`) sits between the
emulator and lp-cli so every byte is logged with wall-clock offsets.

### 5.3 The walk

```
$ export SCRATCH=… WORKTREE=$PWD
$ PORT=5561 RUST_LOG=info scripts/spike/esp-emu/run-emu-tcp-walk.sh merged-spike.bin walkD-tcp 150 \
      -- upload examples/basic 'serial:TCP' --wait-timeout 90
==> lp-cli upload examples/basic serial:tcp://127.0.0.1:5571 --wait-timeout 90
lp-cli exit=0 after 3 s
```

lp-cli's own view (stderr, trimmed):

```
[device] readiness: engine start (budget 5s)
[device] readiness: hello request sent
[device] readiness: settled at Ready { hello: ServerHello { proto: 20, … board_id: Some("seeed/xiao-esp32-c6"), base_mac: Some("24:0a:c4:00:00:01"), chip_revision: Some("0.3"), … } }
[serial] [INFO] lpa_server::handlers: [mem] stop_all_projects before: 264716 B free / 60820 B used (258k / 59k)
[serial] [INFO] lpa_server::handlers: Loading project: projects/Basic
[serial] [INFO] lpa_server::handlers: [mem] load_project before: 258348 B free / 67188 B used (252k / 65k)
[serial] [INFO] lpa_server::project: [mem] project new after core project: 219k free / 98k used
[serial] [INFO] lpa_server::handlers: [mem] load_project after: 220532 B free / 105004 B used (215k / 102k)
[serial] [INFO] fw_esp32c6::output::rmt::esp32c6_rmt_ws281x_driver: Esp32C6RmtWs281xDriver::open: endpoint=esp32c6-rmt-ws281x:ws281x:local:D10 gpio=/gpio/18 ws281x_ch=0 rmt_slot=0 bytes=723
[serial] [INFO] lpc_engine::nodes::shader::shader_node: [shader-node] compilation starting (node=, 4365 bytes)
[serial] [INFO] lpc_shared::memory: [mem] shader compile before: 171k free / 146k used
[serial] [INFO] lpc_shared::memory: [mem] shader compile after: 157k free / 160k used
[serial] [INFO] lpc_engine::nodes::shader::shader_node: [shader-node] compilation succeeded (node=, elapsed=52ms, lpir_inst_count=573, lpir_func_count=12, lpir_import_count=7, final_inst_count=2048, final_code_size=8192 bytes, float=fixed)
Project uploaded and running.
```

The transcript (`walkD-tcp.uart.bin`, 68,332 B) shows the whole exchange:
the hello request and answer, `stopAllProjects`, `filesystem.write` ×7 and
`writeChunk` ×2 for `shader.glsl` (4,096 + 269 B), `loadProject` →
`{"handle":1}`, the three-part `projectRead` stream (`seq` 0–2), then
heartbeats with the project loaded:

```
M!{"id":0,"msg":{"heartbeat":{…"loaded_projects":[{"handle":1,"path":"/projects/Basic"}],"uptime_ms":10007,"memory":{"freeBytes":160828,"usedBytes":164708,"totalBytes":325536,"largestFreeBlock":65522},…
[INFO] fw_esp32_common::server_loop: [perf] frame=2345 fps=106 elapsed=5006ms recv=0ms tick=7ms send=0ms total=7ms responses=0
[INFO] fw_esp32c6::stack_probe: [stack] heartbeat: high-water 35468 B of 71328 B (35860 B headroom)
```

### 5.4 The hardware harnesses

Each built with `--features <harness>,esp32c6,spike_uart0_link` (defaults
on), merged, run 90 s under `RUST_LOG=info` with `--exit-on` where the
harness prints a sentinel. Every build succeeded; every run exited 0; no
esp-emu line matched `WARN|ERROR|unhandled|unimplemented|Bus fault|panic`.
Artifacts: scratchpad `harness/{1..6}/{build.log,fw.elf,merged.bin,run.stdout,run.stderr}`.

| harness | verdict | last lines (ANSI stripped) |
|---|---|---|
| `test_rmt` | boots, runs silently | `RMT test mode starting...` / `LedChannel::new: RMT slot 0, 256 LEDs (blocks=4 window_words=192 half_words=96)` / `RMT driver initialized (LedChannel created), starting chase pattern...` — then the chase loop, which logs nothing per cycle |
| `test_dither` | boots, runs silently | `DisplayPipeline test mode starting...` / `LedChannel::new: … 256 LEDs …` / `Creating DisplayPipeline with interpolation, dithering, LUT` / `Starting rotating 0-25% brightness ramp (16-bit -> pipeline -> 8-bit -> RMT)` |
| `test_json` | completes cyclically | one `M!{"id":0,"msg":{"heartbeat":{"fps":{"avg":60,…},"frame_count":38,"loaded_projects":[{"handle":1,"path":"projects/test"}],"uptime_ms":38000,"memory":{"freeBytes":325472,"usedBytes":64,"totalBytes":325536},…}}}` per second, 38 in 90 s |
| `test_gpio` | completes cyclically | `Testing GPIO0...` … `Testing GPIO21...` (12/13 skipped as USB D−/D+) / `Cycle complete, restarting...` every ~2 s |
| `test_shader_compile_incremental` | **completes**, `--exit-on "=== DONE ==="` | `[inc-shader-compile] summary case=examples-basic build=54.4ms ticks=92 max_slice=4.6ms max_slice_stage= peak=47.0KiB resident=18.5KiB after_drop=3.9KiB` / `[fw-check-json] {"kind":"total-summary","check":"shader-compile-stress","build_us":54361,"cases":1,"worst_slice_us":4625,"worst_peak_used":48132}` / `=== DONE ===` |
| `memory_fs` + server (app) | boots, serves | `Boot: found 0 entries in /projects` / `[RECOVERY] boot complete (first frame served)` / heartbeats at 491–492 fps, `freeBytes 266,792`, `largestFreeBlock 200,647`, `[stack] high-water 11432 B of 71960 B` |

`test_shader_compile_incremental` is the one whose numbers matter to a
host gate: they are emulated-cycle numbers (see §7) and will read faster
than silicon, but the heap figures (peak 48,132 B, resident 18,932 B,
after-drop 3,976 B) are allocator facts and should transfer.

## 6. Memory fidelity

Same project the ADR measured (`examples/meteor`), same telemetry. Desk
numbers are the ADR's "after" column (XIAO ESP32-C6, firmware
`4e463d805743`, bench 2026-09-02); emulator numbers are from
`walks/meteor.uart.bin` (firmware `d6cfaa205` + the UART0 feature, which
adds a `Uart` driver and its buffers to the heap and stack).

| figure | desk (ADR) | esp-emu | delta |
|---|---:|---:|---:|
| heap free at boot (`[mem] stop_all_projects before`, idle) | 265,040 B | 264,716 B (heartbeat idle: 265,392 B) | −324 B (−0.12 %) |
| heap free after project load (`[mem] load_project after`) | 220,384 B | 216,056 B | −4,328 B (−2.0 %) |
| heap free after both shader compiles | ~150 KB | 148 k (`[mem] shader compile after`), 152,320 B in the next heartbeat | ≈ 0 |
| `largestFreeBlock` steady | not recorded | 77,488 B | — |
| main stack size | 72,768 B | 71,328 B | −1,440 B (.bss grew between the two firmware commits; the probe measures the real gap) |
| stack high-water, meteor steady state | 36,936 B | 35,768 B | −1,168 B (−3.2 %) |
| fps, meteor steady | **26** | **100** (tick 8 ms) | **+3.8×** — see §7 |

Per-step meteor ledger under the emulator, for the record:

```
[mem] load_project before: 261100 B free / 64436 B used (254k / 62k)
[mem] project new after core project: 214k free / 103k used
[mem] load_project after: 216056 B free / 109480 B used (210k / 106k)
[compute-shader-node] compilation starting (node=, 2719 bytes)
[mem] compute shader compile before: 167k free / 150k used
[mem] compute shader compile after: 163k free / 154k used
[compute-shader-node] compilation succeeded (node=, elapsed=53ms, lpir_inst_count=193, lpir_func_count=1, …
[mem] shader compile before: 155k free / 162k used
[mem] shader compile after: 148k free / 169k used
[shader-node] compilation succeeded (node=, elapsed=27ms, lpir_inst_count=196, lpir_func_count=4, lpir_import_count=1, final_inst_count=798, final_code_size=3192 bytes, float=fixed)
heartbeat memory={"freeBytes":152320,"usedBytes":173216,"totalBytes":325536,"largestFreeBlock":77488}
[stack] heartbeat: high-water 35768 B of 71328 B (35560 B headroom)
```

Reading: the allocator is the firmware's own (`esp_alloc`, two regions,
325,536 B total — the emulator reports exactly the ADR's heap total), the
RAM map is the real one (the bootloader's `dram2_seg` reclaim shows up as
the second region), and what differs between desk and emulator is the
firmware commit plus the extra `Uart` driver — not the emulation. The
heap-shaped questions (`largest_free_block` before a `ProjectRead`, the
load gate, compile transients) can be asked here and answered within the
noise of a firmware revision. The ADR's own "before" column (300 KB heap,
overflowing stack) could be reproduced by rebuilding that commit; not
done tonight.

## 7. Determinism and speed

Two identical walks (`det1`, `det2`: same image, same command, same
6 s settle before lp-cli, run concurrently):

- Bytes identical from the ROM banner through the hello and the
  `stopAllProjects` request.
- 26 differing lines out of ~130 device lines, all of the form
  `[mem] load_project before: 258348 B free` vs `255788 B free` — the
  heap counters. The host's connect moment relative to the 5 s heartbeat
  and log-line allocations differs by wall-clock, and everything after
  inherits the offset. With host timing pinned (esp-emu's own
  `--inject-on` scripting instead of lp-cli) the transcript should be
  byte-identical; not verified tonight.
- Emulated time is deterministic: every heartbeat lands at
  `uptime_ms` 5,001–5,003 apart in both runs; `[perf] elapsed=5001ms`.

Speed, from esp-emu's own counters and the proxy's timestamps:

| measurement | value |
|---|---|
| default image, idle server, single emulator, `RUST_LOG=debug` | 3,086,289,140 insns / 3,102,041,652 cycles in 45.0 s = **68.6 M insns/s**, 0.43× a 160 MHz core |
| `RUST_LOG=trace`, three emulators concurrent | 1,637,592,982 insns in 120 s = 13.6 M insns/s |
| wall time to `entry 0x4086c410` (ROM done) / `Loaded app` / `boot complete`, meteor walk, `RUST_LOG=info` | 0.05 s / 0.33 s / **0.45 s** after the proxy connected |
| same, `det1` (three emulators concurrent) | 0.20 s / 1.44 s / 2.13 s |
| meteor rendering: emulated 5 s between heartbeats (`uptime_ms` 5003 → 10008 → 15016) | **36.4 s and 37.5 s of wall time** — 0.14× real time while the JIT'd shader runs |
| meteor frame time | 8 ms emulated (`[perf] tick=8ms`, 100 fps) vs 38 ms on silicon (26 fps) |

The last two rows are the same fact from both sides: esp-emu has no cycle
model (≈1 instruction per cycle, no flash cache, no pipeline), so
emulated time runs ~4–5× *faster* than silicon per frame while wall time
runs ~7× *slower*. Anything measured in `[perf]`, `elapsed=`, fps or
`slice_us` under the emulator is a count of instructions in disguise. The
heap and stack figures do not depend on this.

Two operational notes. (1) With `--uart-tcp` and `RUST_LOG=debug` or
`trace`, the process stopped emulating at `--timeout` but did not exit in
2 of 2 runs (the `info` runs exited every time, 5 of 5); the walk script
hard-kills it 20 s after the timeout. (2) esp-emu's TCP server does not
buffer UART output before a client connects — the proxy connects at
launch to keep the ROM banner.

## 8. Peripheral inventory (summary)

The full tables are in
`docs/reports/2026-09-07-esp-emu-c6-peripheral-inventory.md`. Two
sources, because esp-emu's trace is not a bus log: it logs UART, PCR
(as "SYSTEM"), the LP blocks (as "RTC_CNTL"/"LP_AON"), eFuse, the SPI
flash controller, the ext-mem MMU, the WiFi MAC and every *unhandled*
address, but GPIO, RMT, TIMG, SYSTIMER, INTPRI/PLIC, GDMA and USB_JTAG are
modeled silently. So the second source is a static scan of the shipped
ELF (`scripts/spike/esp-emu/mmio-scan.py`: every `lui`+offset load/store
into `0x2000_0000` or `0x6000_0000..0x600F_FFFF`, 748 register sites):

| block (PAC name) | distinct regs | static sites | who |
|---|---:|---:|---|
| WIFI MAC/BB `0x600A_0000` (PAC's IEEE802154 window; undocumented) | 169 | 762 | esp-radio blob |
| MODEM_SYSCON `0x600A_9800` | 28 | 184 | esp-radio |
| LP_APM0 `0x6009_9800` | 46 | 171 | esp-radio / esp-hal init |
| PMU `0x600B_0000` | 60 | 143 | esp-hal clocks, esp-radio |
| PCR `0x6009_6000` | 37 | 93 | esp-hal (clock gates/resets for every driver) |
| SYSTIMER `0x6000_A000` | 5 | 64 | esp-hal `Instant::now`, esp-rtos |
| I2C_ANA_MST `0x600A_F800` | 36 | 56 | esp-radio (PHY) |
| LP_AON `0x600B_1000` | 5 | 37 | reset cause, RTC ledger |
| MODEM_LPCON `0x600A_F000` | 5 | 31 | esp-radio |
| LP_WDT `0x600B_1C00` | 6 | 21 | RWDT feeder |
| LP_IO `0x600B_2000` | 8 | 16 | esp-hal GPIO init |
| EFUSE `0x600B_0800` | 7 | 15 | chip identity |
| USB_DEVICE `0x6000_F000` | 6 | 14 | esp-println, connection monitor, esp-hal USB-SJ |
| LP_CLKRST `0x600B_0400` | 3 | 13 | esp-hal clocks |
| INTPRI `0x600C_5000` | 5 | 13 | esp-hal interrupt priorities |
| TIMG0 `0x6000_8000` | 3 | 12 | esp-rtos tick |
| APB_SARADC `0x6000_E000` | 3 | 12 | esp-hal RNG seed |
| PLIC_MX `0x2000_1000` | 4 | 9 | interrupt enable/threshold |
| RMT `0x6000_6000` | 5 | 9 | WS281x driver (**under-counted** — the driver holds a base pointer across branches) |
| INTERRUPT_CORE0 `0x6001_0000` | 6 | 9 | interrupt→CPU mapping |
| TIMG1, UART0, SPI1, GPIO, ASSIST_DEBUG, HP_APM, RNG, LP_APM | 1–3 each | 1–4 each | |

Unhandled by esp-emu during the whole walk (reads return 0, writes
dropped — all harmless for us): IO_MUX pad registers
`0x6009_0044..0x6009_007C` (GPIO16–30: UART0 pads and the flash pins),
MODEM_SYSCON `0x600A_980C/14/1C`, MODEM_LPCON `0x600A_F00C/10/18/20`,
LP_WDT `0x600B_1C54`, LP_IO `0x600B_2400/08/14/7FC`, ASSIST_DEBUG
`0x600C_2044/74`; CSRs `0x7A0/0x7A1/0x7A2/0x7A5` (debug triggers — the
hardware watchpoint esp-rtos puts on the main stack's guard word is
therefore **silently dropped** under esp-emu; only whatever esp-rtos checks
in software at a context switch remains) and `0x800/0x801`
(Espressif custom, written once at boot). ROM functions intercepted:
`usb_serial_tx_one_char` ×231 (the ROM banner), `rom_i2c_writeReg_Mask`
×183 / `rom_i2c_readReg_Mask` ×14 (PHY/clock calibration), `ets_printf`
×23 (the bootloader), `usb_serial_tx_flush` ×9, `uart_tx_flush` ×4,
`ets_install_putc1` ×1.

### What we would have to build

For our firmware to run on an in-house C6 emulator the way it ran here,
the minimum is the set above with these fidelity levels:

1. **CPU**: RV32IMAC with the Zicsr/machine-mode set, `mret`, PLIC-style
   external interrupts through INTPRI/INTERRUPT_CORE0/PLIC_MX, the four
   trigger CSRs as no-ops (or better, honoured — esp-emu drops them and so
   loses esp-rtos's stack guard), and the Espressif custom CSRs `0x800/0x801`
   as writable scratch. No FPU (the C6 has none).
2. **Memory map**: ROM at `0x4000_0000` (the mask ROM must be *executed* or
   stubbed at the 23 entry points esp-emu intercepts — `ets_printf`,
   `uart_tx_one_char`, `rom_i2c_*`, `usb_serial_*`, the spiflash legacy
   table, `software_reset`), HP SRAM `0x4080_0000` 512 KB including the
   bootloader's `dram2_seg` at `0x4086_E610`, flash cache windows
   `0x4200_0000` (icache) mapped through EXTMEM's 64 KB MMU pages, LP RAM.
3. **Boot chain**: either run the real ROM + IDF bootloader from a merged
   image (esp-emu's way; needs SPI1 flash controller + MMU + eFuse) or a
   loader that places the app segments and jumps to `_start_rust` — the
   latter loses the bootloader partition-table checks our `flash-size`
   note in `.cargo/config.toml` cares about.
4. **Timers**: SYSTIMER (5 registers, the `Instant` source) and TIMG0's
   timer0 + watchdog (esp-rtos tick, RWDT via LP_WDT) — with a cycle model
   we choose, which is the whole point of building one.
5. **UART0** with the ROM's polling TX and esp-hal's interrupt-driven
   RX/TX (FIFO, thresholds, `RXFIFO_TOUT`), bridged to a host socket.
6. **USB_SERIAL_JTAG**: EP1/EP1_CONF/INT_* with an *honest* host model —
   SOF only while a host is attached, `EP_DATA_FREE` only while it drains,
   `RECV_PKT` when it sends — so the connection monitor can be tested
   rather than deceived.
7. **SPI flash** (SPI1 legacy + `esp-storage`'s ROM routines) with the
   `lpfs` partition persistent across runs; **RMT** TX channels with the
   48-word memory blocks, wrap mode, the threshold interrupt the refill
   ISR depends on, and a waveform sink (bit-decoded WS281x frames) — this
   is the pin observation esp-emu lacks; **GPIO/IO_MUX/PCR** enough for
   the drivers' init sequences to complete.
8. **Radio**: not emulated. The esp-radio blob's 762 register sites in the
   undocumented WiFi MAC window mean an in-house emulator runs the
   `--no-default-features --features esp32c6,server` image, or stubs the
   window to return "done" bits. Under esp-emu the blob initialises
   against a modeled MAC (`WiFi RX config: enabled=true`) but ESP-NOW
   reports an empty `device_id` — the first thing to compare on the desk.

Not needed for any exercised path: I2C, SPI2, LEDC, PCNT, MCPWM, PARLIO,
TWAI, I2S, SDIO, AES/SHA/RSA/ECC/HMAC/DS, GDMA (no driver in this image
uses DMA), TRACE, TEE/APM beyond the reads esp-hal does at init.

## 9. Desk follow-up (a XIAO C6 was not on the bus tonight)

Passive check by the coordinator: no Espressif USB device attached. These
are the comparisons one board would settle in the morning, each a
one-liner against artifacts already in the scratchpad:

1. **Same harness, silicon vs emulator.** Flash `harness/5/fw.elf`
   (`test_shader_compile_incremental,esp32c6,spike_uart0_link` — the tee
   makes it print on both USB and UART0), capture the `[fw-check-json]`
   lines, diff against `harness/5/run.stdout`: `peak_used`, `resident_used`,
   `after_drop_used` should match to the byte; `build_us`/`max_slice_us`
   quantify the cycle-model lie (§7).
2. **Same project, heap ledger.** `just demo-esp32c6-host` with
   `examples/meteor` at this commit, then grep `[mem] load_project after`,
   `[mem] shader compile after`, `largestFreeBlock`, `[stack] high-water`
   and put them next to §6's column — this removes the "different firmware
   commit" caveat from the 2 % delta.
3. **Same serial script, diffed.** Run `lp-cli upload examples/basic
   serial:/dev/cu.usbmodem…` and `… serial:tcp://…` (via the proxy) and
   diff the two `.uart.bin` transcripts with heap digits masked: every
   remaining difference is an emulator divergence in ordering or content.
4. **`device_id=` on the desk.** The ESP-NOW driver's log line under
   esp-emu has an empty `device_id`; confirm what silicon prints.
5. **USB-SJ negative control.** On the desk, with the port *closed* by
   every host program, the connection monitor should log
   `[io_task] host not draining; dropping protocol writes` within ~0.5 s;
   under esp-emu it never can (§4). This is the one behaviour esp-emu
   cannot test at all.

## 10. Reproduce

```bash
# 0. install (scratchpad), verify
curl -fsSLO https://github.com/espressif/esp-emulator/releases/download/v0.42.0/esp-emu-0.42.0-aarch64-apple-darwin.tar.gz
curl -fsSLO https://github.com/espressif/esp-emulator/releases/download/v0.42.0/SHA256SUMS
shasum -a 256 -c <(grep aarch64-apple-darwin SHA256SUMS) && tar xzf esp-emu-0.42.0-aarch64-apple-darwin.tar.gz
# 1. shipped image → silent (§3)
just build-fw-esp32c6
espflash save-image --chip esp32c6 --flash-size 4mb --merge --partition-table lp-fw/fw-esp32c6/partitions.csv \
    target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6 merged-default.bin
RUST_LOG=debug esp-emu --chip esp32c6 --firmware merged-default.bin --timeout 45s --log-color never
# 2. the USB-SJ registers, live (§4)
esp-emu --chip esp32c6 --firmware merged-default.bin --gdb 3334 --timeout 70s &
lldb -b -o 'target create target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6' -o 'gdb-remote 127.0.0.1:3334' \
     -o 'memory read --size 4 --count 8 --force 0x6000f000' -o 'process detach' -o quit
# 3. spike image (UART0 link) and the walk (§5)
cd lp-fw/fw-esp32c6 && touch src/main.rs && \
  cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server,radio,spike_uart0_link && cd ../..
espflash save-image --chip esp32c6 --flash-size 4mb --merge --partition-table lp-fw/fw-esp32c6/partitions.csv -S \
    target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6 merged-spike.bin
cargo build -p lp-cli
export SCRATCH=<dir holding esp-emu/ and the scripts> WORKTREE=$PWD
PORT=5561 RUST_LOG=info scripts/spike/esp-emu/run-emu-tcp-walk.sh merged-spike.bin walk 150 \
    -- upload examples/meteor 'serial:TCP' --wait-timeout 90
# 4. harness (§5.4): same build with --features test_shader_compile_incremental,esp32c6,spike_uart0_link, then
esp-emu --chip esp32c6 --firmware harness.bin --timeout 90s --exit-on '=== DONE ===' --log-color never
# 5. shipped image unchanged
just fw-esp32c6-size-check
```

## Appendix A — `esp-emu --help` (0.42.0), abridged to the options

```
Usage: esp-emu [OPTIONS] --chip <CHIP> --firmware <FIRMWARE>
      --chip <CHIP>            [esp32c3, esp32c5, esp32c6, esp32h2, esp32p4, esp32s31]
      --firmware <FIRMWARE>    merged flash image
Networking: --net <user|tap,ifname=..|vmnet>  --wifi-ssid  --wifi-password
BLE:        --elf <ELF>  --ble-hci <tcp:host:port|hci0>
Thread:     --thread-sim <bind:PORT,peer:HOST:PORT>
ESP-Hosted: --hosted <bridge:host:/sock|bridge:slave:/sock>
ROM:        --rom <ROM ELF>   (default: embedded per chip; C6 = 489,768 B, 3,699 symbols, 23 functions patched)
Advanced:   --timeout <DUR>  --exit-on <STR>  --save-state  --efuse <FILE>  --skip-bootloader  --skip-rom
            --batch-size <N=50000>  --inject <PAYLOAD> / --inject-on <TRIGGER> (repeatable, paired)
            --uart-tcp <HOST:PORT>  --uart1-tcp <HOST:PORT>  --gdb <PORT>  --gdb-halt
            --strap-mode <HEX>  --rmt-loopback <TX:RX[,..]>  --log-color <auto|always|never>
Subcommands: update
```
