---
status: fixed
found: 2026-10-24      # how: hardware-walk (Yona updating FS3 from Bluefy over Bluetooth)
area: lp-app/lpa-studio-core own build (bundled_own_build.rs, own_build_source.rs, update_build_facts.rs, update_host.rs, studio_controller.rs)
class: config-masked-defect   # every walk staged ONE target's build, so "the first served build" was always the board's
related:
  - lp2025/2026-10-20-2335-s3-radio-and-ota (strip-test/ble-update.md)
  - docs/adr/2026-10-23-the-s3-ships-a-split-image.md
  - docs/adr/2026-10-06-studio-updates-over-the-update-channel.md
---
# Studio offered every board the C6's build

**Symptom** — FS3 (XIAO ESP32-S3 Plus, 16 MB), release 2026.10.24-4, a
healthy split image, connected to lightplayer.app's Studio from Bluefy over
Bluetooth: no Update. The card read "Needs one update over USB — This board
can't update over Bluetooth yet." Over the cable, `lp-cli link capture
--ota-offer <2026.10.24-6's S3 files> --ota-heal-only` read the same board
as layout 2, loader 1, `running`, and decided `OfferUpdate` at once: the
board was never the problem.

**Cause** — Studio held **one** build of its own: `BundledOwnBuildSource`
read the served targets in `served.json` order and kept the first
update-capable one, which on lightplayer.app is always `esp32c6-4mb`. Every
card's standing, every automatic restore and every install decided against
that build, so an S3 board was compared with the C6's (`OtherTarget`: no
offer, on any link), and "Other version…" was never listed for it; the
store's release index was even fetched for the C6's target, because the
controller took its target from the own build before any board's. An
`Install` of a version the C6's build shared would have loaded the C6's
bytes for the S3 (`wants_own` compared versions only); the board would have
refused them by chip.

The exact words Yona saw (`NeedsUsbOnce`) are not what today's code
produces for that board (it produces no offer at all); a Studio page from
before 2026-10-21, whose code table had no chip 2 or layout 2, produces
exactly them. Either way, the fixed Studio offers the update.

**Why it hid** — every S3 update walk (`walk-ota-emu --s3`, `--ble`) staged
the S3's package alone, so the first served build was the board's. The one
silicon S3 update (2026-10-22) went through `lp-cli`, which is handed its
build directly.

**Fix** — the own-build source keeps every update-capable served target
(`OwnBuildSource::facts` is a list, `load(target)` loads one), and each
board is decided against its own target's build (`own_for`); a board of a
target Studio holds no build of gets none. The store is asked about the
first board's target. Tests:
`a_studio_serving_two_targets_updates_a_board_to_its_own_targets_build`
(e2e, over Bluetooth, the other target listed first),
`every_update_capable_target_is_kept_and_loaded_by_target`,
`a_board_gets_the_build_of_its_own_target`.

**Still one target at a time** — the store's `latest` and release index are
held for one target (the first board's). A desk with a C6 and an S3 open at
once lists "Other version…" choices for one of them only; Update itself is
right for both.
