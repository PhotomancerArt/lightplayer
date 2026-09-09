# esp-emu spike scripts

> **What has been promoted lives in `scripts/emu/`.** `build-reference-image.sh`
> (the pinned firmware tree the committed C6 transcripts came from),
> `pac-regnames.py` (the generated register-name tables) and the bench
> instruments (`board-port.py`, `tty-capture.py`, `flash-image.sh`, the
> `uart-bridge-*` pair, `reset-and-capture.py`) are there, with `just
> lint-emu-regnames` and `just test-emu-c6` over them. `mask-transcript.sh`'s
> rules are now code, in `lp-emu/lp-emu-validate/src/mask.rs`, where each
> carries its field class and the reason it is ignored.
>
> **`desk-espflash-step.sh` MOVED to `scripts/emu/desk-espflash-step.sh`**
> (M8's sweep, 2026-09-08). The validation runner (`lp-cli validate run
> --config silicon:…`) shells out to it rather than re-deriving the port
> discipline, so its foreground-`script(1)`, `SIG_DFL`-shim, SIGINT-by-pid,
> `lsof`-post-check behaviour is a product dependency, and a product
> dependency living under `spike/` was a wart the plan's closing sweep owed
> you. `lp_emu_validate::driver::DESK_STEP_SCRIPT` is the one place that
> spells the path.
>
> The committed silicon transcripts' `.meta.json` `source` lines still name
> the old path, and they stay that way: a sidecar records the command that
> was actually run, and editing one to keep a link alive would be editing a
> transcript. Read them as history and use the new path.
>
> **The rest of this directory stays where it is.** These are the spike's own
> record: `docs/reports/2026-09-07-esp-emu-c6-spike.md` §10 names these paths
> in its reproduce section, none of them is a dependency of anything that
> runs today, and moving them would cost that report its provenance and buy
> nothing.


Host-only tooling from the 2026-09-06 spike that ran `fw-esp32c6` under
Espressif's binary emulator (`esp-emu` 0.42.0). Report:
`docs/reports/2026-09-07-esp-emu-c6-spike.md`. The emulator scripts never
open a serial port; `run-desk-tcp-walk.sh` and `usb-tcp-bridge.py` are the
desk half (report §11) and do — one named
USB-Serial-JTAG port, one step at a time.

| script | what it does |
|---|---|
| `run-emu-tcp-walk.sh` | Boots a merged flash image under `esp-emu --uart-tcp`, puts the transcribing proxy in front of it, and drives `lp-cli … serial:tcp://…` against the proxy. Leaves `<label>.uart.bin` (every byte, host→device wrapped in `<<HOST … >>`), `<label>.uart.bin.times` (wall-clock per chunk), `<label>.cli.log`, and the emulator's stdout/stderr. Hard-kills the emulator `timeout+20 s` after `--timeout` (it has been seen not to exit at `RUST_LOG=debug`/`trace`). |
| `uart-tcp-proxy.py` | The transcribing TCP proxy: connects to the emulator's UART socket at once (so the boot banner is captured), listens for one client, replays the backlog to it, logs both directions. |
| `pty-tcp-bridge.py` | pty ↔ TCP bridge, kept for the record: **useless for lp-cli on macOS** — `serialport` sets the rate with `IOSSIOSPEED`, which a pty answers with `ENOTTY` ("Not a typewriter"). Works for readers that only `cat` the slave. |
| `mmio-scan.py` | Static MMIO scan of an rv32 disassembly (`rust-objdump -d --no-show-raw-insn`): `lui`+offset loads/stores into the C6 peripheral windows → JSON rows (peripheral, register address, R/W, sites, functions). Under-counts drivers that hold a base pointer across branches. |
| `usb-tcp-bridge.py` | Raw tty↔TCP bridge (`os.open` + `termios`, no pyserial, never toggles DTR/RTS — opening does not reboot a native-USB C6). Put `uart-tcp-proxy.py` in front of it and the desk walk leaves the same `.uart.bin` transcript the emulator walk does. |
| `run-desk-tcp-walk.sh` | Desk twin of `run-emu-tcp-walk.sh`: optional `--flash <merged.bin>` (`espflash write-bin 0x0`, blank lpfs like the emulator), bridge, proxy, `lp-cli … serial:tcp://…`, then holds the port `<hold-secs>` for heartbeats. |
| `mask-transcript.sh` | ANSI/CR strip + heap/time digit masking so two transcripts diff on content (§11.3). |
| `symbolize-pcs.py` | Maps `PC=0x…` samples from `RUST_LOG=debug` esp-emu output to symbols using `rust-nm -n -C --defined-only <elf>` output. |

Typical run (from the worktree root, image already merged with
`espflash save-image --merge`):

```bash
export SCRATCH=/path/to/scratch WORKTREE=$PWD
PORT=5561 RUST_LOG=info scripts/spike/esp-emu/run-emu-tcp-walk.sh merged.bin walk 120 \
    -- upload projects/test/basic 'serial:TCP' --wait-timeout 60
```

The firmware image must carry `--features spike_uart0_link` (fw-esp32c6): the
emulator has no USB host, so the host link has to be UART0.

Desk twin (report §11): flash the very image the emulator booted and walk it
through the same proxy —

```bash
export SCRATCH=/path/to/scratch WORKTREE=$PWD PORT_DEV=/dev/cu.usbmodem1433201
scripts/spike/esp-emu/run-desk-tcp-walk.sh meteor 40 --flash merged-default.bin \
    -- upload catalog/patterns/meteor 'serial:TCP' --wait-timeout 90
diff <(scripts/spike/esp-emu/mask-transcript.sh $SCRATCH/walks/det1.uart.bin) \
     <(scripts/spike/esp-emu/mask-transcript.sh $SCRATCH/desk/basic.uart.bin)
```
