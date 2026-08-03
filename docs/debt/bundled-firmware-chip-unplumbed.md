---
status: carried
since: 2026-07-27
logged: 2026-08-01
area: lp-app/lpa-studio-web (roster device cards)
related:
  [
    "../../lp-app/lpa-studio-core/src/app/roster/firmware_update.rs",
    "../../lp-app/lpa-studio-web/src/app/home/device_card.rs",
    "../../lp-app/lpa-studio-web/src/app/home/home_gallery.rs",
  ]
---
# The "firmware update available" chip never fires in production

**Shape** — `BundledFirmware` (`firmware_update.rs`) is the evidence for the
advisory amber chip on a device card's Settings tab: the packaged image's
commit vs the device hello's `FwProvenance::commit`. The comparison logic is
implemented and unit-tested, `DeviceRichInput::bundled_fw` threads it through
`device_rich_object`, and `DeviceCard` takes it as a prop
(`device_card.rs`, `bundled_fw`).

Nothing in production ever supplies it. `home_gallery.rs` constructs every
`DeviceCard` without the prop, so it defaults to `None`; the only call sites
that pass `Some(..)` are `roster_card_stories.rs` and the core's own tests.
The chip is therefore visible in the design library and invisible in the app.

**Why it matters** — the whole point of the chip is to notice that a desk
board is running older bits than the Studio build in front of you. Today it
silently never notices, and the code reads as if it does. Tests pass; stories
render; the feature does not exist.

**What it would take** — Studio has to read the packaged manifest at runtime:
fetch `./firmware/<build-id>/manifest.json` (schemaVersion 2, from roadmap
M3), take `core.commit` / `core.dirty`, hold it in app state, and pass it down
through `home_gallery` to every non-sim `DeviceCard`. That is a browser fetch
plus app-state plumbing, and it needs an answer for the desktop/native shell
where there is no served manifest — which is why it was left out of M3 rather
than bolted on.

**Not** a rendering bug: do not "fix" it by making the chip appear in stories
only, and do not delete the comparison logic — it is correct and wanted. See
roadmap `2026-08-01-1200-firmware-manifest`, M3.

**Update 2026-08-02** — "fetch `./firmware/<build-id>/manifest.json`" now has
to answer *which* build id, and the honest default changed. The site serves
three (`lp-fw/builds/served.json`), so a single fetched manifest is the right
comparison for one chip and wrong for the other two: on an S3 it would
compare the device against the C6 image's commit and advise an "update" that
is really a different ISA. Resolve the build from the device's own chip
through `lpa_boards::provisioning_build_id(None, chip)` — the same function
provisioning uses — and fetch that one, or say nothing when it resolves to
`None`. Cheaper than it sounds: the chip is already on the card as
`UiDeviceCard.detected_chip`, and `ServerHello` carries it for a Ready link.

This is also why the gap did **not** become a live wrong-image bug when
chip→build selection landed: nothing supplies `bundled_fw`, so there was no
hardcoded C6 comparison in production to correct. The plumbing job simply
grew a correctness requirement it did not have when this entry was filed.
See `docs/adr/2026-08-02-flash-image-selected-from-the-discovered-chip.md`.
