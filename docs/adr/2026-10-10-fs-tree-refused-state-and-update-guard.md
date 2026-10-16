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
  (`lpc_wire::FsBootState::Refused`, wire 43).
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
  feature: wire 43. A littlefs build never produces either.

## Amendment (2026-10-16): the littlefs half — a littlefs core holds a tree store

Plan `lp2025/2026-10-16-0848-tree-store-adoption`, A5 (decision AD4). The
Consequences above left one direction open: "a **littlefs** board would
still format a tree store it is handed". A littlefs core reaches a tree
store three ways once boards are migrated: the loader rolls back to it when
a tree core's trial fails (`lp_bootctl::choose` boots the other record), an
older image is flashed over USB, or Studio's "Other version…" installs one.
Each formatted the store. This closes it, in the image that ships today,
before any tree core is released (AD4).

- **The firmware.** When littlefs will not mount the C6's `lpfs`, the
  littlefs build looks at the first four bytes of every sector for the tree
  store's sector magic `"LTS1"` (FORMAT.md, "Sector": bytes 0..4 of every
  header in every version) before anything formats
  (`fw_esp32_common::lp_fs::holds_tree_store`). Every sector, because the
  store levels its wear and its first sectors may be free; only the magic,
  never a CRC or a version, because a newer store must be held too. A sector
  that will not read counts as one. The magic is spelled in the littlefs
  build, not imported: it links no tree store (a test pins it to
  `lp_tree_store::SECTOR_MAGIC`). Read-only, four bytes a sector, only on a
  failed mount.
- **What it does: the existing `refused`.** Nothing written, files kept, a
  RAM filesystem served, `fs: refused` in the hello, the device store locked
  (USB, a trusted link, still holds edit), Bluetooth off, network writes
  refused — exactly the tree core's refusal above. The boot word is
  `[FS] this board's files are in the tree store, a newer filesystem than
  this firmware reads — not formatting; files kept; using memory FS, access
  locked. Update the firmware.` The tree check comes before the legacy-layout
  probe: a tree store is never reported `legacy_held`, whose migration would
  write over it.
- **Over the air, the same guard.** While a littlefs core holds a tree
  store, its update session refuses a core install whose embedded manifest
  lacks `fs.tree`, exactly as an `fs-tree` board's does
  (`fs_tree_core_guard`, now in every server build;
  `ota::cores_must_read_tree_store`): an older littlefs core picked with
  "Other version…" would take the store for blank flash and format it. A
  core that reads the store is the way forward, and installs.
- **No wire change.** `FsBootState::Refused` (wire 43) already meant
  "files kept, access locked, nothing to press". Which refusal it is follows
  from a fact the hello already carries: a build with `fs.tree` refused its
  own store (newer or damaged header); one without is a littlefs core on a
  tree store. The server, lp-cli and Studio choose their words from that
  (`lpa_server::network_store::refused_store_refusal`,
  `lp-cli … refused_store_words`, `BoardFs::TreeStoreHeld` — a host-model
  value, not a wire one): "the board's files are in the tree store, a newer
  filesystem than its firmware reads; they are kept — update the firmware".
  Studio's card says **Files need newer firmware** and offers nothing that
  moves files. `WIRE_PROTO_VERSION` and `PACK_FORMAT_VERSION` are unchanged;
  no persisted byte changed.
- **The host half: the USB flash path never writes over a tree store.** The
  layout probe (`lpa_link::layout_migration`) recognises one
  (`LayoutState::TreeStore`): from the 8 KB superblock read it already makes
  when the store's first sectors are in use, and otherwise from the rest of
  the target `lpfs`, which it now reads before it says "no filesystem" (a
  tree store with free first sectors used to classify `CurrentNothing`, and
  a pending backup would have been restored over it as littlefs). A tree
  store on the frozen legacy table, or one at the target with a stale
  legacy superblock left at `0x310000`, is a tree store too. On it, only a
  firmware whose embedded manifest core lists `fs.tree` is written, as a
  plain flash on the layout it is on; a littlefs image is refused
  (`Refusal::TreeStoreNotRead`), and a restore or a layout change is
  refused (`Refusal::TreeStoreWouldBeWritten`) — nothing written, in words,
  in lp-cli (`firmware install`, `hardware lpfs migrate|restore`) and in
  Studio's flash step. Providers hand the package's manifest over
  (`LinkLayoutInspection::target_manifest`: host from the package's images,
  the browser from the same images esptool-js loads); the decision is the
  shared pure code's.
- **What this does not cover.** Table-only preflights (`fwcheck`,
  `validate run`, `just flash-fw-esp32c6`) still read only the partition
  table: they flash this tree's image, which holds. An image from before
  this change, flashed by a path that skips the layout probe (plain
  espflash), still formats a tree store — which is why AD4 ships this first
  and why the migration (A6) leaves no older littlefs core in the other
  boot record.
- **Evidence.** Host: `fw-esp32-common` `lp_fs` tests (a real store found at
  the front and with its first sectors free; a refusal writes nothing),
  `lpa-link` `layout_state` / `migration_plan` / `tree_store_magic` /
  `firmware_reads_tree_store`, Studio's `device_layout_step` and the board
  card's e2e test, `lpa-server`'s `network_requests`. Emulated
  (`lp-emu:esp32c6:t1+net=lan`, never hardware-validated):
  `lp-cli/tests/emu_littlefs_on_tree_store.rs` — the shipped image (direct
  and split, ROM-up) on a tree store holds, says so, keeps answering, and
  leaves `lpfs` byte-identical across a power cycle; the USB probe refuses
  the shipped image and takes the `fs-tree` one by their own manifests — and
  `emu_split_scenarios.rs` S11, the rollback itself.
