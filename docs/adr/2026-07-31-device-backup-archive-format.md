# ADR: Device backup archive format

- **Status:** Accepted
- **Date:** 2026-07-31
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

M6 of the device-recovery plan gives Studio a device backup: the raw `lpfs`
partition read off the board over the bootloader — deliberately workable on a
device that cannot boot — mounted in wasm by the same littlefs implementation
the firmware uses, and handed to the user as a ZIP.

The moment M7 (restore) reads these archives back, the layout stops being an
implementation detail and becomes a contract: an archive written today must
restore correctly tomorrow, and a future selective restore ("put just this
one project back") must be able to recover device paths without reversing a
scheme. That crossing — from artifact to contract — is what warrants an ADR.
The full field-level reference lives with the code
(`lp-app/lpa-studio-core/src/app/device/filesystem_backup/README.md`); this
records the decisions and their reasons.

## Decision

**Posture: support-facing, shaped as if public.** The format is not promised
to users or documented outside the repo, but it is designed as though it
might be — that is what keeps the promotion path free. This mirrors the
product's alpha wire posture rather than fighting it.

**Versioning: version and refuse, never migrate** — the same alpha rule as
share envelopes (`2026-07-28-share-envelopes.md`). `manifest.json` carries
`formatVersion`; a reader meeting an unknown version says so and stops. No
dual-format decode paths, no shims.

**Layout: device paths mirrored verbatim under one `files/` root.**

```
manifest.json
files/.lp/device.json
files/projects/porch/project.json
```

- Nothing flattened, renamed, or restructured: recovering a device path is
  `strip_prefix("files/")`, not the reversal of a scheme. This is the
  property selective restore depends on.
- The `files/` prefix exists for exactly one reason: `manifest.json` cannot
  collide with a file the device kept at its filesystem root.
- Entries sorted by device path, manifest written first: the same device
  state produces byte-stable archives, and a streaming reader learns what it
  is holding before the content.
- Deflate compression; the content is small and mostly text.

**The manifest carries what restore and forensics need**: `formatVersion`,
capture time, the `deviceUid` found *in the captured image* (absent for a
never-named board), the chip the bootloader named itself as, the partition
offset/length and littlefs block size the image was read at, and file
count/total bytes. The chip and partition fields are what let M8 refuse
cross-chip restores and let M7 detect a cross-device restore before it
happens.

## Consequences

- M7 restore and future selective restore have a stable substrate; promoting
  the format to user-facing later costs documentation, not redesign.
- The uid living inside the archive means restore MUST implement the
  identity guard (preserve the target's own `/.lp/device.json`) — the
  archive faithfully carries the hazard, and the reader owns the safety.
- Byte-stable output makes archives diffable and testable by fixture.
- A `formatVersion` bump strands older Studio builds by design; acceptable
  under the alpha posture, revisit when devices ship.

## Alternatives Considered

- **Raw partition image instead of a ZIP.** Rejected as the user-facing
  artifact: opaque, unbrowsable, ties the backup to one partition geometry
  (the C6 and S3 already differ). Whole-flash imaging remains M8's separate,
  explicitly image-shaped feature.
- **A flattened or manifest-indexed layout.** Rejected: any scheme that must
  be reversed to recover a device path is a scheme somebody will reverse
  wrongly.
- **Migrating old archives on read.** Rejected by the standing alpha rule:
  version and refuse.

## Follow-ups

- M7 (restore) reads this format and owns the identity guard and
  verify-before-write.
- If the format is promoted to user-facing, document it outside the repo and
  add fixture archives per version to `schemas/`-style history.
