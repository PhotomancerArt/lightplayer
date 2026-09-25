# lp-json-pack

**JSON Pack**: a compact binary form of JSON that decodes back to
**byte-identical JSON text**. LightPlayer's board sends its wire replies packed;
everything downstream of the decoder still sees today's JSON.

The format spec (tag tables, varints, text-exact decimals, dictionary codes,
back-references, blobs, framing, and the deviations from Ion with their
measured bytes) is the crate documentation in [`src/lib.rs`](src/lib.rs):

```bash
cargo doc -p lp-json-pack --all-features --open
```

## Shape

- `#![no_std]`, no `alloc` on the device path. It knows no wire vocabulary:
  the `Dictionary` (keys, values, hash tables, all `&'static`) is injected, and
  `lpc-wire` owns the wire's one. `Dictionary::fingerprint()` is a `const fn`
  the two ends compare.
- `PackEncoder` — events in, one frame out into a caller buffer; `Err(Full)`,
  never a panic.
- `PackLexer` (feature `lex`) — JSON text into the same encoder.
- `decode` — a frame back to the exact JSON text through a `JsonOut` sink.
- `cobs_frame` — `0x00 'P' COBS(payload) 0x00`, built in place.
- `FrameScanner` — a byte stream into text and decoded frames.

Features: `lex` (the device uses this one), `alloc` (Vec sinks and scanner,
`DictionaryBuilder`), `std` (both, plus the `json-pack` tool).

## Tool

```bash
cargo run -p lp-json-pack --features std --bin json-pack -- roundtrip tests/fixtures/choker-lens-sample.txt
```

`encode` / `decode` pipe a capture through the packed form and back.
`lp-cli wire unpack` is the user-facing reader; this is the codec's own oracle.

## Tests

`cargo test -p lp-json-pack --all-features` runs the round trip over the
committed traffic sample (`tests/fixtures/`, provenance in its README), the
f32 subset, and the framing and torn-frame cases. The full f32 sweep is
ignored by default:

```bash
cargo test -p lp-json-pack --features lex --release --test f32_text -- --ignored
```
