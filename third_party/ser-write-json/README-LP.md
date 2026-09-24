# ser-write-json — LP fork

Vendored from crates.io **ser-write-json 0.3.1** (upstream
<https://github.com/royaltm/rust-ser-write>), MIT OR Apache-2.0; both licenses
are alongside (`LICENSE-MIT`, `LICENSE-APACHE`). Patched in through the root
`Cargo.toml`'s `[patch.crates-io]`. The version stays `0.3.1`: the patch table
substitutes a source, never a version. The sources were reformatted with
`rustfmt` when vendored, and the packaging files were dropped.

## The diffs

### `serde_json::RawValue` is written verbatim (2026-07)

`serialize_struct` recognizes serde_json's private `RawValue` marker and
writes the contained JSON fragment as-is, the way serde_json does, instead of
as a one-field object. See `docs/adr/2026-06-27-ser-write-json-raw-value.md`.

### The token hook (2026-09, plan `lp2025/2026-09-23-1701-lp-json-pack`, P2)

The vendored `ser-write` (`third_party/ser-write`) gives `SerWrite` a provided
`token` method that declines by default. The serializer (`src/ser.rs`):

- offers a `Token` at every output site (strings, keys, numbers, bools, null,
  `{ } [ ]`, `,` and `:`) and writes its usual JSON text only when the sink
  declines, so a sink that declines sees exactly the bytes it always did;
- recognizes the `"$lp::blob"` newtype marker (`BLOB_MARKER`, the same trick as
  `RawValue`): it offers `Token::Blob(bytes)`, and otherwise serializes the
  inner value as usual. `lpc-wire`'s `serde_base64` uses it, so base64 fields
  reach a token sink as raw bytes;
- writes struct field prefixes (`,` key `:`) through one helper per field,
  and keeps it and the token-offer helpers out of line (`#[inline(never)]`):
  inlined, they are paid once per field of every wire type. Measured on the
  C6 image against upstream: inlined +49 KB of flash and +7 % JSON-path
  instructions, out of line −4.7 KB and +27 %. The numbers are on the helpers
  in `src/ser.rs`.

Two paths still reach a token-taking sink as text, each as one complete JSON
value: `RawValue` fragments, and `collect_str` (a quoted, escaped string).
