# fw-esp32-common

Chip-generic firmware layer shared by the per-SOC ESP32 firmware crates —
`fw-esp32c6` today, a future Xtensa `fw-esp32s3` next. Extracted so the second
chip crate reuses the firmware substance instead of forking ~9k LOC.

## Seam rules

Per `docs/adr/2026-07-29-per-chip-fw-toolchains.md`, this crate must build under
BOTH the pinned workspace nightly and the Espressif Xtensa rustc fork:

- No `esp-hal` / `esp-alloc` / `esp-println` / `esp-storage` / `esp-rtos` /
  `esp-radio` dependencies.
- No `unwinding` dependency and no panic-strategy assumptions. Every chip is
  abort tier now (ADR `2026-08-02-rv32-firmwares-are-abort-tier`) — the C6 was
  the last one unwinding — but each chip crate still owns its own posture and
  panic path.
- No `rust-toolchain.toml` and no `.cargo/config.toml` here.

Chip facts arrive by injection instead:

- `server_loop::run_server_loop` takes a `memory_stats` fn (heap free/used) and
  a `feed_watchdog` closure.
- `uart_link` (the classic's UART on lp-link) takes its bytes from the chip
  crate's I/O task through two static pipes (`uart_link::uart_link_pipes`);
  `usb_link` (the C6/S3) takes the chip's USB halves and register facts.
- `hardware::manifest_loader::load_hardware_manifest` takes the compiled-in
  fallback manifest fn.
- `lp_fs::LpFsFlash` is generic over `littlefs_rust::Storage`; the flash
  adapter and partition-derived `Config` stay chip-side.
- `logger` stores injected write fns / a type-erased `dyn SerialIo` handle.
- `split_core` (feature `split-core`) is generic over one `SplitChip` the
  chip implements (below).

## Contents

`boot`, `server_loop`, `transport`, `time`, `logger`, `jit_fns` (the JIT
host-log symbol), `lp_fs` (littlefs-backed `LpFs`), `hardware::manifest_loader`,
`output::provider` (trait-driven output provider), `output::power_gate`
(switched-power-rail state machine behind the manifest's `power_gate`
descriptors — the chip crate supplies the pin and the clock; see
`docs/adr/2026-08-08-switched-power-rail-mechanism.md`), `serial::shared_serial`.

`output::rmt_state` used to sit beside the provider; it went away with its only
consumer when `fw-esp32c6` moved onto `lp-ws281x` (2026-08-01). Per-channel RMT
state is `lp_ws281x::ChannelState` now, shared by all three chips.

## The split image's update board side (`split_core`)

Feature `split-core` (the C6's `server` turns it on; its `LP_SPLIT_LINK`
builds call it, a plain build links none of it). What a split image's core
does about updates, on any chip: the boot records' marks (`begin`,
`mark_started`), the boot state it trusts, the fenced raw flash
(`SplitFlash`), the engine choice and its guard (`choose_engine`,
`engine_guard`, the `[CORE]` line), the core's own hash, the running engine's
channel-3 hooks (`install_running_hook`, `running_manifest`, `hello_ready`,
`radio_update_hook`), core-only (`core_only`), and the update session's edge
and `UpdateTarget`. It moved from `fw-esp32c6/src/ota/` unchanged in
behaviour (S3 plan M7 P7), so the S3 runs the same session.

Everything that names a chip is one trait, `SplitChip`: the flash MMU (page
size, the engine's mapping and data-view address, a scratch window, MMU
entry 0), the raw flash calls the fence wraps, the SHA, the reset and its
classification, the layout code and region start, the watchdog, the update
light, and `statics()` — the chip's one `SplitStatics` (Rust has no generic
statics). Associated functions and types only, never `self`: an image has one
chip, so each generic is instantiated once and every call stays direct under
LTO. The C6's is `fw-esp32c6/src/split_core/` (`C6Split`), the S3's
`fw-esp32s3/src/split_core/` (`S3Split`, layout 2, M7 P8); `test_chip.rs` is
a host one for the fence and boot-state tests. `SplitChip::updates_off` (a
default `None`) is how a part that cannot take an update says so: the S3 on
octal flash announces layout 0 and its boot state is never trusted, so
nothing is written. `status_light_note` is the engine's half of the update
light, shared: the chip's RMT driver notes the first strip it opens, the
frame hook records it.

The silicon rules (the split image ADR §7) are kept as code here: no second
`FlashStorage`, the write fence, trust the records only when they agree with
MMU entry 0. The chip calls `begin` right after its `FlashStorage::new`,
before any radio.

The heavy server stack (`lpa-server`, `lpc-model`, `lpc-wire`, `lpfs`,
`lp-recovery`) is behind the `server` feature, mirroring the bin crates.

Deliberately NOT here: RMT register code (`fw-*/src/output/rmt/`) — each chip's
`RmtHw` impl is chip constants that must stay const-folded in the interrupt hot
path, while the refill sequencing above it is `lp-fw/lp-ws281x`; app orchestration
(`boot_firmware`/`FirmwareApp`) — deferred until a second chip crate exists to
shape the abstraction.

## Validation

```bash
cargo check -p fw-esp32-common
cargo check -p fw-esp32-common --features server
cargo clippy -p fw-esp32-common --features server -- --no-deps -D warnings
```

The crate is a workspace default-member, so `just check` covers it on the host
toolchain; the firmware builds exercise it under `riscv32imac-unknown-none-elf`.

## Network links (feature `wifi`): the LAN, the relay and the network slot

`radio_link/` holds the one secure network session as a **network slot**
(`NETWORK_LINK_SLOTS = 1`) shared by the C6's LAN endpoint and the cloud
relay: each link records its trust (`Keyed` on the LAN, `Relayed` through the
relay) and the edge serving it (`SlotEdge`). A newcomer that finds the slot
held parks its first frame (`parked_handshake.rs`) and the mux either turns
it away busy or, for a handshake that proves the holder's own key, lets it
take the slot. `net/relay/` is the relay's board half, sans-IO: `RelayDriver`
joins `lpc-relay`'s `RelayClient` to the slot, and `run_relay_leg` is the
device leg's loop over a platform's sockets (`RelayLegIo`: DNS, TCP, clock,
RNG), shared by `fw-esp32c6`'s relay task and the host harness
(`net/host_lan_harness`, feature `host-lan-harness`, which runs the board's
real port and server over std sockets; its relay tests are `relay_slot_tests`).
`net/ws/` has both halves of the WebSocket (server for the LAN, client for the
relay) over `ByteStream`, the seam a TLS wrapper would slot into. The relay
leg allocates its buffers only while the board may dial (`try_zeroed_bytes`:
no room is a failed dial, never a reset). Relay protocol 2: the board sends
its picture and its project's name to the relay — the main thread makes
them (`serve_relay`, engine-side) and hands them to the leg through
`RelayBoard` (`RelayPictureSlot`, one buffer going back and forth); the
heartbeat says `pictures N idle|watched|off` (`relay_picture_tests`).
Decisions: `docs/adr/2026-10-06-cloud-relay.md` ("Device side").
