---
status: fixed
found: 2026-08-02      # how: review question during the M5 gate
area: lpa-link browser_serial_esp32/browser_esp32_flash.js (flashFirmware)
class: metadata-parsed-but-never-enforced
related: 2026-08-02-erase-fails-a-successful-erase-on-flash-id-noise.md
---
# Studio would flash a C6 image onto an S3 without a word

**Symptom** — latent; caught by inspection, not by a user. Studio's
browser provisioning flow writes its firmware image to whatever ESP32 is
connected, with no check that the image was built for that chip. On an
ESP32-S3 or a classic ESP32 the board takes a C6 image, fails to boot,
and nothing explains why.

**Cause** — `flashFirmware()` calls `loader.main()` (which returns the
chip the bootloader handshake identified), then calls `loader.writeFlash()`
— never comparing the two. The manifest's `targetChip` IS parsed, IS
validated as present, and IS reported all the way out through
`LinkFirmwareManifest::target_chip` … and is read by nobody. The fact was
sitting in a local variable one line above the write.

The **host** provider never had this hole: it passes `TARGET_CHIP` into
espflash, which raises `ChipMismatch` at connect. Only the browser path,
which drives esptool-js directly, skipped the check.

**Fix** — `assertImageMatchesChip` before `writeFlash`, comparing on
alphanumerics only (the bootloader says "ESP32-C6", manifests say
"esp32c6"). Only a DEFINITE mismatch refuses; an unidentifiable chip on
either side proceeds as before, so the guard catches the wrong image
without inventing a new way to block a legitimate flash.

**Lesson** — the same shape as its sibling defect on the erase path, from
the opposite direction: there, a proxy signal outranked the real outcome;
here, the real fact was carried faithfully through four layers and then
consulted by nothing. Threading a value through to a struct field is not
the same as USING it — grep for readers, not just for the field.

**Not fixed here** — Studio still serves only `esp32c6-4mb`, so an S3 or
classic now gets a clear refusal instead of a silent wrong write, but
still cannot be provisioned. Chip→build selection is filed separately
(`SERVED_FIRMWARE_BUILDS` is the seam; `lpa_boards::compatible_builds_for`
already computes the join).

**Coverage** — none: `browser_esp32_flash.js` is browser-only JS driving
real USB hardware and the repo has no harness for it. The guard's decision
table lives in its doc comment.

**Amendment 2026-08-02** — the comparison rule shipped here was wrong, and
the absent coverage is why it shipped. `assertImageMatchesChip` assumed the
bootloader says `ESP32-C6` where the manifest says `esp32c6`; esptool-js
actually returns `getChipDescription()` — `ESP32-C6 (revision 0)`,
`ESP32-S3 (QFN56) (revision v0.2)`, and for the classic a die name like
`ESP32-D0WDQ6`. Under equality every one of those is a "DEFINITE mismatch",
so the guard refused **every** legitimate flash, C6 included. Replaced by
`lpa_link::chip_id_from_reported` (ordered prefix table, Rust-side and
unit-tested, handed to JS as data) in
[provisioning-flashes-one-image-unchecked](2026-08-02-provisioning-flashes-one-image-unchecked.md),
which also carries the transcribed esptool-js strings and the selection
half this entry deferred.
