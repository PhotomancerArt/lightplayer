---
status: fixed
found: 2026-10-14      # how: hardware-walk (RAM research V3, CX1) disagreed with `lp-cli emu run --merged` on the same image
fixed: this change
area: lp-emu/esp/lp-emu-esp-common bus.rs SocBus::unmapped × lp-emu/esp/lp-emu-esp32c6 machine.rs (the C6's bus)
class: fidelity
related:
  - docs/defects/2026-10-14-the-c6-loader-copies-an-address-zero-padding-segment-to-address-zero.md
  - lp2025/2026-10-09-1203-ram-research   # experiment l1
---
# The emulated C6 ignored stores to address 0, so a core that faults the loader on silicon booted in the emulator

**Symptom.** #1110 merged with main made a C6 core image with an address-0
padding segment. On CX1 the split loader faulted on every boot:

```
Guru Meditation Error: Core 0 panic'ed (Store/AMO access fault)
PC      : 0x408500ca ... MCAUSE : 0x00000007
```

`lp-cli emu run --merged` on the same image logged `UNMAPPED write1 at
0x00000000 from pc=0x408500ca` 26,452 times, ignored each one, and booted
the core (`[LOADER] core @0x18000 (proven)`). The PC is the one that faulted
on silicon.

**Root cause.** `SocBus::unmapped` answered every unmapped access the same
way outside `--strict-bus`: count it, log it once per site, return a zero
or drop the store. That is the right answer for an *unmodelled block*
inside a declared MMIO window, where silicon has a peripheral this model
lacks. It is the wrong answer where the chip has nothing at all: there the
bus raises an access fault. The C6 had no way to tell the two apart, so a
store to address 0 succeeded.

**Fix.** `SocBus::set_fault_stores_to_nothing`: a store to an address in no
region and no declared MMIO window takes the architectural store access
fault, strict or not. The C6's bus turns it on (`machine.rs`, beside its
MMIO windows). Loads, and stores inside a window, keep the permissive answer
unless strict, because emulated firmware still reads through incomplete
stubs (the Wi-Fi blob's `R4 UNMAPPED+0x1143c`). The S3 and the classic are
unchanged.

**Regression coverage.**
`lp-emu-esp-common`: `a_store_to_nothing_faults_when_asked_and_nothing_else_changes`.
`lp-emu-esp32c6/tests/split_boot.rs`:
`a_core_segment_at_address_0_faults_the_loader_as_on_silicon` re-aims the
shipped core's first RAM segment at address 0 and boots ROM-up: the ROM now
prints the board's own `Store/AMO access fault`, PC `0x408500ca`, MCAUSE 7,
and the core never starts. In `just test-emu-c6` (CI's C6 emulator job).

**Lesson.** An emulator that is permissive by default turns "the guest wrote
where nothing is" into a passing run. Nothing being there is a fact the
memory map already knows, so it should fault the way silicon does. Only an
unmodelled peripheral calls for the permissive answer.
