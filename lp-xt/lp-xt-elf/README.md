# lp-xt-elf

Loads **linked** Xtensa ELF32 executables into `lp-xt-emu` memory and hosts the
guest syscall ABI (print / exit / panic) used by the fixture corpus. The
optional `reloc` feature adds the M6 stretch prototype: relocatable-`.o`
linking with `R_XTENSA_32` / `R_XTENSA_SLOT0_OP` application.

## What it does

- `XtensaElf::parse(bytes)` — validates ELF32, little-endian,
  `e_machine == EM_XTENSA` (94), object kind *Executable*, and **rejects any
  file with REL/RELA relocation sections** (linked executables are
  pre-resolved; relocation processing is deliberately out of scope until M6).
- `XtensaElf::load_into(&mut Emulator)` — copies each `PT_LOAD` segment to its
  `p_vaddr` (zero-filling the `p_memsz` tail for `.bss`), returning a clear
  error if a segment falls outside the emulator's modeled memory.
- `XtensaElf::entry()` / `XtensaElf::symbol(name)` — entry point + symbol
  lookup for test harnesses.
- `run_elf(bytes, arg)` — one-call harness: parse, load into a fresh
  `Emulator`, run from the ELF entry via the synthesized windowed CALL8
  (`Emulator::run_loaded`), with `GuestHost` handling syscalls. Returns a
  `GuestRun` (outcome, collected output, exit code, panic message).

## Guest syscall ABI

Defined in [`src/abi.rs`](src/abi.rs): the guest executes the `SYSCALL`
instruction with the syscall number in `a2` and arguments in `a3..a5`; the
host writes the result into `a2` and resumes (or terminates the run for
`SYS_EXIT` / `SYS_PANIC`). The guest-side mirror is
`lp-xt/lp-xt-emu-guest`; the two constants files must stay in sync.

Address expectations for fixtures (see `fixtures/link.ld`): `.text` at
`0x40378000` (the I-bus alias of SRAM1 `0x3FC88000`), data at D-bus
`0x3FC98000` — both views of the emulator's modeled code region.

## `reloc` feature (M6 stretch prototype)

`cargo test -p lp-xt-elf --features reloc` builds `src/reloc.rs`: parse
relocatable Xtensa `.o` files, lay their `SHF_ALLOC` sections out in the
emulator's map (per object: `.literal*` pools first — `l32r` is backward-only —
then executable sections at I-bus `0x40378000`; data / rodata / zeroed bss at
D-bus `0x3FC98000`), resolve global symbols across objects, apply relocations,
and run (`reloc::run_linked(&[main_o, builtin_o], "lp_main", arg)`).

Relocation subset (prototype — a real linker is out of scope):

| type | handling |
|---|---|
| `R_XTENSA_NONE` | ignored |
| `R_XTENSA_32` | `*loc = S + A + *loc` (word; literals / data pointers) |
| `R_XTENSA_ASM_EXPAND` | no-op (relaxation annotation; we never relax) |
| `R_XTENSA_SLOT0_OP` | decode the instruction at the site with `lp-xt-inst`, recompute its PC-relative operand for the resolved target, re-encode the slot. Handles `call0/4/8/12`, `j`, `l32r`, RRI8 / BRI12 branches, and `beqz.n`/`bnez.n`; anything else is a clear `Slot0Unpatchable` error |
| everything else (`DIFF*`, `SLOT1..14_OP`, `*_ALT`, PLT/GOT/TLS kinds) | explicit `UnsupportedReloc` error naming the type |

Range / alignment violations (call target unaligned, `l32r` literal not in the
backward 256 KiB window, branch out of range) are hard errors, never silent
truncation.

## Tests

- `tests/fixtures.rs` — runs every toolchain-compiled fixture ELF from
  `fixtures/elf/` (build with `fixtures/build.sh`; tests skip with a note when
  the ELFs are absent so the stable host workspace never needs the esp
  toolchain). Expected outputs are host-side oracles mirroring each guest
  program.
- `tests/loader_hosted.rs` — synthetic-ELF loader validation (segments, bss,
  rejection paths) with guest code assembled by `lp-xt-inst`'s encoder.
- `tests/reloc_link.rs` (`--features reloc`) — links + runs the two-object
  fixtures from `fixtures/reloc/` (assembled by `fixtures/reloc/build.sh`;
  skip-with-note when absent) against host-side oracles, **and** runs the same
  pairs linked by GNU ld as a behavioral differential (results must agree —
  images are not byte-compared, GNU ld relaxes and we don't). Field-math unit
  tests for `call`/`l32r`/branch retargeting live in `src/reloc.rs` and need
  no toolchain.

## Provenance

Original code. ELF parsing is delegated to the permissively-licensed `object`
crate (Apache-2.0/MIT), the same dependency `lp-xt-inst`'s objdiff uses; no
ELF handling was hand-rolled beyond reading `object`'s public API, and no GPL
source (binutils/GDB, QEMU) was consulted for code. See
`docs/adr/2026-07-29-license-provenance-discipline.md`.

For the `reloc` feature: relocation type numbers and `S + A` semantics are
facts from the Xtensa ELF psABI relocation appendix; what `R_XTENSA_SLOT0_OP`
does was understood by reading binutils `elf32-xtensa` **behaviorally** and by
diffing GNU `ld` output on the fixture objects — no binutils code was copied
or transliterated. Operand slot encodings come from `lp-xt-inst`
(LLVM-derived, Apache-2.0 w/ LLVM-exception); PC formulas from the Xtensa ISA
Reference Manual, cross-checked against `xtensa-esp32s3-elf-objdump`.
