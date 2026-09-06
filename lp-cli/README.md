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
