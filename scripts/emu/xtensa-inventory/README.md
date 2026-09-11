# Xtensa firmware ISA inventory scripts

Throwaway (but committed) scripts for the M0 census behind
`docs/reports/2026-09-1x-xtensa-firmware-isa-inventory.md`. They exist so the
report's numbers are reproducible — the scope study's own scripts lived in a
scratchpad and are gone.

> ## ⚠️ Pass `--objdump` for the chip you are measuring
>
> **`census.py` and `sweep.py` default to `xtensa-esp32-elf-objdump` — the
> CLASSIC's.** That default was right when the only artefacts were the
> classic's; it is wrong for every other Xtensa chip, and it fails *quietly*,
> with a plausible number rather than an error.
>
> Measured on the S3 image (M6 P01, 2026-09-11), same bytes, same decoder,
> same `objdiff` binary — only the objdump differs:
>
> | objdump | instructions | coverage | mismatched | unsupported kinds | top unsupported |
> |---|---:|---:|---:|---:|---|
> | `xtensa-esp32-elf-objdump` (the default) | 726,615 | **99.19 %** | **8** | 19 | `lsi` 5,550, then the `f64*` family |
> | `xtensa-esp32s3-elf-objdump` (correct) | 725,373 | **99.53 %** | **0** | 99 | the `ee.*` PIE family |
>
> This is the *same failure mode* as the 2026-09-10 per-section correction
> below and it has the **same tell: a flood of `lsi`.** There the Xtensa
> configuration was lost by lifting a section; here it is lost by asking a
> different chip's objdump. Either way objdump falls back to an opcode table
> where a loose `lsi` entry beats real entries, and the `f64*` emulation
> mnemonics the classic has (and the S3 does not) appear out of nowhere.
>
> `objdiff` itself already defaults to `xtensa-esp32s3-elf-objdump`
> (`lp-xt/lp-xt-inst/src/bin/objdiff.rs`), so the two disagree; `census.py`
> overrides it through `XT_OBJDUMP`. The defaults are left alone deliberately
> — changing them would silently move the classic's committed numbers — so
> **name the objdump explicitly on every run**, the way the S3 report's
> commands do.
>
> `mmio-census.py` and `in-symbol.py` take `--chip` / a chip-prefixed default
> instead, and pick the right binary on their own.

## `census.py` — exact decoder coverage

Runs `lp-xt-inst`'s `objdiff` binary (decode()-exact, not a mnemonic-name-table
proxy) **once, over the artefact whole** — `objdiff` (as of M1 P1, PR #660)
iterates every `SHF_EXECINSTR` section of the ELF it is given itself, so this
script no longer has to. The report: instructions decoded, ranked unsupported
mnemonics, and the special/user register census (`rsr`/`wsr`/`xsr` targets),
plus a per-section byte/vma table read back from `objdiff`'s own section
listing (metadata only — the instruction counts are never split per section,
only measured once for the whole artefact).

> **2026-09-10 correction.** Before this date the script closed the
> "`objdiff` only sees `.text`" gap itself by lifting each CODE section into
> its own one-section ELF with `objcopy` and summing per-section `objdiff`
> reports. That **under-measured**: lifting a section drops the Xtensa
> configuration (`e_flags`) the original ELF carries, so `objdump` falls back
> to a config-less opcode table where a loose `lsi` entry beats real MAC16
> entries. Same bytes, same `objdump` binary, same decoder — the classic mask
> ROM read **96.52 % / 60 mismatches** section-by-section and **99.96 % / 0
> mismatches** read whole (PR #660's finding; this script now reproduces the
> latter). The v3 image moved **98.48 % → 98.86 %** the same way. The
> bootloader's raw-binary path (`--base`, below) was never an instance of
> this: a raw blob has no Xtensa configuration to lose in the first place, so
> it is unchanged.

Build `objdiff` first:

```
cd lp-xt/lp-xt-inst
timeout 300 cargo build -p lp-xt-inst --features objdiff --bin objdiff --release
```

Then, for an ELF artefact (the shipped image, a ROM ELF):

```
scripts/emu/xtensa-inventory/census.py <artefact.elf> \
  --objdiff lp-xt/lp-xt-inst/target/release/objdiff \
  --json /tmp/census-<name>.json
```

For a raw binary at a known load address (the bootloader, carved from a merged
espflash image):

```
scripts/emu/xtensa-inventory/census.py <blob.bin> --base 0x1000 \
  --objdiff lp-xt/lp-xt-inst/target/release/objdiff \
  --json /tmp/census-bootloader.json
```

`--skip SECTION` drops a named CODE section from an ELF measurement (repeat
for more than one). It uses an ELF-to-ELF `objcopy --remove-section`, not a
lift through a raw-binary intermediate, so it does not reintroduce the bug
above — `objcopy` copies the input ELF's header wholesale and only strips the
named section.

Licence note (AGENTS.md): binutils (`objdump`/`objcopy`) is used here as a
tool whose *output* is fact — disassembly text and section headers — never as
a source of tables or logic copied into this repo.

## `sweep.py` — literal-pool collision sweep

A symbol-seeded, width-following sweep over the shipped v3 image (study §4.4):
walk decoded instruction widths forward from each ELF symbol's start, and
count bytes decoded that fall outside `[symbol, symbol+size)` or inside a
`.literal`/`.rodata` section — i.e. bytes a naive discovery walk would
mis-decode as instructions because it ran into an interleaved literal pool.

```
scripts/emu/xtensa-inventory/sweep.py <artefact.elf>
```

Report: one collision count, plus the worst-offending symbols by collision
byte count.

## `in-symbol.py` — is an unsupported site real code, or a literal pool?

`census.py`'s unsupported ranking is misleading on its own, and on the S3 it
is misleading in a way that would change a milestone's scope: 87 of the S3
image's 99 unsupported kinds are PIE `ee.*` vector instructions, which reads
as "this image needs a vector unit". It does not — Xtensa literal pools sit
inside `.text`, and objdump, reading the S3's PIE-bearing configuration,
emits plausible `ee.vmulas.*` mnemonics for constants.

The discriminator is the ELF symbol table: real code lives inside some
`[sym, sym+size)`; an interleaved literal pool does not.

```
scripts/emu/xtensa-inventory/in-symbol.py <artefact.elf> --census <census.json>
```

⚠️ It tests only **pure-unsupported** mnemonics — the ones that do NOT also
appear in the census's supported list. A mnemonic in both cannot be
attributed site by site by matching objdump's text: on the S3 image `retw.n`
is decoded 7,414 times and misread once, and matching the name finds all
7,415. The mixed mnemonics are excluded **and named in the output**, never
silently dropped. On the S3 image that is three kinds and 23 sites (`ret`,
`retw.n`, `any4`), leaving 3,378 of 3,401 testable — and **all 3,378 fall
outside any sized symbol**.

## `mmio-census.py` — which peripheral blocks, and which ROM entry points

The two questions a *machine* is built from, answered before one exists:
which peripheral blocks the image touches and at which offsets (bucketed
against the chip PAC's own `Periph<..., 0xBASE>` declarations, never a
datasheet, and attributed to the touching symbol), and which mask-ROM entry
points it reaches (resolved against `esp-rom-sys`'s `PROVIDE`d symbols).

```
scripts/emu/xtensa-inventory/mmio-census.py <artefact.elf> --chip esp32s3
```

Two things it does deliberately, both of which a naive version gets wrong:

- **It never snaps a ROM address to the nearest symbol below it.** Multiple
  `PROVIDE`s share addresses in the S3 ROM linker scripts — `0x4000_1c68` is
  both `r_llc_rem_phy_upd_proc_continue_hook` and `MD5Update` — so only exact
  matches resolve, every name at an address is printed, and an address with
  no exact match is listed as unresolved.
- **It caps a PAC base's claim at the block stride.** Without the cap
  `bisect` hands RTC fast (`0x600f_e000`) to WCL (`0x600d_0000`) and invents
  a `WCL+0x2e000` that is not a register anywhere.

The `deref` column is how many of a block's sites had a dereference the
script's linear pointer tracker could follow. A block whose `deref` is **0**
is a constant that merely looks like a peripheral — on the S3 image exactly
two rows are zero, `UART0` (51 × `0x6000_0020` + 2 × `0x6000_0000`) and
`RTC_SLOW` (7 × `0x5000_0000`, from `libm::rem_pio2f`) — and the audit list
below the table names every such value.
