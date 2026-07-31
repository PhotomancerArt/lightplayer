---
status: carried
since: 2026-07-30
logged: 2026-07-30
area: lp-fw/fw-esp32c6 (fw-esp32s3 is already clean)
related:
  [
    "../../lp-fw/fw-esp32c6/src/flash_storage.rs",
    "../../lp-fw/fw-esp32s3/src/flash_storage.rs",
    "../adr/2026-07-30-esp32s3-partition-floor.md",
  ]
---
# Partition-table facts are hand-transcribed into firmware constants

**Shape** — `lp-fw/fw-esp32c6/src/flash_storage.rs` hardcodes the `lpfs`
partition's location as Rust constants copied by hand from `partitions.csv`:

```rust
/// lpfs partition offset (from partitions.csv)
const LPFS_PARTITION_OFFSET: u32 = 0x310000;
/// 960KB partition = 240 blocks
const BLOCK_COUNT: u32 = 240;
```

Nothing checks them against the table that is actually flashed. The comment
"(from partitions.csv)" is the entire enforcement mechanism. `esp-bootloader-esp-idf`
is already a dependency of both firmware crates and can read the flashed table
at runtime, so the duplication is avoidable rather than inherent.

**Why it matters, concretely** — this nearly caused silent flash corruption
when the S3 app layer was ported (M3 P2, 2026-07-30). The port brief said the
file addressed partitions by name and would carry over unchanged. It does not,
and it would not: M3 P1 moved the S3's `lpfs` to `0x610000`, so the C6's
`0x310000` lands in the middle of the S3's **factory** partition. A verbatim
copy would have made the first filesystem mount erase running code, with no
diagnostic and no obvious link back to the cause.

It was caught by reading the file rather than trusting the brief. The next
person may not.

**Why it is acceptable now** — the constants are correct *for the C6 as it
stands today*, and the C6's `partitions.csv` has been stable. The failure mode
needs someone to change that table (or copy the file to a chip with a different
one) without noticing the constants.

**What makes it unacceptable later** — any of: the C6's partition table
changes; a third firmware crate copies the C6 file the way the S3 nearly did;
or per-board partition tables land (already a deferred decision in
`docs/adr/README.md`), at which point a compile-time constant cannot be right
for every board by construction.

**The fix** — port the C6 to the S3's pattern.
`lp-fw/fw-esp32s3/src/flash_storage.rs` already does it: `LpfsPartition::locate()`
reads the flashed table via `esp_bootloader_esp_idf::partitions` and matches the
`lpfs` label, and returns `None` — for the caller to fail loudly on — when the
label is absent, which is exactly the signature of an image flashed without
`--partition-table`. No second copy of the layout exists to drift. The change is
small and mechanical; it was left out of M3 P2 only because that phase was
scoped not to touch `fw-esp32c6`.

**Workarounds** — When changing `lp-fw/fw-esp32c6/partitions.csv`, grep
`fw-esp32c6/src` for the old offset and block count before assuming the change
is confined to the CSV.

**Incident log**
- **2026-07-30** — Filed during M3 P2 (S3 board port). No live incident: the
  hazard was recognized during the port and the S3 got a runtime lookup instead
  of the copied constants. Recording it because the near miss was one careless
  copy away from silent corruption of a running image.

**Exit criteria** — `fw-esp32c6/src/flash_storage.rs` derives the `lpfs`
offset and length from the flashed partition table, and no firmware crate
carries a transcribed copy of a `partitions.csv` value.
