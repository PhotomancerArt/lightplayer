# esp-emu spike scripts

Host-only tooling from the 2026-09-06 spike that ran `fw-esp32c6` under
Espressif's binary emulator (`esp-emu` 0.42.0). Report:
`docs/reports/2026-09-07-esp-emu-c6-spike.md`. Nothing here opens a serial
port or touches a board.

| script | what it does |
|---|---|
| `run-emu-tcp-walk.sh` | Boots a merged flash image under `esp-emu --uart-tcp`, puts the transcribing proxy in front of it, and drives `lp-cli … serial:tcp://…` against the proxy. Leaves `<label>.uart.bin` (every byte, host→device wrapped in `<<HOST … >>`), `<label>.uart.bin.times` (wall-clock per chunk), `<label>.cli.log`, and the emulator's stdout/stderr. Hard-kills the emulator `timeout+20 s` after `--timeout` (it has been seen not to exit at `RUST_LOG=debug`/`trace`). |
| `uart-tcp-proxy.py` | The transcribing TCP proxy: connects to the emulator's UART socket at once (so the boot banner is captured), listens for one client, replays the backlog to it, logs both directions. |
| `pty-tcp-bridge.py` | pty ↔ TCP bridge, kept for the record: **useless for lp-cli on macOS** — `serialport` sets the rate with `IOSSIOSPEED`, which a pty answers with `ENOTTY` ("Not a typewriter"). Works for readers that only `cat` the slave. |
| `mmio-scan.py` | Static MMIO scan of an rv32 disassembly (`rust-objdump -d --no-show-raw-insn`): `lui`+offset loads/stores into the C6 peripheral windows → JSON rows (peripheral, register address, R/W, sites, functions). Under-counts drivers that hold a base pointer across branches. |
| `symbolize-pcs.py` | Maps `PC=0x…` samples from `RUST_LOG=debug` esp-emu output to symbols using `rust-nm -n -C --defined-only <elf>` output. |

Typical run (from the worktree root, image already merged with
`espflash save-image --merge`):

```bash
export SCRATCH=/path/to/scratch WORKTREE=$PWD
PORT=5561 RUST_LOG=info scripts/spike/esp-emu/run-emu-tcp-walk.sh merged.bin walk 120 \
    -- upload examples/basic 'serial:TCP' --wait-timeout 60
```

The firmware image must carry `--features spike_uart0_link` (fw-esp32c6): the
emulator has no USB host, so the host link has to be UART0.
