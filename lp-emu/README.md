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
  esp/                          Espressif SoC layer (see esp/README.md)
    lp-emu-esp-common/          bus, MMIO decode, peripherals, trace
    lp-emu-esp32c6/             the C6 machine: map, ROM, run loop, CLI
    roms/                       vendored mask-ROM ELFs (Apache-2.0)
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

- **`esp/lp-emu-esp-common`** — the Espressif SoC substrate: `SocBus` (RAM
  regions, MMIO decode, watchpoints, the unmapped policy), the `Peripheral`
  trait and its `BusCx`, `RegFile` for the accept-and-remember blocks, the
  bus trace with its spin detector, host byte streams, and an ELF
  program-header view. It contains **no chip numbers**; a chip crate
  registers its own regions and peripherals. Generated register-name tables
  (`scripts/emu/pac-regnames.py`, gated by `just lint-emu-regnames`) live in
  the chip crate for the same reason. See its README.

- **`esp/lp-emu-esp32c6`** — the ESP32-C6 machine, and the only place in the
  family that holds chip numbers: the memory map (every base cited to
  esp-hal's linker script), the mask-ROM loader and its deliberately empty
  hook table, direct load with the bootloader's memory state, the PD5 run
  loop, snapshot, and a CLI whose timeouts are all emulated time. Ships with
  no peripherals: the phase gate is the first access nothing claims. See its
  README.

- **`esp/roms/`** — the vendored Espressif mask-ROM images, committed verbatim
  with their Apache-2.0 licence and checksums. Not a crate. **Never edit an
  ELF, and never edit `SHA256SUMS` to make a check pass** — re-run
  `scripts/emu/fetch-rom-elfs.sh`, which re-derives both from the published
  tarball.

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

## Speed

The interpreter's throughput is a product concern, not a curiosity: the
emulator is on its way to being a *device* in Studio, and a machine that runs
at a fifth of real time cannot stand in for a board someone is watching.

**The opt-level rule.** The workspace `[profile.release]` is `opt-level = "z"`,
chosen for firmware flash, and a size-optimizing pass is exactly what an
interpreter loop cannot afford — it cost 2.3x here. The root `Cargo.toml`
therefore names the five host-side emulator crates in
`[profile.release.package.*]` at `opt-level = 3`: `lp-emu-core`,
`lp-riscv-emu`, `lp-emu-esp-common`, `lp-emu-esp32c6`, `lp-xt-emu`.

It names crates rather than flipping a profile because `lp-riscv-inst`,
`lp-xt-inst` and `lp-emu-abi` are in firmware graphs and `release-esp32`
inherits `release` — an override reaching those would grow flash. **Anything
added under `lp-emu/` that a firmware links must stay off that list**;
`cargo tree -p fw-esp32c6 --target riscv32imac-unknown-none-elf` is the check.

**The probe.**

```bash
just bench-emu-c6                     # both reference images, both grades
just bench-emu-c6 --json out.json
scripts/emu/bench-c6.sh --bin <saved-binary> --no-build --no-promote
```

It reports user seconds, instructions/second, two real-time ratios and the
load average, and `cmp`s the UART0 bytes against the previous run. It is an
**oracle, not a gate** — nothing in CI runs it, and no number it prints gates
anything (see "never gate on emulated microseconds", above and in AGENTS.md).
Read the user-seconds column: this desk is often at load 150+ with other
agents building, and a wall-clock ratio measured there is not a result. A
before/after belongs in a PR body as a same-window A/B with the load quoted.

**The measured ladder** (compile-stress harness, `t1`, M2 Max):

| build | instr/s | vs stock |
|---|---:|---:|
| stock release (`opt-level = "z"`) | 33.6 M | 1.00x |
| + opt-level 3 on the five crates | 76.9 M | 2.29x |
| + the bookkeeping pass (both shipped) | 103.5 M | 3.08x |
| + PGO (a recipe, never a default) | 149 M | 4.45x |

Those are quiet-machine figures. The same-window A/B that landed the two
shipped rungs, on a desk at load ~190, read 25.5 M -> 75.8 M (2.97x) with the
`stopped after` line, the UART bytes and a 20 ms `--trace` byte-identical
either side — which is the bar every rung of this work is held to (PD5, ADR
2026-09-06: a run is a pure function of the instruction stream).

**M2's MMIO fast path** (last-hit cache in `mmio_index`, a two-entry
data-region cache, and a precomputed range compare for the single store
watchpoint esp-hal keeps on the stack guard) landed on top: a same-window A/B
against M1's binary, desk load ~65-83 both sides, read the harness (the
longer-running, primary benchmark) up 6.5-11.8% (83.3 M -> 88.7 M at `t2`,
85.6 M -> 95.7 M at `t1`); `boot-idle-memfs` — a 0.5 s run, too short for the
load noise at this desk to average out — moved within +-8% either side of
even. Identity: `stopped after`, UART and a 20 ms `--trace` byte-identical to
M1's binary on both images, both grades (trace md5 `44015adf...`, unchanged
from M1). The ABI-slimming item (`Box` the register dump in `EmulatorError`,
`Option<Box<InstLog>>` on `ExecutionResult`) was implemented and measured
against the MMIO-only build: the harness read 0 to -5.9% (a same-load-window
regression, not the plan's required >=3% gain), so it was reverted rather
than kept — smaller `Result<T, E>` types did not translate into a faster
interpreter loop here, at least not enough to clear load noise.

**M4's pure poll-loop skip** is the largest rung so far, and the first that
is a semantic change rather than a codegen one — it has an ADR
(`docs/adr/2026-09-08-emulator-poll-loop-skip.md`) and a flag,
`--no-poll-skip`, that turns it off. A boot spends most of its cycles
spinning on `UART0+0x01c` while the console drains at baud; the hart
recognises such a loop as a fixed point of machine state except for time and
credits whole iterations of it to both counters. Same-window interleaved A/B
against `origin/main`, minimum USER seconds, this desk at load 60-110:

| image | grade | `main` | M4 | |
|---|---|---:|---:|---:|
| harness | t1 | 4.79 s | 0.80 s | **5.99x** |
| harness | t2 | 2.83 s | 0.57 s | **4.96x** |
| jit-math-perf | t1 | 1.99 s | 0.71 s | **2.80x** |
| jit-math-perf | t2 | 1.43 s | 0.60 s | **2.38x** |
| boot-idle-memfs | t1 | 0.73 s | 0.90 s | **0.81x** |
| boot-idle-memfs | t2 | 0.78 s | 0.88 s | **0.89x** |

The last two rows are the trade, recorded rather than hidden:
`boot-idle-memfs` is a `wfi`-idle image where the skip fires zero times, and
it pays 11-19% for the per-instruction bookkeeping that lets the machine
notice a poll loop at all. `jit-math-perf` — JIT'd Q32 kernels, the shape of
the product's render loop — gains 2.4x, so the loss is not a property of
"images the skip does not help" in general. The ADR's Consequences section
records what was tried against it.

Identity, at `boot_idle.rs`'s real 5.5 s deadline on all three images and
both grades: 25/25 artefacts byte-identical both with-vs-without the skip and
against `origin/main`.

**PGO** is `just bench-emu-c6-pgo` / `scripts/emu/pgo-c6.sh`: an instrumented
build, one training run of each pinned reference image, a merge, an
optimized rebuild, then the probe on the result — each phase in its own
`target/emu-pgo/*` dir so the RUSTFLAGS involved never invalidate the plain
release build. Opt-in (D4): never a default build or CI step, and the target
is met without it. Needs `rustup component add llvm-tools-preview`; the
script checks for it first.

The merge step calls `llvm-profdata` directly (resolved via `rustc --print
sysroot`) rather than the plan's `cargo profdata -- merge`: `cargo-binutils`
0.4.0 panics on any invocation on this toolchain (`cargo profdata -- merge
--help` alone crashes inside its own clap parsing), reproduced fresh after
reinstalling it — a real defect in that release, not an environment quirk.
`llvm-profdata` is the exact binary `cargo profdata` shells out to, so the
merge is unchanged; only the broken wrapper in front of it is gone.

**Measured 2026-09-07**, same-window A/B against the plain (non-PGO) release
build, desk load ~13-15 both sides:

| image | grade | non-PGO instr/s | PGO instr/s | speedup |
|---|---|---:|---:|---:|
| harness | t1 | 109.0 M | 163.1 M | **1.50x** |
| harness | t2 | 103.2 M | 161.1 M | **1.56x** |
| boot-idle-memfs | t1 | 81.6 M | 138.3 M | **1.69x** |
| boot-idle-memfs | t2 | 79.9 M | 134.4 M | **1.68x** |

`stopped after`, UART0 and the identity oracles are unchanged from the
non-PGO build (PGO changes codegen, never behaviour). On top of M1 and M2's
MMIO fast path, the harness now reads 163 M instr/s at `t1` on this desk —
call it the toolchain-bound end of this milestone's ladder; the overnight
research's opt-3-plus-patch-tree figure (103.5 M -> 149 M, ≈1.45x) is the
pre-M2 baseline this rung was designed against.

**The browser/phone rig.** The same binary, unmodified, builds for
`wasm32-wasip1` and runs in any browser under a small JavaScript WASI
preview1 shim (D6: no `wasm32-unknown-unknown` entry point, zero source
changes — the module is the CLI). `just bench-emu-web` builds it, stages it
beside the two reference images plus a page and a dedicated-Worker runner,
and serves it on the LAN (port via `scripts/dev-port.sh`, never pinned) so a
phone can open it, run the sequence in the Worker, and upload its result
JSON back to the Mac:

```bash
just bench-emu-web              # build, stage, serve — open the printed URL
just bench-emu-web --collect    # print every uploaded result-*.json as a table
```

The page shows a live table (wall seconds, instr/s, real-time ratio per run)
and a running best-of-t2 headline per image; `--collect` reads every
`result-*.json` in the stage directory and prints device, engine guess,
image, grade, wall seconds, instr/s and the real-time ratio. Baseline numbers
and the method are in
`docs/reports/2026-09-07-emu-web-bench-baseline.md`.

**The Xtensa core** (`lp-xt-emu`) has its own probe and its own ladder:

```bash
just bench-emu-xt                     # one `bench_loop` run, >=100 M instructions
scripts/emu/bench-xt.sh --bin <saved-binary> --no-build --no-promote
```

It reports instructions/second only — that core is an ISA core with no SoC
around it, so there is no emulated clock and no real-time ratio — and `cmp`s
the guest output *and* a capped text trace against the previous run. M6 took
it from 21.4 to 31.2 M instr/s on the recursion-heavy `ackermann` fixture and
54.5 to 61.3 M on `fib_rec`, with both captures byte-identical; the win is
almost entirely one memory resolution per access instead of four or five.
(Those two fixtures were the workload while the probe reached 100 M by
repeating a short program; it now runs the trip-counted `bench_loop` once
instead.) `lp-emu/lp-xt-emu/README.md` has the rung-by-rung table and the
generic-codegen trap that per-package `opt-level` overrides hide.

Evidence, and the rungs not yet climbed (poll-loop skip, block cache): the
planning workspace's
`2026-09-06-1001-esp-emulator/2026-09-07-speed-ladder-research.md` and its
`speed-research/` directory, executed by the `2026-09-07-0827-emu-speed-ladder`
plan.

## Roadmap

`lp-emu-validate/` and the first two transcripts landed with M2 of the
2026-09-06 esp-emulator plan. `esp/lp-emu-esp-common` landed with M3 P3,
alongside `lp-emu-core`'s discrete-event `Scheduler`; `esp/lp-emu-esp32c6` and
the vendored C6 ROM with M3 P4; the boot peripheral set with P5; UART0, the
host-absent USB-Serial-JTAG and the radio window with P6, which is where the
shipped image first said hello.

**Plan one is closed by M8.** `lp-emu:esp32c6:t1` and `:t2` are runnable
configurations, and every claim is a committed transcript under
`transcripts/esp32c6/` that `cargo test` replays. Milestone by milestone: M3
brought the machine up to the shipped image's hello; M4 gave it SPI1 flash and
the MMU windows, and with them a project upload transcript-identical to
silicon's; M5 gave it RMT and a WS281x decoder, so a frame can be read back off
a pad; M6 made the USB-Serial-JTAG honest, so the shipped image serves over the
link it ships with; M7 booted it from the reset vector through the real mask
ROM and the ESP-IDF bootloader; M8 put the hardware walk on it.

Everything the machine claims is graded `modeled`, with byte-equality recorded
as evidence in the reason rather than as a promotion. Promoting a class to
`measured` needs an instrument, not another agreement between two of our own
models.

```bash
cargo run -p lp-cli -- validate list          # the configurations
lp-cli emu run --merged <chip.bin> --link 127.0.0.1:5591 --monitor   # a C6 to talk to
just walk-esp32c6-emu                         # THE WALK — the hardware walk's twin
just test-emu-c6                              # the machine's gates + the replays
just heap-budget-check-chips                  # the firmware's own heap ledger
just emu-c6 <elf> --strict-bus --timeout 6s   # the workshop binary
```

`just test-emu-c6` runs in CI as the path-gated `Emulator C6 (x64)` job
(`.github/workflows/pre-merge.yml`), gated on changes under `lp-emu/**`,
`lp-fw/fw-esp32c6/**` or `scripts/emu/**` (see the `emu_c6` filter in that
workflow's `detect-changes` job). The walk is not in it: it builds a firmware
image, a merged flash image and a release `lp-cli`, then runs the machine for
eight emulated seconds. What it proves per-tick is
`esp/lp-emu-esp32c6/tests/shader_oracle_pin.rs`, which does run there.

What the emulator does **not** cover — the Chromium USB stack, radio traffic,
anything analog, RX pins, and any claim about wall-clock time — is listed in
`docs/reports/2026-09-08-esp32c6-emulator-walk.md`. Read that before quoting a
number from here.

Next: plan two (the browser shim, vision D16) and plan three (the classic
ESP32). The interfaces both need are already in place — the machine takes N
harts, the signal fabric is arch-neutral, and the host streams are byte
sources with a control channel.
