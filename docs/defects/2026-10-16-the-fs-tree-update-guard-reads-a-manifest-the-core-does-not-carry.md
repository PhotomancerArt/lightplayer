---
status: fixed
found: 2026-10-16      # how: packaging the cutover's fs-tree split image (A6) refused "not a ManifestCore"
fixed: this change     # PR #1167 (A7, the default switch; held for the cutover)
area: fw-esp32-common `fs_tree_core_guard` × lp-fw-split's core/engine placement
class: contract-gap
related:
  - docs/adr/2026-10-10-fs-tree-refused-state-and-update-guard.md (D8, the guard)
  - docs/adr/2026-10-04-c6-split-link-firmware-loader-and-boot-records.md (what goes in the core)
  - docs/adr/2026-10-16-c6-tree-store-cutover-migration.md
---
# The fs-tree update guard looks for a manifest the core does not carry

**Symptom** — `lp-cli firmware package esp32c6-4mb --fs-tree` failed: "manifest
payload in …/fw-esp32c6-merged.bin is not a ManifestCore". The cutover's
`lpfs migrate --to tree` read the same image as a littlefs one. Both took
the first manifest begin marker in the image, and in an `fs-tree` image that
marker is not the blob.

**Root cause** — the `fs-tree` core's update guard (D8) holds the markers as
string constants, so the core carries a begin marker followed at once by the
end marker. The real manifest blob (`lp_embed_manifest_core!`) is linked into
the **engine**: in the cutover's build, `core.bin` holds one begin marker (the
guard's constant at `0x1212c`), and `engine.bin` holds the blob (`0x5244`).
`core_store_verdict` scans only the new core
(`ota/update_target_impl.rs`, `write_trial_record`). So every `fs-tree` core
reads as `NoManifest` and its install is refused — an `fs-tree` board cannot
take any core over the air, including another `fs-tree` one. No test caught
it: the guard's tests build synthetic images, and no emulated OTA runs an
`fs-tree` image.

**Fixed so far** — the stray marker: main's `48e158aee` (#1150, A5) stores
the guard's markers masked, so no image carries a contiguous copy outside
its real blob, and A6 makes `lpc_model::manifest::find_manifest_core` take
the first begin marker followed by a JSON object as well. The packager and
`image_fs_kind` find the engine's blob in a merged image. **The guard still
reads only the core, where there is no blob.**

**Owed (A7, the default switch)** — the guard must read what the new core
will mount from something the core itself carries (a feature word beside the
build id, say), or check the engine once it is written; and an emulated OTA
of one `fs-tree` image onto another must pass before the default switches.

**Fixed (A7, the default switch, PR #1167)** — the core says what it mounts
in its own bytes. A core that mounts the tree store links a 20-byte mark,
`lpc_model::manifest::CORE_FS_TREE_MARK` (`"\u{1}LP-CORE-FS-TREE-v1\u{2}"`),
as the static `LP_CORE_FS_TREE_MARK` in `fw_esp32_common::tree_fs`, named by
the mount itself (`black_box`) so only a build that mounts the store links
it. `lp-fw-split` makes the symbol a core root (like the seam table and the
digest slot), so the split link always places it in `core.bin`, and fails
the build when the manifest lists `fs.tree` but `core.bin` carries no mark,
or the other way round (`check_core_tree_mark`; every C6 build prints which
it is). The guard (`core_store_verdict`) now scans the core for the mark —
held masked, so the guard's own copy is never a match and a littlefs core
that links the guard (A5's holding core) does not read as a tree core — and
never reads the manifest. Verdicts: `KeepsTheStore`, `WouldFormat` (no mark:
every littlefs core, and every tree-store dev core built before the mark),
`Unreadable` (a read failed).

Regression coverage: `fs_tree_core_guard` tests (a core whose manifest says
`fs.tree` but whose bytes carry no mark is refused — this defect's shape;
the masked copy and a cut-short mark are not matches; every read boundary),
`lp-fw-split`'s `a_tree_manifest_needs_the_mark_in_the_core_and_only_then`,
`tree_fs::the_core_mark_is_the_models`, and the emulated OTA of one
tree-store image onto another (`just test-emu-c6-ota`, U1 X→Y on the default
build since the switch).
