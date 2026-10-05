# lp-crc32

The one CRC-32 (IEEE / ISO-HDLC: reflected, polynomial `0xEDB88320`,
check value `crc32(b"123456789") == 0xCBF43926`) of the boot and update
records. `no_std`, no `alloc`, no dependencies, table-free and bitwise —
it runs over a few dozen bytes at a time, and on the ESP32-C6 flash is the
binding constraint.

```rust
let whole = lp_crc32::crc32(b"2026.10.05-3+abc123456789");
let mut crc = lp_crc32::Crc32::new();
crc.update(b"2026.10.05-3");
crc.update(b"+abc123456789");
assert_eq!(crc.finish(), whole);
```

## Who uses it

| Record | Field | Crate |
|---|---|---|
| Update-progress record v1 (`factory + 0x5000`) | `build` (the build hash) and `crc` (of bytes 0..52) | `lpc-update` (`transfer_record`) |
| The board manifest | `refusedBuild`: the build hash a host recomputes from a build id | `lpc-update` (`build_hash`), `lpa-update` |
| Boot record v1 | `build` and `crc` | `lp-bootctl` (moves onto this crate after the split image merges; until then it keeps a private copy, which this crate's tests match vector for vector) |

These values are compared across a board and every future host, and two of
the records are forever formats, so the CRC is defined once, here, instead
of copied beside each record (`one-way-doors.md` §7, §10).
