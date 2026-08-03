# The flashed image is selected from the discovered chip, and refused when it disagrees

- Status: accepted
- Date: 2026-08-02
- Plan: `2026-08-02-1706-provisioning-firmware-build-selection`
- Extends: `2026-08-01-firmware-manifest-architecture.md` (the three-store join)
- Defect: `docs/defects/2026-08-02-provisioning-flashes-one-image-unchecked.md`

## Context

Studio provisioning flashed whatever image the provider was constructed
with. That was correct while one image existed and became a silent
wrong-image bug the moment a second shipped: an S3 or a classic ESP32 took
the C6 build, reported success, and did not boot. The chip was known —
both providers run an esptool/espflash SYNC handshake before writing — and
was simply never compared to the image.

Three questions had to be answered together, because answering any one
alone leaves the failure intact: *which image should this device get*,
*who decides*, and *what happens when the answer turns out to be wrong at
the moment of writing*.

## Decision

### The chip is the necessary condition; the board refines it

`lpa_boards::provisioning_build_id(board, chip)` picks the build. Chip
first, because a different ISA cannot execute the image at all — flash
size, feature set and board identity are all refinements of a question
that chip identity has already settled. A picked board chooses among the
several served builds that run on that chip (today never more than one;
flash-size variants are exactly what build defs exist to express). With no
board picked — the generic-install path, and the common one — the chip
alone still resolves, preferring the **smallest** declared flash, since an
unidentified board's flash size is unknown and the smallest image is the
one most likely to fit.

### The chip is discovered, not declared

Chip identity comes from the ROM boot banner or the bootloader handshake,
never from configuration. This is the same argument
`LinkFlashRegion::lpfs_for_chip` already makes for partition offsets, and
it is what makes provisioning work on a board nobody has identified yet.

Consequence: reported chip names arrive in several spellings for the same
silicon, and **neither equality nor a substring test distinguishes them**.
Read out of esptool-js 0.6.0's shipped per-target `getChipDescription`
(2026-08-02), what `main()` actually returns is `ESP32-C6 (revision 0)`,
`ESP32-S3 (QFN56) (revision v0.2)`, and — for the classic — a *die* name
such as `ESP32-D0WDQ6 (revision v1.0)`. None equals the bare id a manifest
carries, so equality refuses every real device; and all of them contain
`esp32`, so a substring rule accepts a classic image on a C6. Both
mistakes were made in this codebase before this ADR: the first in the
guard as originally merged (#292), the second in
`LinkFlashRegion::lpfs_for_chip`, which was safe only because `esp32` had
no row in its table.

`lpa_link::chip_id_from_reported` resolves a reported name against an
ordered id table, most specific first, with bare `esp32` last. The
ordering is the correctness argument, and it is enforced by a test over
the whole table rather than by a comment, because two pairs already
collide (`esp32c61`/`esp32c6`, and `esp32` with everything). Every
comparison — the flash guard, the `lpfs` region table, build selection,
the picker's board-family matching — goes through it. The browser guard
runs in JS, so the table crosses that boundary **as data from Rust**
rather than as a second copy: the JS half has no test harness in this
repo, so anything duplicated there is untested by construction.

### A board pick that contradicts the chip is honoured, and refused

Provisioning does two things with the board: it flashes the board's build
*and* stamps that board's runtime manifest into `/hardware.json`. Quietly
substituting the detected chip's image would produce a booting device
carrying another board's pin map — a wrong device that looks right.
Refusing, by contrast, produces an error naming both chips and telling the
user which control to change. So the pick wins, and the guard is what
catches it.

### The guard sits at the handshake, in every provider

Between "the chip answered" and "the first byte is written", both
providers compare the reported chip to the image's `core.target.chip` and
refuse by name. The host additionally declares the manifest's chip to
espflash, so its handshake fails too; the explicit check exists to make
the message actionable rather than an espflash `ChipMismatch`.

`None` — no board, no chip — is a legal request. It leaves the provider on
its deployment default and lets the guard decide. Nothing in this path
guesses an image.

### What the deployment serves is one file

`lp-fw/builds/served.json`, read by `lpa-boards` (the picker's eligibility
filter and the selection candidate set), by the justfile recipes that
package and copy `firmware/<id>/`, and by the Pages smoke check. Three
readers, three languages, one fact. The previous arrangement — a Rust
`&[&str]` and a justfile variable, unchecked against each other and
against the build defs — is how the site came to offer a board it could
not flash.

## Alternatives considered

**Keep one image per deployment and add only the guard.** Cheapest, and it
converts a silent wrong flash into an honest refusal — but it leaves every
non-C6 user with a refusal and no path forward, which is not a fix.

**Let the chip always win over the board pick.** Fewer refusals, but it
writes an image the user did not ask for while recording metadata for the
board they did: a booting device with the wrong pin map, and a class of
support question with no visible cause. Rejected for the same reason
`lpfs_for_chip` returns `None` rather than guessing.

**Resolve the build inside the provider, after the handshake.** The most
robust ordering — the chip would be known before the manifest is chosen —
but it puts catalog policy (`served.json`, board compatibility, flash-fit
ranking) inside the transport layer, and the provider would have to
re-derive what the app already knows. The guard recovers nearly all of the
robustness at none of the layering cost.

**Serve only the images CI can build without extra toolchains.** Would
have kept the Pages deploys as they are, but the S3 and classic builds
need Espressif's Rust fork, and not serving them is exactly the status quo
being fixed. Both deploy workflows take the toolchain step instead.

## Consequences

- Adding a chip is now a mechanical list: a build def, a
  `studio-firmware-package-<chip>` recipe, an entry in `served.json`, an
  id in `KNOWN_CHIP_IDS`, and — for a new ISA — a toolchain step in both
  deploy workflows. Drift tests and the Pages smoke check fail on a
  half-done addition.
- Production deploys now build three firmware images, two of them Xtensa.
  Deploy time grows; the alternative was a picker offering boards the site
  cannot flash.
- `just studio-dev` requires the Xtensa toolchain. Deliberate: a dev server
  that silently omitted an image would offer that board and 404 at flash
  time, and the hardware walk runs against `studio-dev`.
- `LinkManagementRequest::FlashFirmware` grew a payload and the
  payload-less form is gone. Alpha wire posture — no alias, no shim.
- The advisory "firmware update available" path (`BundledFirmware`) is
  story-only today and untouched. When it goes live it must choose its
  comparison build from the device's chip through the same function.
