# lp-xt FP fixtures

Two things live here, both assembler-derived and both about the Xtensa
single-precision FP coprocessor (M6 of the native-f32 roadmap):

| File | What it is |
|---|---|
| `fp_subset.S` | Every instruction in M6's normative FP / Boolean / special-register subset, at both ends of every operand field. The **objdiff target** — the mechanical oracle for the encoder and disassembler. |
| `probe.S` | The **capability probe payloads** for the ESP32-S3 desk session. Built and checked in; **run 2026-07-31, all 26 probes PRESENT** (below). |

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
M6 P7 (after adding `mksadj.s`, missed at P1 — see
`docs/defects/2026-07-31-mksadj-missing-from-fp-subset.md`) this reports
**136 / 136 matched, 0 mismatches, 0 unsupported**.

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

**Status: run 2026-07-31.** All 26 probes came back **PRESENT** on a XIAO-class
ESP32-S3 (16 MB, MAC `d8:3b:da:47:29:70`) with zero crashes and zero reboots.
The subset table's Silicon column in `lp-xt-inst/src/fp.rs` is filled from that
session; the full record is `p1-silicon-results.md` in the M6 planning
directory. PRESENT means *the instruction executed* — its numeric behavior is
P6's question, not P1's.

The `unarmed` probe returned its staged value rather than faulting: the S3
arrives with `CPENABLE` **already armed** under the esp-hal boot chain. No
`wsr.cpenable` exists in esp-hal 1.1.1 or xtensa-lx-rt 0.22 startup, so the
provenance is presumably ROM or the second-stage bootloader and is *not* pinned.
M7 arms it defensively regardless.

## The FP-ABI probe (no hardware)

`abi_probe.c` + `abi_probe.sh` answer a different question that needs no board:
**which FRs does the esp toolchain treat as callee-saved?** M5 compiles the f32
builtins with this toolchain at `-O3` and M7 must lay out a frame that survives
calls into them — and the FR file is flat, so nothing is preserved for free.

Answer, from `xtensa-esp32s3-elf-gcc 14.2.0` (esp-14.2.0_20240906) at `-O3`:

> **No FR is callee-saved. Every FR is call-clobbered.**

The evidence is the shape of the generated code, not a claim in a document. Six
float values live across a `call8`, and the compiler:

- passed the float arguments in **address** registers `a2..a7`, moving them into
  FRs with `wfr` only at the point of use, and returned the result the same way
  (`rfr a2, f0`);
- spilled the surviving values with plain integer `s32i.n` to the frame *before*
  the call, and reloaded them with `lsi` *after* it;
- stored **no** FR before the call and restored none after it, then reused
  `f0..f5` freely on the far side.

Which is exactly what a flat FR file with no free preservation would produce: if
saving is not free, the ABI does not ask a callee to do it. Consequence for M7:
an FR value that must outlive a call is the *caller's* problem, and the frame
needs the spill slots.

One incidental measurement worth the ADR: the toolchain **contracts** `a*b + c`
into `madd.s` at `-O3`. `docs/design/float.md` §4 files expression contraction
under target-defined, and this is that target being definite about it.
