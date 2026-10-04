# Device backup archive (format 2)

What a board's filesystem becomes when it is backed up: a ZIP of every file
and directory in its `lpfs`, read raw over the bootloader and mounted in Rust
(on the host, or in wasm in the browser) by the same littlefs implementation
the firmware uses. It works on a board that **cannot boot**.

Format 2 exists for the 2026-10 C6 repartition
(`docs/adr/2026-10-02-c6-repartition-and-layout-migration.md`): every layout
migration stores one of these in the browser — and offers it as a download —
**before** it writes anything to the board, because for a few seconds of the
write it is the only copy of the board's files. `lp-cli hardware lpfs save`
writes the same archive, and `lp-cli hardware lpfs restore` puts one back.

Format 1 (July 2026) was Studio's danger-zone backup, deleted with that UI on
2026-08-25 and revived here, in `lpa-link`, so Studio and `lp-cli` share one
implementation (`docs/adr/2026-07-31-device-backup-archive-format.md`).

## Posture

**Support-facing, but shaped as if it were public.** We do not promise this
format to users, and we design it as though we might, because restores read
these archives and a layout invented for convenience today is a layout
somebody has to reverse tomorrow.

Alpha versioning rule, same as share envelopes
(`docs/adr/2026-07-28-share-envelopes.md`): **version and refuse, never
migrate.** A reader that meets any `formatVersion` but 2 says so, by number.

## Layout

```
manifest.json                              ← archive root, written first
files/.lp/                                 ← directories are entries too (v2)
files/.lp/device.json                      ← device paths, mirrored verbatim
files/hardware.json
files/projects/porch/
files/projects/porch/project.json
files/projects/porch/shader.glsl
```

- **Device paths are mirrored verbatim** under the single `files/` root.
  Recovering a device path is stripping `files/` and prepending `/`.
- The `files/` prefix exists so `manifest.json` cannot collide with a file
  the device keeps at its filesystem root.
- **Directories are entries** (a trailing `/`), so an empty directory comes
  back as one. Format 1 carried files only.
- Entries are **sorted by device path**, the manifest is first, and entries
  carry no timestamps of their own: the same board state produces the same
  bytes twice.
- A reader **refuses** any entry outside `files/`, absolute, or containing
  `.`/`..` components.
- Compression is **deflate**.

## Manifest fields

`manifest.json`, camelCase:

| Field | Meaning |
|---|---|
| `formatVersion` | `2`. |
| `capturedAtEpochSeconds` | When the backup was taken, from the app's injected clock. |
| `deviceUid` | The uid in `/.lp/device.json` **in the captured files**, or absent for a board never named. |
| `chip` | What the bootloader named itself as during the read. |
| `baseMac` | The board's factory base MAC, read in the same session (v2). A restore refuses an archive whose `baseMac` is not the board's. |
| `partitionOffset` / `partitionLength` | Where the captured filesystem lived. |
| `targetPartitionOffset` / `targetPartitionLength` | Where a migration is putting it; absent for a plain backup (v2). |
| `blockSize` | littlefs block size (4096). |
| `fileCount` | Number of files (directories not counted). |
| `totalBytes` | Sum of the files' sizes — **not** the partition size. |
| `purpose` | `"backup"` or `"layout-migration"` (v2). |

Whether a migration that took a backup finished is **not** in the archive: a
pending/completed status lives in Studio's backup index
(`device-backups/index.json`), because an archive never changes after it is
written.

### Why `deviceUid` and `baseMac` are load-bearing

`/.lp/device.json` lives inside `lpfs`, so a board's identity is in every
backup and a naive restore writes it back. Restoring one board's backup onto
another would give two boards the same uid, and Studio's device registry keys
on it. `baseMac` is burned into the chip; a restore checks it.

### An archive holds the board's secrets

Every file in `lpfs` rides, so an archive carries the device store
(`/.lp/access.json`: the keys that unlock the board) and the network file
(`/.lp/network.json`: the Wi‑Fi password, in plaintext). A restore must
bring both back, so neither is filtered out. **Treat an archive like the
board itself**: keep it on your machine, don't share it. See
`docs/adr/2026-10-04-device-wifi-settings.md`.

## Where the code lives

| File | Job |
|---|---|
| `backup_manifest.rs` | The manifest type and its format version. |
| `backup_archive.rs` | Tree + manifest → ZIP bytes and back, path validation, the download file name. |
| `../lpfs_tree.rs` | Mount a raw image and walk it into a tree (one mount implementation, shared with the migration). |

The geometry (4 KB blocks, 512 B cache, 64 B lookahead) must match the
firmware's `lpfs_config()` (`../lpfs_geometry.rs` pins it).
