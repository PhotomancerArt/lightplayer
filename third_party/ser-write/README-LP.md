# ser-write — LP fork

Vendored from crates.io **ser-write 0.3.1** (upstream
<https://github.com/royaltm/rust-ser-write>, commit
`205b12823a718f78422cb04ceac4609ed7d00a36` per the crate's
`.cargo_vcs_info.json`), MIT OR Apache-2.0; both licenses are alongside
(`LICENSE-MIT`, `LICENSE-APACHE`). Patched in through the root `Cargo.toml`'s
`[patch.crates-io]`. The version stays `0.3.1`: the patch table substitutes a
source, never a version.

The crates.io files are kept verbatim except that CRLF line endings were
normalised to LF (as `third_party/ser-write-json` already is), the packaging
files (`.cargo_vcs_info.json`, `Cargo.lock`, `Cargo.toml.orig`) were dropped,
`Cargo.toml` gained an empty `[workspace]` table (as `ser-write-json`'s has) so
`just test-ser-write` can run its tests standalone, and the diff below.

## The diff: a token hook on `SerWrite`

`src/lib.rs` gains:

- `Token<'a>`, a structural token: `Str`, `Key`, `U64`, `I64`, `F32`, `F64`,
  `Bool`, `Null`, `MapBegin`, `MapEnd`, `SeqBegin`, `SeqEnd`, `Separator`,
  `Blob(&[u8])`;
- `SerWrite::token(&mut self, Token) -> Result<bool, Self::Error>`, a provided
  method returning `Ok(false)` ("not taken");
- `const SerWrite::TAKES_TOKENS: bool = false`;
- the `&mut T` blanket impl forwards both.

Why: the fork of `ser-write-json` (`third_party/ser-write-json`) offers every
output site to its sink as a token before writing JSON text, so a sink that
takes tokens (the packed wire encoding, `lp-json-pack`) builds its frame
without re-lexing the text. Every existing sink declines by default and sees
exactly the bytes it saw before. Plan: `lp2025/2026-09-23-1701-lp-json-pack`,
phase P2.
