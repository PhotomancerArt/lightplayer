# Firmware build definitions

A **build def** is the checked-in, machine-readable description of one
shippable firmware variant: which crate, which cargo target/profile/features,
which flash size and partition table. It is a *build input* — the answer to
"how do I produce this image" — and deliberately **not** a description of what
the resulting image contains. That answer is extracted from the artifact
itself (the embedded manifest core, see
`docs/adr/2026-08-01-firmware-manifest-architecture.md`); nothing here restates
feature lists, wire proto, or limits.

`lp-cli firmware list | build <id> | package <id>` reads these files. They
replaced the flash-size/feature/profile strings that used to live only in
justfile recipes.

## Fields

| Field | Meaning |
|---|---|
| `format` | Schema version of this file. Only `1` is accepted — version + refuse, no dual decode (alpha posture). |
| `id` | Variant id. Also the packaged output directory: `firmware/<id>/`. Convention: `<chip>-<flash>` (`esp32c6-4mb`). |
| `displayName` | Human label carried into the distribution manifest. |
| `package` | Cargo package. The crate directory is resolved as `lp-fw/<package>`. |
| `cargoTarget` | Rust target triple. |
| `profile` | Cargo profile. |
| `cargoFeatures` | Cargo features **added to the crate defaults** (`--features`, no `--no-default-features`). These are cargo features, not `LpFeature`s. |
| `flashSizeMb` | Physical flash the image header declares. Must match `partitionsCsv` — the bootloader validates the table against the header, not the chip. |
| `partitionsCsv` | Repo-relative partition table, the same file espflash flashes with. |
| `chip.family` / `chip.name` | espflash chip identity (`--chip`). |

## Authoring rules

- Keep it minimal. If a fact is discoverable from the built artifact, it does
  not belong here.
- `id` is API: it names the served directory and, from M5 on, the picker
  entry. Do not rename a shipped id.
- Changing `flashSizeMb` without changing `partitionsCsv` (or vice versa) is a
  boot-loop; they are one decision.
- Xtensa builds need Espressif's fork on PATH. `lp-cli` runs cargo in the
  crate directory so the crate's `rust-toolchain.toml` selects the channel,
  but the GNU binutils must already be on PATH — `just
  studio-firmware-package-esp32s3` prepends them via `just _xt-gcc-dir`.
  Invoking `lp-cli firmware build esp32s3-8mb` bare-handed does not.

## Distribution

`lp-cli firmware package <id>` writes
`target/studio-web-assets/firmware/<id>/` (merged image + `manifest.json`
schemaVersion 2). Only **esp32c6-4mb** is copied into the Studio site /
Pages artifact today; `esp32s3-8mb` packages correctly but nothing serves it
yet (the provisioning picker gains per-build selection in roadmap M5).

