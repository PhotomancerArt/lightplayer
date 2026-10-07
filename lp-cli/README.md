# lp-cli

Developer CLI: projects, dev server, board manifests, GPIO calibration, and
the commands that talk to a running firmware host (`upload`, `dev`, `serve`,
`fwcheck`, …).

## Host specifiers

Commands that connect to a firmware host (`lp-cli upload <project> <host>`,
`lp-cli dev <host>`, …) take a host specifier string
(`lpa_client::HostSpecifier`, parsed in `lp-app/lpa-client/src/specifier.rs`):

| specifier | connects to |
|---|---|
| `local` (or empty) | an in-memory server, no transport |
| `emu` / `emulator` | `fw-emu` running in `lp-riscv-emu` on a background thread |
| `serial:auto` | auto-detected serial port, default baud |
| `serial:/dev/ttyUSB1` | a specific serial port |
| `serial:/dev/cu.usbmodem2101?baud=115200` | a specific port and baud rate |
| `ws://host:port/`, `wss://host:port/` | a WebSocket host |
| `serial:tcp://host:port` | a device link over TCP instead of a real serial port |
| `serial:ws://host:port/path` | a device link over a WebSocket — `lp-cli emu serve`'s byte endpoint |
| `relay:<board-id>[@<origin>]` | a board through the cloud relay (origin `https://lightplayer.app` by default) — see below |

**`ws://…` and `serial:ws://…` are not the same thing.** A bare
`ws://host:port/` is the **lpc-wire protocol** against an `lpa-server`; the
`serial:` prefix makes it **raw device bytes**, the sibling of
`serial:tcp://`. The raw-byte spelling is what `lp-cli emu serve` hands out:

```sh
lp-cli emu serve --board c6-a=target/emu-ref/esp32c6+server+radio/fw-esp32c6 \
    --listen 127.0.0.1:5599 --state-dir target/emu-serve
lp-cli upload projects/test/basic serial:ws://127.0.0.1:5599/board/c6-a/bytes
```

Like `tcp://` it has no modem lines and so no reset-on-open; the board's
DTR/RTS, `attach`/`detach` and `reset` live on the **control** endpoint
(`/board/<id>/control`), which is a second WebSocket on purpose — in-band
control would be a dialect every byte client would have to speak. See
`lp-app/lpa-client/src/stream/ws_stream.rs` and
`lp-cli/src/commands/emu/serve/`.

`serial:tcp://host:port` is for hosts that expose their UART as a TCP
socket rather than a real serial device — Espressif's `esp-emu`
(`--uart-tcp`) and QEMU (`-serial tcp::PORT,server`) both do this, and it is
also the only way to reach a device emulator on macOS, where a pty cannot
stand in for a real port (`serialport` sets the baud rate through
`IOSSIOSPEED`, which a pty driver refuses with `ENOTTY`). It has no modem
lines, so there is no reset-on-open — the wire protocol's periodic hello
establishes the session instead. See
`lp-app/lpa-client/src/stream/tcp_stream.rs` and
`HostSerialEsp32Provider::connect`
(`lp-app/lpa-link/src/providers/host_serial_esp32/provider.rs`) for the
implementation, and
`docs/reports/2026-09-07-esp-emu-c6-spike.md` for where this came from.

## Boards through the cloud relay: `relay:` and `serve --relay`

A board on Wi-Fi with Cloud relay on dials lightplayer.app and stays there;
`relay:<board-id>` reaches it from anywhere — the same secure lp-link as a
`lan:` board, inside a WebSocket to `<origin>/relay/board/<id>`. The board id
is its MAC (`10bda3b08e30`; `10:BD:A3:…` reads too). The relay wants a
signed-in session, read from **`LP_CLOUD_SESSION`** (the value of the
`lp_session` cookie of a signed-in browser — environment only, never argv,
never printed). With an account session the account's key is tried first, so
a board the account plugged in by USB opens at its tier; anyone else gives the
board's password (`--password-stdin` or `LP_PASSWORD`), and with neither the
board refuses — through the relay a board never grants its "Anyone" tier.

```sh
LP_CLOUD_SESSION=… lp-cli upload projects/test/basic relay:10bda3b08e30
```

`lp-cli serve --relay <origin>` puts lp-cli's host board on the relay the way
a C6 does — the same `lpc-relay` client, one session at a time — for building
against before a board is on Wi-Fi. With `LP_CLOUD_SESSION` set it installs
the account's key in its own access store first (it prints only
`installed <name>'s account key`), then prints the `relay:` address to use:

```sh
just cloud-serve                                   # a local relay; note its URL
LP_CLOUD_SESSION=… lp-cli serve --memory --relay http://127.0.0.1:<port>
```

`lp-cli wifi status <device>` prints a board's relay state in the words
Studio uses (`relay: connected to lightplayer.app`, `relay: no account key — …`,
`relay: connected, no internet — …`), and `wifi set --cloud-relay on|off`
flips the board's switch. With no board on Wi-Fi, an emulated C6 reaches a
local relay through a virtual LAN's uplink — an `[[uplink]]` table in the
`emu run --lan` fixture (`name = "lightplayer.app"`, `to = "127.0.0.1:<port>"`) —
which is what `just walk-wifi-emu relay` does
(`lp-cli/tests/emu_relay_link.rs`).

The pieces: `lp-cli/src/server/relay_host/` and
`lp-app/lpa-client/src/transport_relay/`; the end-to-end test is
`lp-cli/tests/relay_link.rs`; the decision is
`docs/adr/2026-10-06-cloud-relay.md`.

## Measuring a board's link: `lp-cli link rtt`

How promptly a rendering board answers: request round trips, the link's own
round trips (send → ACK, from lp-link's estimator), transfer rates both ways
and the idle frame rate, through warm-up → idle → transfers → requests →
tail. It runs on a serial device in wall-clock time, or on the emulated C6 in
this process in emulated time (`emu:<fw-esp32c6 ELF>`, which deploys a
project first — the PLAYFUL choker by default — and adds the WS281x frames
decoded off the pads to the report).

```sh
lp-cli link rtt /dev/cu.usbmodem2101 --transfers-at-s 40 --requests-at-s 63 --json si.json
lp-cli link rtt emu:target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6 \
    --requests-at-s 40 --json emu.json --console emu.console.txt
```

A project's frame cost varies with its pattern time and every transfer and
request waits on the frame in flight, so two builds are only comparable when
their phases start at the same **board time** (`--transfers-at-s`,
`--requests-at-s`): emulated time since power-on on `emu:`, the board's own
uptime from its heartbeats on a serial device. Compare emulated runs in
frames, never against silicon in milliseconds. `lp-cli link rtt --help` has
every knob; `lp-cli/src/commands/link/rtt.rs` the phases.
