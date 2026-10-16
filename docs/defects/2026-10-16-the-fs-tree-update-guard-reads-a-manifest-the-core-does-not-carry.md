---
status: open
found: 2026-10-16      # how: packaging the cutover's fs-tree split image (A6) refused "not a ManifestCore"
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

**Fixed so far (A6)** — `lpc_model::manifest::find_manifest_core` and the
guard both take the first begin marker followed by a JSON object, so the
packager and `image_fs_kind` find the engine's blob in a merged image. The
guard still reads only the core, where there is no blob.

**Owed (A7, the default switch)** — the guard must read what the new core
will mount from something the core itself carries (a feature word beside the
build id, say), or check the engine once it is written; and an emulated OTA
of one `fs-tree` image onto another must pass before the default switches.
