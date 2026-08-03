---
status: fixed
found: 2026-08-02      # how: hardware-walk (M5 provisioning gate, PR #277)
fixed: this change
area: lpa-link/providers (browser + host serial ESP32), lpa-boards, justfile
class: assumed-context
related:
  - 2026-08-02-browser-flash-never-checks-the-chip.md
  - docs/adr/2026-08-02-flash-image-selected-from-the-discovered-chip.md
  - 2026-07-31-1002-hardware-board-selection (roadmap M5)
  - 2026-08-02-1706-provisioning-firmware-build-selection
---
# Provisioning had only one image to give, and no rule that could compare it

Sibling of [browser-flash-never-checks-the-chip](2026-08-02-browser-flash-never-checks-the-chip.md),
which fixed the missing guard. This entry is the other half — there was
nothing for a guard to choose *between* — plus a defect **in that fix**,
found while extending it.

**Symptom** — Studio's browser provisioning could only produce an ESP32-C6
device. Pointed at an S3 or a classic it flashed the C6 image, reported
success, and left a board that does not boot. Found at the M5 provisioning
gate, on hardware, by having other chips on the desk.

**Root cause** — `LinkManagementRequest::FlashFirmware` carried no payload.
The image was fixed at provider construction (one manifest path in
`BrowserSerialEsp32Options`), and the site published exactly one build, so
the picker's eligibility filter quietly hid every non-C6 board. The
sibling defect's guard converts the silent wrong write into an honest
refusal; it cannot make an S3 flashable, because no S3 image was served.

## The guard's own defect: equality refuses every real device

The merged guard (`assertImageMatchesChip`) compared chip names for
equality after stripping non-alphanumerics, and refused only a "DEFINITE
mismatch". The premise was that the bootloader says `ESP32-C6` where the
manifest says `esp32c6`. It does not.

esptool-js 0.6.0's `main()` returns `getChipDescription()`, and reading the
shipped bundle's per-target implementations (2026-08-02):

| chip | `getChipDescription()` | normalized |
|---|---|---|
| C6 | `ESP32-C6 (revision 0)` | `esp32c6revision0` |
| S3 | `ESP32-S3 (QFN56) (revision v0.2)` | `esp32s3qfn56revisionv02` |
| classic | `ESP32-D0WDQ6 (revision v1.0)` (a **die** name) | `esp32d0wdq6revisionv10` |

None equals the manifest's bare id, so every one is a "DEFINITE mismatch"
and **every legitimate flash is refused** — including the C6 the guard was
written to keep working. The failure was not caught because the guard's
only coverage is hardware, and the C6 walk that would have caught it had
already happened before the guard landed.

A substring test — the rule `LinkFlashRegion::lpfs_for_chip` was already
using for partition offsets — fails the other way: all three strings
*contain* `esp32`, so a classic image would sail onto a C6. That one
worked only because `esp32` had no row in its table.

**Fix** — `FlashFirmware { build_id }`;
`lpa_boards::provisioning_build_id` computes it from the discovered chip
refined by the picked board; `lp-fw/builds/served.json` became the single
statement of what ships and now lists all three builds.
`provider::chip::chip_id_from_reported` replaces both broken comparisons
with a prefix match over an ordered id table, most specific first, bare
`esp32` last. Every comparison goes through it — the flash guard, the lpfs
region table, build selection, the picker's family matching — and the JS
guard receives the table **as data from Rust** rather than as a copy.

**Regression coverage** — `provider::chip::tests`, notably
`the_strings_esptool_js_actually_returns_all_resolve` (the table above,
transcribed from the shipped bundle) and
`no_id_is_hidden_by_an_earlier_prefix`, which enforces the ordering rule
over the whole list so a future id cannot shadow one already there;
`host_esp32_flash::tests::a_mismatched_image_is_refused_by_name`;
`firmware_join::tests::*` for the four selection cases;
`firmware_join_drift::{every_served_build_has_a_build_def,
chip_names_are_already_normalized}`; the Pages smoke check fails an
artifact missing any served build.

**Lesson** — two lessons, and the second is the expensive one.

A constant standing in for a fact the system can observe (`TARGET_CHIP`,
one manifest path, a one-element served list) is not a simplification; it
is an assertion that the plural case will never arrive, and it fails
silently on the day it does.

And: **a guard written against a remembered string format is a guess.** The
comparison rule here was derived from what the two names "obviously" look
like, and every observable spelling contradicted it — the repo's own
`flash_region.rs` had recorded the chatty form months earlier, and the
answer was fifty lines away in the vendored library. When a comparison
depends on an external tool's output format, read the tool. The cost of
getting it wrong is not a missed catch; it is a guard that blocks the
operation it was protecting, on hardware, where the only feedback loop is
a human with a board.
