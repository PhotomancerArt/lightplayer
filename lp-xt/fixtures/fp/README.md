# lp-xt FP fixtures

Two things live here, both assembler-derived and both about the Xtensa
single-precision FP coprocessor (M6 of the native-f32 roadmap):

| File | What it is |
|---|---|
| `fp_subset.S` | Every instruction in M6's normative FP / Boolean / special-register subset, at both ends of every operand field. The **objdiff target** — the mechanical oracle for the encoder and disassembler. |
| `probe.S` | The **capability probe payloads** for the ESP32-S3 desk session. Built and checked in; **not yet run on hardware.** |

Build both with `./build.sh` (needs the espup toolchain under
`~/.rustup/toolchains/esp`). Output lands in `obj/`, which is gitignored —
regenerable, never committed.

## Why these are `.S` files and not byte arrays

House rule (AGENTS.md): **instruction bytes are assembler-derived or
hardware-verified, never hand-written from memory.** The spike lesson behind it
was that 2 of 3 recalled encodings were wrong. These files hold mnemonics only;
the bytes come out of `xtensa-esp32s3-elf-as`.

The same rule is why binutils' *output* is used freely here while its *source*
is off limits — see `docs/adr/2026-07-29-license-provenance-discipline.md`.

## The objdiff oracle

```bash
./build.sh
cargo run -p lp-xt-inst --features objdiff --bin objdiff -- obj/fp_subset.elf
```

Every instruction must MATCH: `lp-xt-inst`'s disassembly of the bytes has to
agree with `xtensa-esp32s3-elf-objdump`'s, mnemonic and operand values. As of
M6 P1 this reports **134 / 134 matched, 0 mismatches, 0 unsupported**.

An instruction that is not in `fp_subset.S` is not covered by the mechanical
oracle, whatever the unit tests say. When the subset grows, grow this file.

### Deriving a new encoding

This is also the procedure that produced every golden byte in
`lp-xt-inst/tests/fp_golden_vectors.rs`:

```bash
printf '\t.text\n\t<the instruction>\n' > /tmp/one.S
$XT/xtensa-esp32s3-elf-as -o /tmp/one.o /tmp/one.S
$XT/xtensa-esp32s3-elf-objdump -d /tmp/one.o       # the 24-bit word, MSB first
$XT/xtensa-esp32s3-elf-objcopy -O binary -j .text /tmp/one.o /tmp/one.bin
xxd -p /tmp/one.bin                                # the memory bytes, LE
```

Note the two renderings differ: objdump prints the instruction *word*
(`0a0120` for `add.s f0, f1, f2`), memory holds it little-endian
(`20 01 0a`). Goldens are memory bytes.

## The capability probes

`probe.S` answers a question no document answers: **does the ESP32-S3's FPU
configuration actually implement the divide/square-root helper family?** Whether
those instructions exist is a build-time option of the Xtensa core, and Espressif
does not document it to that standard. If they are absent, M7's division lowering
is a different design.

Each probe is its own ELF section, so one payload can be extracted and handed to
the experiment repo's crash-recovering runner:

```bash
./build.sh
./probes.sh              # prints a Rust (name, bytes) table; blobs in obj/probes/
```

Verdict rule, per probe:

| Observed | Meaning |
|---|---|
| returns the staged value | instruction **present** and CP0 armed |
| Exception, EXCCAUSE 32 | present, but **`CPENABLE` not armed** |
| Exception, EXCCAUSE 0 | instruction **absent** (illegal) |

The `unarmed` probe deliberately skips the `wsr.cpenable` + `isync` preamble, so
the arming requirement is confirmed by the *difference* between it and the rest,
not assumed.

Several probes return a computed value rather than a staged id — `rfr_wfr`
(42 through the FR file), `float_trunc` (7 through the converters), `lsi_ssi` and
`lsx_ssx` (42 through memory), `oeq_bf` / `oeq_movt` / `oeq_rsr_br` (three
independent readback paths for one compare), `rur_fcr` / `rur_fsr` (the reset
values). A wrong answer there is as loud as a fault.

**Status: not yet run.** Every verdict in `lp-xt-inst/src/fp.rs`'s subset table
still reads `NOT PROBED`, and that is a finding to report, not a default to
assume. The desk session is scheduled; see the M6 sub-plan's P1 phase file.
