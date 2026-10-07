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
| `bootloader` | Optional. Repo-relative second-stage bootloader to merge instead of the one the installed espflash bundles (`--bootloader`). Provenance and the rule for changing it: `lp-fw/bootloaders/README.md`. |
| `split` | Optional, ESP32-C6 only. `true` builds the **split image** (`tools/lp-fw-split`): the loader, the boot records, the core and the engine inside `factory` (`docs/adr/2026-10-04-c6-split-link-firmware-loader-and-boot-records.md`). The package is still one merged image at `0x0`, plus a `split` block in `manifest.json`; `core.bin` and `engine.bin` go to `target/firmware-parts/<id>/`, never into the package. Absent: one linked image. |

## The target

A build def's `id` is a **target**: the name of a line of builds
(`esp32c6-4mb`). Every image built from a def embeds it — `lp-cli firmware
build <id>` hands it to the build as `LP_FW_TARGET`, and the image's manifest
core carries it as `target` (a plain `cargo build`, with no def behind it,
says `unknown`). The rules:

- **A target is an opaque name.** No code parses a chip or a flash size out
  of it; those are the def's fields and the manifest core's `platform`. (It
  is already untrue that the name spells the chip: `esp32v3-4mb`'s chip is
  `esp32`.)
- The convention is `<chip>-<flash>[-<variant>]`, matching
  `[a-z0-9][a-z0-9-]{0,63}`; the default variant has no suffix.
- **A target is never renamed.** Boards report it and releases are filed
  under it.
- A release channel, a version, a board's wiring and runtime settings are
  **not** part of a target.

Three names, never confused: the **target** (`esp32c6-4mb`), the
**version** (`2026.10.05-3`), and the **build id** (`<version>+<commit>`).

## Authoring rules

- Keep it minimal. If a fact is discoverable from the built artifact, it does
  not belong here.
- `id` is API: it names the served directory and, from M5 on, the picker
  entry. Do not rename a shipped id.
- Changing `flashSizeMb` without changing `partitionsCsv` (or vice versa) is a
  boot-loop; they are one decision.
- The packager refuses an image whose **bootloader segments** are not the
  ones `lpa_devices::bootloader::bootloader_code_ranges` names for the chip.
  Studio recognizes a board hung in its bootloader by the ROM's `Saved PC`
  against that table, so a bootloader change (an espflash upgrade, a
  `--bootloader` override) must update the table — and the copy in
  `scripts/c6-lp-ana-i2c.py` — from the new bootloader's image header. See
  `docs/defects/2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md`.
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
  `just studio-firmware-package-served` builds them), and so does every
  release (`scripts/release/release-firmware.sh` attaches each one's
  package; `fetch-release-firmware.sh` takes them back for the deploys).
- `scripts/pages/static-site-smoke.mjs` fails a Pages artifact that is
  missing any of their `firmware/<id>/manifest.json`.

A copy of this list in a second place is how the site came to offer a board
it could not flash. Adding an id means adding a build def, a
`studio-firmware-package-<chip>` recipe (and its arm in
`studio-firmware-package-target`), and — for a new ISA — the toolchain step in
`release-firmware.yml` (which builds every release's firmware) and in
`deploy-pages-channel.yml` (whose untagged betas build their own).

## Distribution

`lp-cli firmware package <id>` writes
`target/studio-web-assets/firmware/<id>/` (merged image + `manifest.json`
schemaVersion 2; a split def adds the manifest's `split` block and writes its
parts to `target/firmware-parts/<id>/`). `served.json` decides which of those directories reach the
Studio site / Pages artifact.

For a split def, `package` also writes the **OTA files** into the parts
directory (never the Studio bundle): `ota-manifest.json` (format 1,
`schemas/ota-manifest.schema.json`, `lpc-firmware-release`) and `core.z` /
`engine.z` (encoding 1, compressed by `lpa-update`'s one packer). Identity is
read from the image's manifest core; the full commit is resolved from the
checkout, and a release version whose commit is `unknown` refuses (a dev
build skips its OTA files with one warning).

`lp-cli emu run --host-link --ota-offer <dir>` and `lp-cli link capture
<port> --ota-offer <dir>` offer the build in such a parts directory over a
board's update channel: they read its `ota-manifest.json` and the files it
names (`lpa_update::HostBuild::from_ota_manifest`), never the package's
`split` block. A release staging directory (files named `<target>.<file>`)
works too.

**Which builds take over-the-air updates.** Every image says so in its
manifest core: a split C6 image carries `"ota": {"layout": 1}` (the update
layout it supports, `lpc-update`'s code table); a single image carries no
`ota` key and only USB updates it. A def with `"split": true` packages the
split image. **A plain local build stays a single image**: `cargo build` in
`lp-fw/fw-esp32c6`, `just build-fw-esp32c6` and `just flash-fw-esp32c6` link
one image (no loader, no boot records, no `ota`), the faster build for
everyday firmware work, and the hello of a board running one carries no
`firmware` block (a host reads it as "connect over USB to update"). Both
kinds stay supported.

**The local Studio packages a single image by default.** `lp-cli firmware
package <id> --single-image` (and `firmware build … --single-image`) builds a
split def as one linked image, with no update files; the dev Studio recipes
(`just studio-dev`, `studio-dev-emu`, `studio-web-dev-build`, and
`studio-firmware-package-*` called bare) pass it, because a local firmware
change should not cost the split image's second link pass. Measured warm on
an M2 Max (2026-10-06, one source file touched, the rest cached): **single
image 23 s, split image 50 s** (pass 1 link 23 s + pass 2 link 25 s, then the
packer). To test updates, ask for the split image: `LP_FW_IMAGE=split just
studio-dev` (or `just studio-firmware-package-served split`). The release
bundle (`just studio-web-build`, which every deploy runs) is **always** split,
and the Pages artifact refuses to stage without the update files.

**What the Studio bundle carries** (OTA M7, DS10): for a split package,
`firmware/<id>/` also holds its `ota-manifest.json`, `core.z` and `engine.z`,
beside `manifest.json` (two segments under `firmware/`: lightplayer.app's
firmware lookup owns every three-segment `/firmware/` path) — never `core.bin`/`engine.bin`, which Studio slices out of the merged image
by the package manifest's `split` offsets. For the C6 that is about 1.8 MB
(`core.z` 741,552 + `engine.z` 1,084,808 + the manifest 13,255 bytes at
`e6775ad53`), fetched only when an update runs.
`scripts/studio-copy-firmware.sh` copies them, and refuses update files that
describe another package (`ota-manifest.json`'s `package` entry must hash the
copied `manifest.json`). Studio believes it holds an update-capable build only
when that check passes **and** the image's manifest core says `"ota":
{"layout": 1}` — otherwise it has no build of its own, offers no over-the-air
update, and over USB keeps today's flash.

`lp-cli firmware release-assets --out <dir> [--targets <id,…>] [--allow-dev]`
stages those packages under release asset names (`<target>.<file>`), verifying
every file and compressing nothing; `lp-cli firmware release-check <dir>
[--version <v>]` re-verifies a staged (or downloaded) release from its files
alone, including every compressed chunk. Both refuse targets packaged at
different versions (a release is one build), and `--version` requires the
release's own. Neither uploads anything.

**Releases.** Every main merge's GitHub release carries every served
target's package (`<target>.package.json` = its `manifest.json`, and
`<target>.<image>`) and, for a split target, its update files
(`<target>.ota-manifest.json`, `.core.bin`, `.engine.bin`, `.core.z`,
`.engine.z`). `.github/workflows/release-firmware.yml` attaches them, running
`scripts/release/release-firmware.sh <version>`: package each target once
(`just studio-firmware-package-target <id> split`), stage, `release-check`,
upload — immutably: an asset already there with the same SHA-256 is skipped,
a different one fails the run, nothing is ever replaced — then mark GitHub's
Latest as the newest release that carries firmware. `--dry-run` prints the
upload instead (the pre-merge `release-dry-run` job runs it for the C6 and
keeps the staging directory as an artifact). lightplayer.app serves these
assets at `/firmware/<target>/<release>/<file>`
(`lp-cloud/lp-cloud-server/README.md`), and lists every release a target can
install, newest first, at `/firmware/<target>/releases` (the release index,
format 1, `schemas/firmware-release-index.schema.json`); `just
firmware-store-smoke` proves both locally.

**Putting a published release on a board.**
`lp-cli firmware install --release <version|previous|latest> (--mac <MAC> |
--port <PORT>) [--target <id>] [--yes]` resolves the release (`latest` and a
version against lightplayer.app's lookup directly; `previous` — the newest
published release older than latest that carries the target — against the
GitHub releases list, `gh` when it is on `PATH` else the public REST API),
downloads `<target>.package.json` and its merged image from that same lookup,
verifies every file's length and SHA-256 against the manifest before writing
anything, leases the board on the desk's board bench when `board` is
installed, and writes it with the same layout-aware flasher
`lp-cli hardware lpfs migrate` uses: a board whose filesystem layout already
matches gets the plain write (the ordinary case), one that does not gets a
backed-up migration. `--dry-run` resolves, downloads and verifies without
touching the board at all — the way to check a release exists and hashes
clean before committing a desk sitting to it.

**The deploys ship the release's firmware, not their own build.**
`just studio-web-build` fills the bundle's firmware after dx through `just
studio-web-firmware`: `LP_STUDIO_FIRMWARE` unset (every local recipe)
packages it here; `release:<version>` takes release v`<version>`'s
(`scripts/release/fetch-release-firmware.sh`, verified, into the same
directories `package` writes), which `deploy-cloud.yml` sets — waiting for
its commit's "Release firmware" run — so the engine a bundle flashes is one
the store serves. A release whose firmware fails does not deploy.

**The published `package.json` is additive-only** from its first release:
readers ignore unknown keys; a flasher may refuse another `schemaVersion`,
never an unknown key. See `docs/adr/2026-10-06-firmware-distribution.md`.

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

