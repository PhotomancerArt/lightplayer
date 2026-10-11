# An `fs-tree` board refuses a store it will not mount, locked; and refuses a core that would format it

- Status: accepted
- Date: 2026-10-10
- Plan: `lp2025/2026-10-08-2339-tree-store-firmware-and-emulator` (M5 of
  `lp2025/2026-10-08-1017-tree-store-device-round`; D1, D2, D8)
- Related: `2026-10-02-c6-repartition-and-layout-migration` (`legacy_held`,
  the first "files kept, access locked" state),
  `2026-10-06-ota-update-protocol` (channel 3, never-break),
  `2026-07-14-wire-hello-versioning` (the bump)

## Context

`fs-tree` (non-default, never shipped) makes the C6's `lpfs` partition the
tree store (`lp-base/lp-tree-store`). Two failures have lasting
consequences for a board's files and its access:

1. **A store the mount will not adopt.** A newer format's header, a single
   leaked bit in a header's version field (which reads as newer), or a
   store with no complete root. Formatting would destroy files that
   `lp-cli hardware tree extract` can still read. Serving a RAM filesystem
   as `memory` is worse in a quieter way: the device store
   (`/.lp/access.json`) reads as missing, which is open by default
   (#929), so a board whose real access list is intact on flash would boot
   **open, with its passwords gone from view**. Serving it as `legacy_held`
   locks access correctly, but Studio and lp-cli then tell the user to run
   the layout migration, and a `migrate` over a store destroys it.
2. **A core on the other side of the line.** The C6 takes cores over the
   air and rolls back to its other core. A littlefs core finds no littlefs
   on `lpfs`, takes the tree store for an unformatted partition, and formats
   it; an `fs-tree` core does the same to littlefs.

Before either, the store could not tell blank flash from damage: every
mount without a complete root was `Corrupt("no complete root")`.

## Decision

- **The store classifies** (`lp-tree-store`'s `mount_verdict.rs`):
  `NoStore` (no trusted sector header of this format — blank, littlefs,
  foreign — or only the empty directory an interrupted first `format`
  writes), `Damaged` (a store's records and no complete root) and
  `Unsupported` (a newer format). No byte of the format moved.
- **An `fs-tree` board formats only `NoStore`**, and only when the
  legacy-layout probe holds nothing (then `legacy_held`, as today).
  Everything else the mount refuses — `Damaged`, `Unsupported`, a flash
  that would not read — is **refused**: nothing written, files kept, a RAM
  filesystem served, and the hello says `fs: refused`
  (`lpc_wire::FsBootState::Refused`, wire 42).
- **A refused board's access is locked**:
  `lpa_server::access_store::device_store_at_boot` returns
  `DeviceAccessFile::locked()` for `Refused` as for `LegacyHeld` (USB, a
  trusted link, still holds edit). Bluetooth stays off, and the network
  writes a held board refuses are refused.
- **Studio and lp-cli say it in words and offer nothing**: "This board's
  file store has a newer or damaged header; its files are kept — read them
  with `lp-cli hardware tree extract`." No migration, no restore: there is
  no verb a click could safely press here. The core's view publishes the
  line; the web builds no action.
- **The `fs-tree` build's update session refuses a core install whose build
  lacks `fs-tree`** (`fw_esp32_common::fs_tree_core_guard`). Before the boot
  record that would boot the new core, the board reads that core's own
  embedded manifest core off flash and looks for the feature `fs.tree`
  (`LpFeature::FsTree`, which an `fs-tree` build declares in its manifest
  core and its hello). Absent — or no readable manifest — it logs
  `[OTA] core install refused: this board holds a tree-store filesystem;
  that core would format it …` and fails the install as the target
  refusing it (`FlashFault`). Channel 3's bytes do not change.

## Consequences

- A refused board is recoverable by reading its flash; nothing on it
  changes until someone decides.
- The guard covers one direction only: an `fs-tree` board will not take a
  littlefs core. A **littlefs** board would still format a tree store it is
  handed — the product image's half of the guard (hold on tree-store
  magic) is the adoption round's, with the migration it needs. Until then
  `fs-tree` images are never served (`served.json`, a release,
  `bless-chips`), and CX1 and emulated boards are flashed over USB.
- The guard refuses with the target's generic refusal: the host sees the
  install stop, and the board's log says why. A protocol-level reason is
  a channel-3 addition, left for the adoption round if it is wanted.
- `refused` is a new value of a required hello field and `fs.tree` a new
  feature: wire 42. A littlefs build never produces either.
