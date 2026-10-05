# lpc-firmware-release

The firmware distribution contract. Three edges share it: `lp-cli` writes it
(the release assets), `lp-cloud-server` serves it (the
`/firmware/<target>/<release>/<file>` lookup on lightplayer.app), and Studio
and the update protocol's host crate (`lpa-update`) read it. `no_std` +
`alloc`, sans-IO: no clock, no filesystem, no HTTP.

It lives in `lp-core` although it is not engine-internal because
`lp-cloud-server` depends on `lp-core` and never on `lp-app`; the firmware
never links it.

## Names

| Name | Example | Rule |
|---|---|---|
| **target** | `esp32c6-4mb` | A line of builds (today's `lp-fw/builds/` id). `[a-z0-9][a-z0-9-]{0,63}`. **Opaque**: never parsed for a chip or a flash size, never renamed. |
| **version** | `2026.10.05-3` | A release, `YYYY.MM.DD-N`, tagged `v<version>`. A dev version (`abc1234`, `abc1234-dirty-101500PT`) is never in the store. |
| **build id** | `2026.10.05-3+abc123456789` | `<version>+<commit[..12]>`, exactly 12 lowercase hex. JSON key `buildId`, never `build`. |

## `ota-manifest.json`, format 1

One target's build in one release: its identity (`target`, `chip`,
`version`, `commit`, `wireProto`), what a board must have to take it
(`requires.layout` must be **equal**, `requires.loader` is a **minimum**),
its two pieces (`core`, `engine`: file, length, SHA-256), their compressed
`encodings`, and the USB `package` it came from. No timestamp, no `buildId`
field (`OtaManifest::build_id()` derives it). The compatibility pin is
`tests/fixtures/ota-manifest.v1.json`; the schema is
`schemas/ota-manifest.schema.json` (`just schema-gen`).

**Compatibility.** Readers refuse another `format` and ignore unknown
fields. An additive optional field keeps `format: 1`; so does a new
encoding. `format` is bumped only when an old reader would misread
something, and the old form is then still written beside the new one.
`version`, `commit` and `target` are never re-rendered.

**Encodings, by `id` alone.** An entry whose `id` a reader does not know is
skipped, never an error, whatever its other keys look like (only `id` is
required). `id: 1` is a `.z` file per piece — independent raw-deflate
streams, one per 4 KiB chunk, back to back — with `chunks[i]` the
compressed length of chunk `i` and **`0` meaning "no compressed form, send
raw"**. What encoding 1 *means* (the dictionary rule) is the update
protocol's `lpc-update`; this crate only checks the structure (chunk count,
`chunks` summing to the file length). Whether a chunk decodes is proven at
package time with M4's prover.

**The board reports the same identity.** A board running release R says
the same `target`, `chip`, `version`, `wireProto`, core/engine hashes and
lengths, and `buildId = build_id()`; the table is on `OtaManifest`'s doc.

## The lookup

```text
https://lightplayer.app/firmware/<target>/<release>/<file>
```

- `<release>`: `latest`, a release version, or a build id (`%2B` reads as
  `+`). A `<release>` starting with a digit that is neither (a dev version, a
  build id with 7 or 40 hex) is refused. **Every `<release>` that does not
  start with a digit is a named selector, and the words are reserved**:
  `latest` is the only one in use; `stable`, `beta`, … parse as
  `ReleaseSelector::Reserved` and are answered 404 today, so channels can
  arrive later, additively.
- `<file>`: `ota-manifest.json`, or a file the manifest names
  (`OtaManifest::files()`). `OtaManifest::verify(file, bytes)` checks length,
  then SHA-256.
- Release assets are `<target>.<file>` (`asset_name`, `split_asset_name`).

## Files

| File | Concept |
|---|---|
| `ota_manifest.rs` | `OtaManifest`, `Requires`, `PieceFile`, `PackageRef`; parse, validate, `build_id()` |
| `ota_encoding.rs` | `EncodingEntry` (lenient), `Encoding1`, `EncodedPieceFile` |
| `ota_manifest_error.rs` | why a manifest was refused |
| `firmware_file_check.rs` | `files()` allowlist, `verify()` |
| `target_name.rs` | `TargetName` |
| `release_version.rs` | `ReleaseVersion`, `BuildId` |
| `dev_version.rs` | dev versions (never in the store) |
| `release_selector.rs` | `ReleaseSelector` and the reserved words |
| `firmware_lookup_path.rs` | `FirmwareLookupPath` |
| `lookup_error.rs` | why a lookup path was refused (every reason is a 404) |
| `release_asset_name.rs` | `<target>.<file>` |
| `lower_hex.rs` | SHA-256 and hex spelling |
