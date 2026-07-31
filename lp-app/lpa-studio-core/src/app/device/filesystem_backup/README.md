# Filesystem backup archive

What Studio hands the user when they back up a device's storage: a ZIP built
from the raw `lpfs` partition read off the board over the bootloader
(`LinkManagementRequest::ReadRawFilesystem`).

This works on a device that **cannot boot** — which is the whole point. The
originating failure was a project bright enough to brown its own board out at
power-up, looping forever. Nothing on such a device can be asked to list its
own files, so the partition comes off as bytes and is mounted here, in wasm,
by the same littlefs implementation the firmware uses.

## Posture

**Support-facing, but shaped as if it were public.** We do not promise this
format to users and we do not document it outside the repo. We *do* design it
as though we might, because that is what keeps the upgrade path free: M7
(restore) and any future selective restore ("put just this one project back")
read these archives, and a layout invented for convenience today is a layout
somebody has to reverse tomorrow.

Alpha versioning rule, same as share envelopes
(`docs/adr/2026-07-28-share-envelopes.md`): **version and refuse, never
migrate.** A reader that meets an unknown `formatVersion` says so.

## Layout

```
manifest.json                              ← archive root, written first
files/.lp/device.json                      ← device paths, mirrored verbatim
files/lightplayer.json
files/projects/porch/project.json
files/projects/porch/shader.glsl
```

- **Device paths are mirrored verbatim** under the single `files/` root.
  Nothing is flattened, renamed, or reordered into a different hierarchy.
  Recovering a device path is `entry.strip_prefix("files/")` and prepending
  `/` — not the reversal of a scheme.
- The `files/` prefix exists for exactly one reason: so `manifest.json` cannot
  collide with a file the device happened to keep at its filesystem root.
- Entries are **sorted by device path**, and the manifest is written first, so
  the same device state produces the same archive twice and a streaming reader
  learns what it is holding before a megabyte of content.
- Compression is **deflate**. The content is small and mostly text.

## Manifest fields

`manifest.json`, camelCase:

| Field | Meaning |
|---|---|
| `formatVersion` | `1`. Bumped when a reader must notice a change. |
| `capturedAtEpochSeconds` | When the backup was taken, from the app's injected clock. |
| `deviceUid` | The uid found at `/.lp/device.json` **in the captured image**, or absent for a board that was never named. |
| `chip` | What the bootloader named itself as during the read (`esp32c6`, `ESP32-C6 (QFN32) …`). |
| `partitionOffset` / `partitionLength` | Where the captured partition lives on that chip. |
| `blockSize` | littlefs block size the image was read at (4096). |
| `fileCount` | Number of files captured. |
| `totalBytes` | Sum of the captured files' sizes — **not** the partition size. |

### Why `deviceUid` is load-bearing

`/.lp/device.json` lives inside `lpfs`, so a device's identity is captured in
every backup and would be written back by a naive restore. Restoring one
board's backup onto another would give two boards the same uid, and Studio's
whole device registry keys on it.

Recording the captured uid in the manifest is what lets a restore **detect**
that case rather than silently perform it. M7 owns the decision about what to
do then (preserve the target's own stamp is the plan); this milestone's job is
to make sure the fact is not lost.

## What is NOT here

- **Restore.** M7. `lpa-link` advertises the raw-filesystem READ only, so no
  UI can offer a write that every provider answers with `unsupported`.
- **Whole-flash images.** M8. This is the filesystem partition, not the chip.
- **Selective restore.** A natural follow-on, and the reason the layout mirrors
  device paths instead of inventing its own.

## Where the code lives

| File | Job |
|---|---|
| `backup_image.rs` | Mount the raw image read-only, walk it into `BackupFile`s. Fails loudly on a damaged filesystem. |
| `backup_manifest.rs` | The manifest type and its format version. |
| `backup_archive.rs` | Manifest + entries → ZIP bytes, and the download file name. Holds the fixture-image tests. |

The geometry (4 KB blocks, 512 B cache, 64 B lookahead) must match
`lp-fw/fw-esp32c6/src/flash_storage.rs`'s `lpfs_config()` or the mount fails
or misreads. Block *count* is derived from the image length rather than
pinned, because it differs per board: 240 blocks on the C6, 384 on the S3.
