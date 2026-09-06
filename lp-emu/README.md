# `lp-emu/` — the emulation family

Every emulator LightPlayer owns lives here, and everything here is **MIT**
(`LICENSE-MIT`), while the rest of the repository is AGPL-3.0-or-later. That
is the point of the directory: it is a home *and* a licence fence, and
`just lint-emu-fence` is what keeps the fence real. See
`docs/adr/2026-09-06-lp-emu-home-and-mit-fence.md`.

## Layout

```text
lp-emu/
  LICENSE-MIT                   the licence for everything below
  lp-emu-core/                  arch-neutral host substrate
  lp-emu-abi/                   host <-> guest protocol
  lp-emu-validate/              the hardware-validation system (host)
  transcripts/                  committed, verbatim payload captures
  lp-riscv-emu/                 RV32IMAC+F executors
  lp-riscv-emu-guest/           rv32 guest-side runtime
  lp-riscv-emu-guest-test-app/  a guest binary the rv32 tests run
  lp-xt-emu/                    Xtensa LX6/LX7 executors, board maps, FP
  lp-xt-emu-guest/              Xtensa guest-side runtime (device-target)
  esp/                          SoC crates land here (see below)
```

**Crates are namespaced by vendor, not flattened** (vision D9). The
architecture cores sit at the root because an ISA outlives any one chip; a
SoC layer — machines, buses, peripherals, vendored ROM images — goes in a
vendor directory. `esp/` is the first, because ESP32 is the focus now, not
forever; a Raspberry Pi Zero would get `rpi/` beside it, and only the vendor
directories are allowed to assume MMIO at all.

## What each crate is

- **`lp-emu-core`** — host-side emulator machinery: guest memory
  (`Memory`/`MemoryError`), the run-loop result contract (`StepResult`,
  `TrapCode`), logging levels, cycle-cost accounting (`CycleModel`/
  `InstClass`), serial plumbing, time control, and the host-side profiler
  (`profile/`, behind the `std` feature). `no_std` + alloc.

- **`lp-emu-validate`** — the hardware-validation system's host half:
  payloads, configurations, transcripts, masking, provenance grading and
  replay, plus the runner behind `lp-cli validate`. It knows what a committed
  capture means and what each configuration is trusted for. It deliberately
  **mirrors** `lp-fw/fw-checks`'s payload registry rather than importing it —
  `fw-checks` is AGPL and outside this fence — and `lp-cli` owns the parity
  test that keeps the two from drifting. See its README.

- **`transcripts/`** — committed, verbatim payload captures, one directory per
  chip and payload, each `.txt` beside a `.txt.meta.json` sidecar carrying its
  provenance header. Not a crate; covered by `LICENSE-MIT` like everything
  else here. A local `.gitattributes` marks them `-text`, so the repository's
  `core.autocrlf = input` cannot edit a byte of them on the way in.
  **Never edit a transcript**: a mismatch is a regression or a re-capture,
  never a fixture to refresh.

- **`lp-emu-abi`** — the host↔guest protocol: syscall numbers, guest serial
  framing, the recovery handshake, and JIT symbol entries. Depended on by
  both the host emulators and the guest-side runtimes.

- **`lp-riscv-emu`** — the RV32 emulator: instruction executors, register
  file, run loops, `EmulatorError`, and the rv32 frame-pointer backtrace
  walk. `lp-riscv-inst` decodes for it; it never re-implements decoding.
  Since M3 it also carries the **machine-mode hart** (`mach::MachineHart`) —
  M-mode CSRs, traps, `mret`/`wfi`, hardware triggers and interrupt delivery
  — which is the piece a SoC machine under `esp/` drives.

- **`lp-riscv-emu-guest`** / **`lp-riscv-emu-guest-test-app`** — the
  guest-side runtime (entry, syscalls, allocator, panic, logging) for code
  running inside `lp-riscv-emu`, and a test binary built against it.
  `fw-emu` and `lps-builtins-emu-app` link the guest for its `memory.ld`.

- **`lp-xt-emu`** — the Xtensa emulator: windowed-register machinery,
  per-board memory maps (`BoardProfile::esp32s3()` / `esp32()`), the
  host-shared data window and full-argument call path that let a host engine
  run compiled shader code against a vmctx in host memory, and an FPU proven
  equal to real ESP32-S3 silicon behind an explicit policy layer. Its FP
  predictions and silicon captures live in `tests/fixtures/fp/` and are
  committed **before** any hardware run.

- **`lp-xt-emu-guest`** — the `no_std` Xtensa guest runtime. A DEVICE-target
  crate: excluded from the host workspace and built as a member of the
  `lp-xt/fixtures` esp-toolchain workspace.

**Arch-neutrality rule:** `lp-emu-core` and `lp-emu-abi` must not depend on
cranelift or on any `lp-riscv-*` / `lp-xt-*` crate. Architecture specifics
enter by injection — the profiler's `StackUnwinder` fn pointer and
`CpuCollector`'s `ram_start`, or the per-arch `trap_code_from_cranelift` that
lives in the arch emulator. See
`docs/adr/2026-07-28-emu-core-crate-family.md`.

## The fence

`scripts/check-emu-fence.sh`, run by `just lint-emu-fence` and by
`just check-lint` (so by CI's `Lint (x64)` job), asserts two things:

1. every package under `lp-emu/` declares exactly `license = "MIT"`;
2. none of them reaches a workspace-local crate outside `lp-emu/`,
   transitively, except the crates listed in the script's `ALLOWED_OUTSIDE`
   table — each with a one-line reason.

The walk uses **declared** dependencies, optional and dev ones included, not
the resolved graph: a dependency behind a cargo feature is still an import.

If a new dependency trips it, the first move is to delete the dependency.
Allowlisting is the fallback, and it means accepting that the MIT unit is not
self-contained on that edge.

## What lives elsewhere, and why

- **`lp-riscv/lp-riscv-inst`, `lp-riscv-elf`, `lp-xt/lp-xt-inst`,
  `lp-xt-elf`** — instruction models and ELF loaders. They are
  **compiler-backend** infrastructure, shared with `lpvm-native`'s codegen
  and `rt_emu`, not emulator infrastructure; they stay where the compiler
  can reach them (vision Q3). They are AGPL today, which is why the MIT unit
  is not yet externally self-contained. Whether they should flip too is
  open (director-log E1).

- **`lp-xt/lp-xt-fp-vectors`, `lp-xt-fp-harness`, `lps-builtins-xt-*`** —
  the FP conformance corpus, the on-silicon rig that runs it, and the Xtensa
  builtins image. Hardware-validation and compiler assets, not emulation.

- **`lp-fw/fw-emu`** — firmware that *runs inside* `lp-riscv-emu`. It is a
  product image, so it stays with the other firmware.

## Bench instruments — `scripts/emu/`

Some of what this family needs is a *desk*, not a host: a UART0 console that
comes out somewhere other than the link under test, and a way to name one board
among several identical ones. The scripts under `scripts/emu/` are those
instruments, and the discipline they keep is the same everywhere:

- `board-port.py` — MAC to `/dev/cu.usbmodem…`, from IOKit, opening nothing and
  probing nothing. Every ESP32-C6 and -S3 enumerates as `303a:1001`, so a port
  list cannot tell two apart; the USB serial number is the MAC and does.
  **Nothing here ever picks the first port.**
- `tty-capture.py` — read one port raw, changing no line state. `stty` asserts
  DTR on open, which on a native-USB Espressif port is espflash's reset
  sequence: a reader that used it would reboot the board it came to watch.
- `uart-bridge-flash.sh` — put the `uart-bridge` payload on **one named board**.
  It takes a MAC and refuses to guess, because flashing the bridge onto the
  board under test destroys the measurement in silence.
- `uart-bridge-wiring-check.sh` — prove the wires with **no change to the board
  under test**: open the bridge's port, reset the other board from its own port,
  and read what came through.
- `reset-and-capture.py` — reset a board from its own USB-Serial-JTAG handle and
  read its boot log on that same handle. This is the only way to see a
  native-USB board boot: `espflash monitor --before default-reset` drops the USB
  device and reopens a new session after the banner has gone, while a passive
  reader cannot make a board boot at all. USB-SJ keeps its session across a
  *chip* reset, so a handle that is already open catches everything.
- `flash-image.sh` — the desk discipline in one place, which the two
  `uart-bridge-*` scripts go through: one named board, foreground, under a pty,
  refuse if a port is held, SIGINT by pid, wait for something the **image**
  prints rather than for espflash's "completed".

⚠️ **`--no-stub`, and power-cycle rather than reset.** On this fixture espflash's
RAM stub cannot connect, and a board can enter a state where the second-stage
bootloader spins forever on `LP_I2C_ANA_MAST_I2C0_BUSY` — LP-domain state that
survives every reset short of power-on. Both are the same root cause and both
are written up in
`docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md`.

The payload itself is `fw-checks`' `uart-bridge` (see that crate's README); the
fixture and its current blocker are
`docs/defects/2026-09-06-c6-analog-master-wedges-the-bootloader.md`.

## Roadmap

`lp-emu-validate/` and the first two transcripts landed with M2 of the
2026-09-06 esp-emulator plan. The ESP32-C6 SoC emulator and the vendored ROM
images arrive under `esp/` from M3 on; nothing in `esp/` exists yet.

`lp-cli validate list` already names `lp-emu:esp32c6:t1`, and says
`unavailable until M3`. That is deliberate: the configuration exists as a name
and a seam (`lp_emu_validate::driver::ConfigurationDriver`), so M3 implements
one trait and nothing else in the validation system changes.
