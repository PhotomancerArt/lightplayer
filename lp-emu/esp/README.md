# `lp-emu/esp/` — the Espressif SoC layer

This directory is the vendor namespace decided in vision D9: architecture
cores live at the root of `lp-emu/`, and anything that assumes a *chip* — a
memory map, MMIO decode, peripherals, a ROM image — lives under the vendor it
belongs to.

Today it holds one machine, and that machine boots the shipped firmware to its
idle loop:

```bash
just emu-c6 target/emu-ref/d6cfaa205-boot-idle-memfs/fw-esp32c6 --timeout 6s --strict-bus
```

## The layering

Four layers, and each one knows strictly less than the one above it. That is
what lets a second chip reuse three of them.

```text
lp-cli validate            payloads, transcripts, replay, trust grading
  lp-emu-esp32c6           THE CHIP: memory map, ROM image, reset state,
                           peripherals with real behaviour, the run loop, a CLI
    lp-emu-esp-common      bus, MMIO decode, Peripheral + BusCx, RegFile,
                           trace + spin detector, host byte streams, ELF view
                             — no chip numbers, none
      lp-riscv-emu         the hart: RV32IMAC executors and, since M3,
                           machine-mode CSRs, traps, mret/wfi, triggers
        lp-emu-core        guest memory, the scheduler, CycleModel, StepResult
```

- **`lp-emu-esp-common/`** — the SoC substrate every ESP machine shares: the
  bus (`SocBus`), the MMIO decode table, the `Peripheral` trait and its
  `BusCx`, `RegFile` (accept-and-remember with a table of exceptions), the
  bus trace with its spin detector, host byte streams, the interrupt-matrix
  seam and the machine-request slot, and the PT_LOAD view of an ELF. It holds
  **no chip numbers** — see its README.

- **`lp-emu-esp32c6/`** — the C6 machine: the memory map (every base cited to
  esp-hal's linker script), the mask-ROM loader and its deliberately empty
  hook table, direct load with the state the bootloader leaves behind, the
  peripheral set the boot path touches, the discrete-event run loop, snapshot,
  and a CLI whose every timeout is emulated time. The generated register-name
  tables in `src/regs/` come from `scripts/emu/pac-regnames.py` and carry
  their provenance headers. This is where the chip numbers live — see its
  README.

- **`roms/`** — the vendored ROM ELFs (Apache-2.0, from `esp-rom-elfs`
  release `20260528`), with their LICENSE and checksums (vision D6). The ROM
  is loaded in **every** configuration, because the application calls into it
  at runtime whatever booted it (plan PD7): `rtc_get_reset_reason` before
  `.bss` is zeroed, `ets_delay_us` from every clock path, `memcpy` and the
  `str*` family because the linker resolves them there.

## How to run it

Three doors, in order of ceremony.

```bash
# 1. one image, whatever flags you are debugging with
just emu-c6 <elf> --strict-bus --trace UART0,TIMG0 --timeout 200ms

# 2. the machine's own gates (needs firmware; builds the reference images)
just test-emu-c6

# 3. a recorded, replayable run — the one that produces a transcript
cargo run -p lp-cli -- validate run emu-m3 --config lp-emu:esp32c6:t1 --dry-run
```

Door 3 is the one that makes a claim. `lp-emu:esp32c6:t1` and `:t2` are
configurations in `lp-emu/lp-emu-validate/validate.toml`, the runner's driver
turns a payload into exactly the command line door 1 takes, and
`validate record` writes the capture under `lp-emu/transcripts/` with a
sidecar. Nothing about a run is decided anywhere else: the time grade, the
eFuse identity, the strict bus, the sentinel and the emulated timeout are all
on that command line, which is why `--dry-run` printing it is the whole
protocol.

## What it is trusted for

Every field class of `lp-emu:esp32c6:*` is graded **`modeled`** in M3, each
with its reason in `validate.toml`. That is not modesty and it is not a
placeholder:

| class | why it is `modeled` |
|---|---|
| memory | byte-equal to silicon on the compile harness — 372/372 values — which is *evidence in the reason*, not a promotion. `measured` would want a transcript per class |
| timing | `t1` counts instructions, `t2` uses the per-class model, and no transcript grades either yet (the vision's graded ladder) |
| boot-log | a direct load prints no ROM banner and no bootloader lines at all; M7 boots from reset |
| usb-serial-jtag | the host-absent state and only that; M6 brings the attached and not-draining ones |
| pin | nothing is observed; M5 brings the RMT channels and the WS281x decoder |
| wire | the bytes are the guest's; a live socket's arrival times are the host's |

The rule behind the table is the vision's: **never trust an emulated number
outside what a transcript proves.** The spike found esp-emu byte-equal on
memory and 2.4x wrong on time in the same run, and the compile harness's own
5 ms slice budget passing there while failing on the board.

## Everything here is MIT

Inside the fence — see `../README.md` and `just lint-emu-fence`. An `esp/`
crate may not reach a product crate, `fw-checks` included: the validation
system's payload registry deliberately **mirrors** `fw-checks` rather than
importing it, and `lp-cli` owns the parity test that keeps the two equal.
