# fw-esp32c6

`fw-esp32c6` is the reference embedded LightPlayer firmware target for ESP32-C6.

This is the main bare-metal product path: GLSL shaders are compiled on the
device at runtime and executed from RAM. Do not replace this with host/browser
precompilation, and do not feature-gate the compiler out of the embedded
compile/execute path to solve build, size, or `no_std` issues.

## Responsibilities

- ESP32-C6 boot and board initialization.
- USB/JTAG serial transport.
- Flash-backed or memory-backed LightPlayer filesystem.
- `lp-server` hosting on device.
- LED output through the shared `lp-ws281x` RMT driver (see below).
- Root-owned hardware capabilities such as buttons and ESP-NOW radio support.
- Firmware check and test harness modes behind feature flags.

Chip-generic firmware logic (server loop, transport, logger, boot, output
provider, littlefs glue) lives in `fw-esp32-common` and is consumed here;
this crate keeps only what is genuinely ESP32-C6: board init, RMT register
code, USB-Serial-JTAG, the recovery backend and abort-tier panic path, and
the hardware test harnesses (see
`docs/adr/2026-07-29-per-chip-fw-toolchains.md` for the seam rules).
Shared firmware plumbing belongs in `fw-core`. Host-local runtime lifecycle
belongs in `fw-host`. Browser Studio simulation belongs in `fw-browser`.

## WS281x output

`src/output/rmt/` implements `lp_ws281x::RmtHw` for this chip and registers one
driver at boot; the sequencing lives in `lp-fw/lp-ws281x` and is shared with
`fw-esp32s3` and `fw-esp32v3`. The chip has **two RMT TX channels** plus two
RX channels, all with 48-word memory blocks; a TX window may absorb the RX
blocks (esp-hal permits `memsize` up to 4 for channel 0, and the pre-migration
legacy driver always ran that way).

The RMT block plan is **computed at driver init from the board manifest's
declared `/rmt/ws281xK` count** — one build serves every shape, no cargo
feature (the former `ws281x_2blocks` feature is gone):

| Declared channels | Plan | Window | Refill half |
| --- | --- | --- | --- |
| 2 (XIAO C6) | one block each | 48 words | 24 words (~30 µs) |
| 1 (e.g. C6 DevKitC) | all 4 blocks, RX absorbed | 192 words | 96 words (~120 µs) |

**2 channels × 24-word halves ships on the XIAO C6**, decided at roadmap M5's
G1 gate (`2026-08-01-1459-rmt-priority-hli`, phase P4) from measurement, not
arithmetic: under a WiFi scan, that config's first channel truncates 28.0 %
of frames (1,380/4,932) vs a single-channel 48-word-half config's 0.49 %
(25/5,071) — margin drops truncation 57×, but does not eliminate it, and the
C6 sits at `Priority::max()` already (RISC-V has no headroom above maskable
levels to raise into). Yona's call: "wifi load isn't a big concern for actual
used installs. that's mostly an editing concern, unless we start streaming
opc or e131." **Reopen trigger**: OPC/E1.31-over-WiFi streaming lands (that
is also the unmeasured S4 scenario — sustained UDP while associated — so S4
measurement precedes any config revisit).
`docs/debt/c6-scan-truncation-accepted.md` carries the 2-channel tradeoff.

A 1-channel board self-serves the wide window — 96-word halves more than
double the margin of the 0.49 % config above — with no build flag: declaring
one `/rmt/ws281x` resource is the whole configuration. The hardware harnesses
(`test_rmt` and friends) drive one strip and publish the 1-channel plan, so
every harness run exercises the widened window on silicon.

`--features ws281x_telemetry` adds a periodic `[WS281X]` counters line per
channel, in the same field order the classic firmware prints, including the
entry-delay fields (see `lp-fw/lp-ws281x`'s README for what they measure —
they are what attributed the scan truncation above to interrupt-to-service
latency rather than refill work).

`--features stress_s2` and `--features stress_s3` add embassy tasks that
replace the ESP-NOW radio driver registration with a continuous load
generator — S2 repeats active WiFi scans to completion, S3 sends bursts of
ESP-NOW broadcasts — for reproducing the M5 stress matrix on the desk. Both
imply `radio`, are off by default, and cost the shipping image nothing
(byte-identical size-check with either enabled). They are a measurement tool,
not a product feature.

Which pins the two channels drive is authored, not fixed: an `Output` node names
a board label (`ws281x:local:D10`) and the driver binds that GPIO when the project
opens the endpoint. The desk jig used for M5 wires three strips — D10/GPIO18,
D9/GPIO20, and D8/GPIO19. Only the first two can be RMT channels; D8 is spare
for a future SPI-class output — **not an RMT resource**, since the chip has
no third TX channel.

## Emulator seams

The image carries an emulator seam table, `LP_SEAM_TABLE` (`src/seams/`,
instantiated with `fw_esp32_common::seam_table!`), and one seam: the LED
wait step, `lp_seam_ws281x_wait_step`, called from `write`'s spin closure in
`src/output/rmt/esp32c6_rmt_ws281x_driver.rs` before the frame-timeout
check. On silicon it costs one call, one no-op hint (`addi zero, zero, 1`)
and one return per spin iteration — the spin only ever waits, so it changes
how often the loop polls, not what it waits for — and the table is 88 B of
flash `.rodata` nobody reads. **No RAM, no IRAM.** Under an emulator that
engaged `led=fast` (only Studio's Devices-page emulated boards), the call
returns and the hart parks until an interrupt; everywhere else it is the
same instruction stream as silicon.

In the split image the table is a core root, so it and the seam function
land in the core (`lp-fw-split build` prints both placements). The
`test_seam_abi` harness adds two TEST ONLY seams to prove the generated call
shims keep their arguments and result through this profile's LTO. The ABI
and its rules are `lp-base/lp-seam/README.md`; why seams exist is
`docs/adr/2026-10-05-emulator-seams.md`.

## Wi-Fi (the LAN link and the cloud relay)

A board with a saved network (`/.lp/network.json`, written by `lp-cli wifi
add` or Studio) joins it by itself. Once joined it serves the secure lp-link
at `ws://<board>/link` (port 80), and answers mDNS for `lp-xxxx.local` and
DNS-SD for `_lightplayer._tcp`. The IP stack (embassy-net/smoltcp, DNS on),
the station, the LAN endpoint, the relay task and mDNS run on their own
thread, `lp-net` (`src/net/`, 8 KB stack), all started from `core_boot`, so a
core-only board has them too. The radio's C heap stays in `HEAP_RADIO`.
ESP-NOW (the Radio node) is off while the board uses Wi-Fi. Decisions and
measured costs: `docs/adr/2026-10-07-c6-wifi-link.md`.

**Updates over Wi-Fi.** Core-only serves the over-the-air update protocol
on the LAN link too (`fw-esp32-common`'s `radio_link::core_only_links`): it
answers the link's key from the device store itself, so the key decides
who may flash, and a LAN link opened in update mode advertises a window of
8. A trial core with a saved network that hears from no host for three
minutes gives the board back to its last good core (`src/ota/core_only.rs`).
`lp-cli link capture lan:<board> --ota-offer <dir>` drives an update over
it; `just test-emu-c6-ota-lan` walks it on the emulated LAN with no cable.
See `docs/adr/2026-10-06-ota-update-protocol.md` (amendment of 2026-10-07).

**The relay task** (`src/net/relay_task.rs`; the loop is
`fw-esp32-common/src/net/relay/`) dials `lightplayer.app:80` when the station
has an address, Cloud relay is on and the board holds an account key
(`lpc_relay::RelayClient::may_dial`), registers, and carries a browser's
sealed session on a route. **One network slot** is shared by the LAN and the
relay: while one holds the session, the other's newcomer is told busy unless
its handshake proves the holder's own key, which takes the slot. The leg's
TCP and WebSocket buffers (5,830 B) exist only while the board may dial and
the route's outgoing frame only while a route holds the slot, so a board with
no account key holds no relay memory. Status: `NetworkStatus.relay`; the
heartbeat says `[relay] state=… routes=… rx=… tx=… · … · pictures N
idle|watched|off` (there is no line on a state change: a 688 B line crossed
a flash page, see the budget ADR's ledger). The board speaks relay protocol
2: its hello carries the build's version, and it sends the relay its
project's name and, when asked, a picture of its lamps (the frame hook
makes it, `src/net/relay_probes.rs`). Costs and what is not yet measured on silicon:
`docs/adr/2026-10-06-cloud-relay.md` ("Device side").

`LP_RELAY_HOST=<host>[:port]` at build time (`build.rs`) makes a **desk
image** that dials that host instead and prints `[INIT] desk image: the relay
is …` at boot. The product image always dials `lightplayer.app:80`; do not
release a desk image.

The diagnostics are off by default and never shipped:

- `net_thread_stack_diag`: `lp-net`'s stack high water;
- `radio_dma_diag`: where the radio's C blocks live, plus the heap map;
- `heap_map_diag` / `heap_track_diag`: holes and live spans by address, the
  backtrace of each live block since the station joined (its table is in
  LP SRAM, so the heap keeps the shipped layout), and `[bigalloc]`, every
  ask of 2 KB or more.

## Common Commands

Run on a connected ESP32-C6:

```bash
just demo-esp32
```

Target check from the workspace root:

```bash
cargo check -p fw-esp32c6 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

For linked firmware builds, size measurements, or bloat analysis, run from this
crate directory so the crate-local linker configuration is active:

```bash
cd lp-fw/fw-esp32c6
cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
rust-size ../../target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6
```

## Flash Budget And Diagnostics

The app image must fit the 3.25 MB `factory` partition (`0x340000`; `lpfs` is
the 704 KB after it, at `0x350000`). The 2026-10 repartition spent the
reservation the budget ADR held for Wi-Fi: a board still on the old layout
(`factory` 3 MB, `lpfs` 960 KB at `0x310000`) keeps its files through Studio's
Update firmware or `lp-cli hardware lpfs migrate`, which move them; see
`docs/adr/2026-10-02-c6-repartition-and-layout-migration.md`. The firmware
reads its `lpfs` partition from the flashed table at boot, and never formats
over an old-layout filesystem it finds instead (it boots on a memory
filesystem and says `fs: legacy_held` in its hello).

`.cargo/config.toml` buys ~155 KB of
that by giving up on-device diagnostics, and `build-std`'s `optimize_for_size`
adds ~50 KB more:

| Flag | Saves | Cost |
|---|---|---|
| `-Zlocation-detail=none` | 59,488 B | panics lose `file:line` |
| `-Zfmt-debug=none` | 95,584 B | `{:?}` formats to nothing |
| `optimize_for_size` (build-std) | 51,344 B | none measured (render loop unaffected) |

**When you need real panic output while debugging, delete the `fmt-debug` line
first** — it is the one that turns `panicked at src/foo.rs:12: bad state {x:?}`
into `panicked at <redacted>:0:0:` with an empty payload. Drop
`location-detail` too if you need line numbers; both are one-line reverts, and
neither is needed for a local debug build to be correct.

Note that `ESP_LOG` does *not* control this firmware's own log level: the
logger installs a runtime `log::max_level()` (see `src/logger.rs`) seeded to
Info and changeable from the client with the wire `SetLogLevel` command.

Check headroom at any time — this is the same check pre-merge CI runs:

```bash
just fw-esp32c6-size-check
```

Background and the decisions behind the budget (including why the ~500 KB WiFi
blob is kept) are in `docs/adr/2026-07-28-esp32c6-flash-budget.md`; the
repartition that spent its lpfs reservation is
`docs/adr/2026-10-02-c6-repartition-and-layout-migration.md`.

## Moving a Board's Files (the 2026-10 repartition)

Studio's Update firmware migrates an old-layout board itself. From a
terminal, with the board in its bootloader on `<port>`:

```bash
cargo run -q -p lp-cli -- hardware lpfs report --port <port>    # how full, does it fit 704 KB
cargo run -q -p lp-cli -- hardware lpfs save --port <port> --out ~/lp-backups/
cargo run -q -p lp-cli -- hardware lpfs migrate --port <port> --merged <merged.bin>
just flash-fw-esp32c6 migrate=1                                  # the flash recipe, migrating
```

`just flash-fw-esp32c6` refuses to write a table that does not match the
board's (exit 3) unless told `migrate=1` or `discard=1`.

## The tree store as `lpfs` (`fs-tree`, never shipped)

`--features esp32c6,server,fs-tree` makes `lpfs` the tree store
(`lp-base/lp-tree-store`, plan
`lp2025/2026-10-08-2339-tree-store-firmware-and-emulator`). It is **never**
in `lp-fw/builds/served.json`, a release or `bless-chips`: tests, the
emulator walks and the CX1 sitting build it. Every board file goes through
it, because they all go through `base_fs`.

- **Flash:** `src/tree_flash.rs`, over esp-storage through the
  word-alignment shim (`fw_esp32_common::aligned_nor_flash`): esp-storage
  here refuses an unaligned offset or length and panics on an unaligned
  buffer, and the store writes records at any byte offset, so every read
  goes through an aligned bounce buffer and every program is padded to
  whole words with `0xFF`. **Every program reaches the ROM word-aligned.**
- **Hasher:** `src/hw_sha.rs` (`StoreSha256`): the SHA accelerator, or
  software `sha2` while a boot hash holds it. Moved out of `ota/` because an
  unsplit build has no `ota/`.
- **Boot words** (`fw_esp32_common::tree_fs`; D2):
  - `[FS] tree store mounted (<n> sectors, <free> free, root <seq>)` — `fs: mounted`;
  - `[FS] tree store formatted (…)` — a partition with no store (blank,
    littlefs, an interrupted first format), when the legacy probe holds
    nothing — `fs: formatted`; the format erases every sector (≈ 3.7 s on
    silicon at 176 sectors, D4);
  - `[FS] tree store refused: a newer or damaged store header — files kept
    (<why>); using memory FS, access locked` — `fs: refused`: nothing
    written, the device store locked, Bluetooth off; read the files with
    `lp-cli hardware tree extract`;
  - a pre-repartition filesystem at the old offset is held, as today
    (`legacy_held`). The legacy probe keeps littlefs linked.
- **Update guard** (D8): the split build's update session refuses a core
  install whose embedded manifest lacks the feature `fs.tree` — a littlefs
  core would format the store
  (`fw_esp32_common::fs_tree_core_guard`).

`fs-tree` + `memory_fs` is a `compile_error!`. See
`docs/adr/2026-10-10-fs-tree-refused-state-and-update-guard.md`.

## Feature Notes

The default feature set targets ESP32-C6 with server and radio support. Many
`test_*` features select focused firmware harnesses for hardware validation,
profiling, or smoke tests. Keep feature additions honest: test and check modes
may narrow behavior for a harness, but the normal firmware path must preserve
runtime shader compilation on device.

`spike_uart0_link` (off by default) moves the host link to UART0 so the wire
protocol can run under Espressif's binary emulator, whose USB-Serial-JTAG
model has no real host behind it (`docs/reports/2026-09-07-esp-emu-c6-spike.md`).
It costs the shipped image nothing — every hunk is `cfg`'d out, verified by
`just fw-esp32c6-size-check`.

### `test_f32_softfloat` — IEEE f32 on a chip with no FPU

The C6 is RV32IMAC: no F extension. It can still execute **f32 semantics**
through soft-float calls, which makes it the only rv32 *hardware* oracle for
f32 until an F-bearing part (ESP32-S31) is on the desk.

```bash
just fwtest-f32-softfloat-esp32c6 /dev/cu.usbmodemXXXX
```

**Pass the port explicitly.** Several ESP32 boards are usually attached and
auto-detection has flashed the wrong one before; the recipe refuses rather than
guessing. The harness configures **no GPIO** — on the C6, GPIO12/13 are the USB
D-/D+ lines and driving them costs a physical replug.

Two halves. `abi_probe` calls `__addsf3`/`__ltsf2`/… directly and compares raw
result words against IEEE reference bit patterns computed **off-device** — on
this chip a Rust `a + b` on two `f32`s *is* a call to `__addsf3`, so computing
the expected value here would compare the routine to itself. What that measures
is the **mask ROM**: the linker resolves these names through `esp-rom-sys`'s
`esp32c6.rom.rvfp.ld` to Espressif's ROM `rvfplib`, a different implementation
from the `compiler_builtins` the host emulator runs. `shader_cases` then
compiles a GLSL shader on the device in `FloatMode::F32`, JITs it, and calls it.

**This is the only configuration in this crate that turns on `float-f32`**, and
it does so deliberately: the shipping image runs Fixed-mode shaders and must not
carry an f32 backend it never enters. That is what keeps
`just fw-esp32c6-size-check` measuring an unchanged product image — check both
sides of the gate when you touch it. See
`docs/adr/2026-07-31-soft-float-via-compiler-builtins.md`.
