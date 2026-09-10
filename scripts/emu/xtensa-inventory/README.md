# Xtensa firmware ISA inventory scripts

Throwaway (but committed) scripts for the M0 census behind
`docs/reports/2026-09-1x-xtensa-firmware-isa-inventory.md`. They exist so the
report's numbers are reproducible — the scope study's own scripts lived in a
scratchpad and are gone.

## `census.py` — exact decoder coverage

Runs `lp-xt-inst`'s `objdiff` binary (decode()-exact, not a mnemonic-name-table
proxy) over every CODE section of an artefact, lifting each section into a
one-section ELF with `objcopy` so `objdiff`'s `.text`-only scan sees it. Sums
per-section reports into a whole-artefact total: instructions decoded, ranked
unsupported mnemonics, and the special/user register census (`rsr`/`wsr`/`xsr`
targets).

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
