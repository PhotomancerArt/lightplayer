# lp-deflate

A small `no_std`, no-alloc **raw deflate (RFC 1951) decoder**. General
purpose — OTA (M4 of `lp2025/2026-10-03-1330-ota-firmware-updates`) is the
first user, sending each 4 KiB firmware chunk compressed against the 32 KiB
of the image already written just before it, but nothing here names OTA.
LED frame data (uncompressed today) is a plausible second user; see
`compression-notes.md` in the OTA split-link spike for the measurements
that motivated this crate.

The crate documentation in [`src/lib.rs`](src/lib.rs) is the fuller version
of this file:

```bash
cargo doc -p lp-deflate --open
```

## Shape

- [`inflate(src, buf, start)`](src/inflate.rs) decodes `src` into
  `buf[start..]`. `buf[..start]` is a **preset dictionary** — history a
  back-reference may copy from without this call having produced it, which
  is exactly the OTA shape: the device already holds the previous 32 KiB
  because it just wrote it.
- All three block kinds: stored, fixed-Huffman and dynamic-Huffman (RFC 1951
  §3.2.4/§3.2.6/§3.2.7).
- [`Error`](src/error.rs) covers every way decoding can fail: truncated
  input, a corrupt block, a full output buffer, or a back-reference
  reaching before the buffer's start. `inflate` never panics — every array
  access into caller-controlled data goes through a checked accessor first
  (`tests/corruption_fuzz.rs` is the seeded loop that checks this directly).
- `#![no_std]`, no `alloc`. Every table is a fixed-size array sized to RFC
  1951's own limits (288 literal/length symbols, 30 distance symbols, 19
  code-length symbols); the only "memory" besides the stack is the caller's
  `buf`.

**No encoder.** That is out of scope for this crate (Q4/Y7 on the OTA
roadmap); only decoding is needed today.

## Provenance

Written independently from RFC 1951, the Internet Standards Document —
never adapted from zlib, miniz, or puff source (see `AGENTS.md`'s
license-discipline rule). `miniz_oxide` (MIT) is a dev-dependency only,
used as an oracle in tests: this crate's tests check that it decodes what
`miniz_oxide` encodes, never the reverse.

The design started from a prototype written the same way for the OTA
split-link spike
(`lp2025/2026-10-01-1854-ota-split-link-spike/data/compression/tinyflate`):
this crate is that decoder, carried over whole, reorganized into this
crate's own file layout (one file per concept — bit reader, Huffman
decoder, the RFC's fixed tables, the block loop), with the fixed-Huffman
encoder the prototype also had left behind.

## Tests

- `tests/oracle_firmware.rs`: `inflate` decodes what `miniz_oxide` encodes,
  at levels 0/1/6/9, on a real firmware image already vendored in this repo
  (`lp-fw/bootloaders/esp32c6-bootloader-idf-v5.5.1.bin`, Apache-2.0) — both
  whole and split into 4 KiB OTA-shaped chunks.
- `tests/preset_dictionary.rs`: preset-dictionary round trips, generated
  against Python's own `zlib` (`zdict=`, an implementation independent of
  both this crate and `miniz_oxide`) by
  `tests/fixtures/gen_preset_dict_vectors.py` and committed as fixtures —
  including a dictionary that is a full 32 KiB window with a match at the
  largest distance RFC 1951 can express, and the real firmware image sliced
  into sequential OTA-shaped sectors.
- `tests/corruption_fuzz.rs`: a seeded, dependency-free PRNG loop (there is
  no `cargo-fuzz` convention in this repo to extend) feeding 3,000+ cases
  each of truncation, single-bit flips, and both combined. Truncation
  always errors (deflate has no "stop here" short-circuit short of running
  out of bits); a single bit flip, measured rather than assumed, surfaces
  as an error only part of the time — a stored block (likely at low
  compression levels) carries no Huffman coding, so flipping a bit in its
  literal data just changes one output byte rather than desyncing anything.
  Neither kind of corruption, nor an undersized output buffer, ever panics.

## What it costs on the C6

Measured the same way as the spike prototype: a release staticlib
(`opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`)
built for `riscv32imac-unknown-none-elf` with a single `extern "C"` entry
point so the decoder stays reachable, then read with `rust-nm --print-size
--size-sort --demangle` (code) and `rust-readobj --stack-sizes` under
`RUSTFLAGS='-Z emit-stack-sizes'` (stack) — the same tool-based method
`just esp-stack-sizes` uses on firmware images.

| | Size |
|---|---|
| Code (the decode path: bit reader, both `Huff::new` monomorphizations, the block loop, and the entry point, after inlining) | **3,184 B** (≈ 3.1 KiB) |
| Stack, entry point's own frame (`inflate` plus everything inlined into it: the block loop, both table builders, `Huff::decode`) | **3,136 B** (0xC40, ≈ 3.06 KiB) |
| Stack, deepest callee not inlined (`Huff::<288>::new`, called while building either table) | 672 B (0x2A0) |
| **Worst-case call-stack depth for one `inflate` call** | **≈ 3,808 B** (≈ 3.72 KiB) |

The code-size figure lines up closely with the spike's own measurement
(~3.3 KB) — expected, since this crate carries that prototype's decoder
over unchanged. The stack figure does not: the spike's compression notes
quote "~1.1 KB of stack for dynamic tables," which reads as a manual tally
of the dynamic block's own data structures (the `lens` table plus both
`Huff` tables' arrays, ≈ 1.1 KB) rather than a tool-measured function
frame. Measuring the *compiled* frame the same way as the code-size figure
— on the unmodified prototype itself, not only this crate — gives the same
≈ 3.1 KB top-level frame reported here, so the gap is in what the two
numbers count, not a regression introduced by this crate's reorganization.
3.1–3.8 KiB of stack is still modest next to the C6's stack budget, but a
future caller sizing a worker stack for this decoder should use this
figure, not the spike's note.
