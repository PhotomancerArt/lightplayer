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

## `served.json` — what the site actually ships

`served.json` is not a build def. It is the **deployment** fact: which of
these builds the Studio site copies into its assets, and therefore which
boards the provisioning picker is allowed to offer.

```json
{ "format": 1, "builds": ["esp32c6-4mb", "esp32s3-8mb", "esp32v3-4mb"] }
```

It has a file rather than a constant because three readers need it and they
are in three languages:

- `lpa-boards` embeds it (`served_build_ids()`, `is_served()`) — the picker's
  eligibility filter and the candidate set for chip→build selection.
- the justfile packages and copies exactly these ids
  (`just studio-served-builds` prints them;
  `just studio-firmware-package-served` builds them).
- `scripts/pages/static-site-smoke.mjs` fails a Pages artifact that is
  missing any of their `firmware/<id>/manifest.json`.

A copy of this list in a second place is how the site came to offer a board
it could not flash. Adding an id means adding a build def, a
`studio-firmware-package-<chip>` recipe, and — for a new ISA — the toolchain
step in both deploy workflows.

## Distribution

`lp-cli firmware package <id>` writes
`target/studio-web-assets/firmware/<id>/` (merged image + `manifest.json`
schemaVersion 2). `served.json` decides which of those directories reach the
Studio site / Pages artifact.

## Consumers

These files are also read app-side, embedded by `lpa-boards`
(`BUILD_DEF_SOURCES`), for the **computed board↔firmware join**: a board runs
a build when `chip.name` equals the board's chip and the board's flash is at
least `flashSizeMb`. The boards catalog renders the result; the provisioning
picker will select through the same function when it lands (board-selection
roadmap M5). A drift test fails if this directory and `BUILD_DEF_SOURCES`
disagree, so adding a build def means adding its `include_str!` entry.

Feature lines shown for a build come from that package's
`manifest-core.expected.json` — the CI-verified extraction of the image's own
manifest — so **two build defs sharing a `package` must share
`cargoFeatures`** (a test enforces it); the day they need to differ, the
fixtures must go per build rather than per package.

