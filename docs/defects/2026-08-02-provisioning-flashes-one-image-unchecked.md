---
status: fixed
found: 2026-08-02      # how: hardware-walk (M5 provisioning gate, PR #277)
fixed: this change
area: lpa-link/providers (browser + host serial ESP32), lpa-boards, justfile
class: assumed-context
related:
  - docs/adr/2026-08-01-firmware-manifest-architecture.md
  - 2026-07-31-1002-hardware-board-selection (roadmap M5)
  - 2026-08-02-1706-provisioning-firmware-build-selection
---
# Studio provisioning flashed one image at every board, and never checked the chip

**Symptom** — Studio's browser provisioning flow could only produce an
ESP32-C6 device. Pointed at an ESP32-S3 or a classic ESP32 it flashed the
C6 image anyway, reported success, and left a board that does not boot. No
error, no warning, nothing in the log distinguishing it from a good flash.
Found at the M5 provisioning gate, on hardware, by having other chips on
the desk.

**Root cause** — two independent holes, and each one alone was enough.

*No guard.* `browser_esp32_flash.js`'s `flashFirmware()` obtained the chip
from `loader.main()`'s SYNC handshake and then called `loader.writeFlash()`
without ever comparing it to the manifest's `core.target.chip`. The chip
was **known** — it was in a local variable, three lines above the write —
and simply not consulted. The host provider had the mirror-image hole: it
declared `Chip::Esp32c6` to espflash from a `const TARGET_CHIP`, so its
handshake was checked against a constant rather than against the image it
was about to write.

*No selection.* `LinkManagementRequest::FlashFirmware` carried no payload.
The image was fixed at provider construction (one manifest path in
`BrowserSerialEsp32Options`), so there was nothing for a guard to select
*between* even if one had existed — and the site published exactly one
build, so `served.json`'s ancestor was a `&[&str]` with one entry and the
picker's eligibility filter quietly hid every non-C6 board.

Both are the same shape: the code had the fact (the chip) and acted on an
assumption (the deployment's single image) instead.

**Fix** — the request names a build
(`FlashFirmware { build_id: Option<String> }`); `lpa_boards::provisioning_build_id`
computes it from the discovered chip refined by the picked board; both
providers refuse before the first write when the handshake's chip is not
the image's. `lp-fw/builds/served.json` became the one place that says
which images ship, read by `lpa-boards`, the justfile and the Pages smoke
check, and it now lists all three.

A second defect surfaced while writing the guard and is fixed in the same
change: **whole-string comparison of chip names does not work, and neither
does a substring test.** esptool-js reports the classic ESP32 by die name
(`ESP32-D0WD-V3 (revision v3.0)`) and the C6 as
`ESP32-C6 (QFN32) (revision v0.2)`; every one of those strings *contains*
`esp32`, so the substring rule `LinkFlashRegion::lpfs_for_chip` already
used would have accepted a classic image on a C6. It worked only because
`esp32` had no entry in its table. `provider::chip::chip_id_from_reported`
resolves a reported name against an ordered id table, most specific first,
bare `esp32` last — and that ordering is asserted, not trusted.

**Regression coverage** —
`provider::chip::tests::{every_reporter_spelling_resolves_to_the_same_id,
the_classic_esp32_resolves_from_its_die_name,
a_more_specific_id_wins_over_the_bare_family}`;
`host_esp32_flash::tests::{a_mismatched_image_is_refused_by_name,
every_served_build_target_maps_to_an_espflash_chip}`;
`firmware_join::tests::*` for the four selection cases;
`firmware_join_drift::{every_served_build_has_a_build_def,
chip_names_are_already_normalized}`; the Pages smoke check fails an
artifact missing any served build. The JS guard has no unit test — this
repo has no JS test harness — so the ordered chip table crosses into JS as
**data from Rust** rather than as a second copy, which is what makes the
Rust tests cover it. The end-to-end proof is the hardware walk.

**Lesson** — a value the code already holds and does not check is worse
than a value it cannot get: the failure looks like success, and every
diagnostic points somewhere else. Both halves here were introduced by
scope that was *correct at the time* — one build existed, so "which build"
was not a question and "is it the right one" had only one answer. The
signal to watch for is a constant standing in for a fact the system can
observe (`TARGET_CHIP`, a single manifest path, a one-element served
list). Those are not simplifications; they are assertions that the plural
case will never arrive, and they fail silently on the day it does.
