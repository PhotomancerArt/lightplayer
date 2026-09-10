# Xtensa firmware ISA inventory scripts

Throwaway (but committed) scripts for the M0 census behind
`docs/reports/2026-09-1x-xtensa-firmware-isa-inventory.md`. They exist so the
report's numbers are reproducible — the scope study's own scripts lived in a
scratchpad and are gone.

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
