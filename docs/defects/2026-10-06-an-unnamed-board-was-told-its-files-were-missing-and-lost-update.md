---
status: fixed
found: 2026-10-06      # hardware-walk (Yona's desk, production Studio)
fixed: this change
area: lpa-studio-core device_layout_view × lpa-studio-web device_roster_card
class: state-conflation
related:
  - docs/adr/2026-10-02-c6-repartition-and-layout-migration.md
  - docs/adr/2026-08-04-device-identity-anchored-in-silicon.md
  - docs/reports/2026-10-02-c6-migration-emulator-walk.md
  - lp2025/_archive/2026-10-01-1843-c6-repartition
---
# An unnamed board was told its files were missing, and lost its Update

**Symptom** — loose-c6 (`10:BD:A3:B0:8E:30`) was carried to the new flash
layout with `lp-cli hardware lpfs migrate`. It then ran release
2026.10.06-12, with its filesystem mounted and its project loaded. It had
never been named: there was no `/.lp/device.json`. Over USB in production
Studio (release -13), the card said only "This board needs its files back —
restore them from a backup file." and offered no Update. Yona: "not being
able to update a board in a state like this doesn't make sense."

**Root cause** — two mistakes, one in core and one in the card.

- **Core.** `device_layout_view` read `fs: mounted` with no uid as "came
  back without its files" (#896's widening after the walk's W7b: the boot
  after a format mounts an empty filesystem that names no one). But Studio
  no longer stamps `/.lp/device.json`: an ESP board's identity is its efuse
  MAC (ADR 2026-08-04). So "mounted, no uid" meant two different things:
  "lost its files" and "was never named". The card always assumed the
  first.
- **Card.** In the firmware row, the restore verbs *replaced* the ordinary
  verbs instead of joining them. Core still offered `update-firmware`, but
  the card never drew it. An update, over the air or a flash that keeps
  `lpfs`, never touches the board's files, so nothing justified hiding it.

**Fix** — core claims lost files only on evidence. That means one of:

- `fs: formatted` (the boot that formatted);
- `fs: mounted`, no identity, and a backup still *pending* for the board's
  MAC in this browser.

The pending backup is the repartition's own "unfinished migration" state
(notes Q14). It carries W7b: a reboot between the interruption and the
user's return still offers the way back. A mounted, unnamed board with no
pending backup gets the ordinary card. The card now draws the board's
Update beside Restore files, Download backup and "Restore from a backup
file…". That is core's existing `update-firmware` offer, not a new
web-built action. The fake board gained `fake_power_cycle`, so an
end-to-end test can walk W7b.

**Regression coverage** —

- `device_layout_view::tests::a_mounted_board_that_was_never_named_is_not_told_its_files_are_missing`
- `studio_device_e2e_tests::an_unnamed_board_with_its_files_gets_the_ordinary_card_and_its_update`
- `studio_device_e2e_tests::a_board_that_formatted_offers_its_restore_and_its_update`
- `studio_device_e2e_tests::a_reboot_after_a_pull_mid_filesystem_write_still_offers_the_backup_and_update`
  (W7b end to end, through a byte-identical restore)
- `fake_flash_layout::tests::a_power_cycle_after_the_format_mounts_the_empty_filesystem`

**Residuals, not changed here** —

- **A fresh board's first boot.** The first boot after a flash of a blank
  chip also reports `fs: formatted`. Until that board reboots, its card
  says it needs its files back. Update is no longer hidden there.
- **A backup that was only downloaded.** Suppose this browser could not
  store the backup (the user downloaded it instead), the migration was
  interrupted, and the board rebooted again before the user came back.
  The board then has no evidence in this browser and shows the ordinary
  card. The way back is the downloaded ZIP and
  `lp-cli hardware lpfs restore`. On the boot that formatted, the card
  still offers "Restore from a backup file…".

**Lesson** — an absent fact is not evidence. "No uid" had been the
empty-filesystem signal, but it stopped meaning that when identity moved
into silicon, and nothing tied the two decisions together. If a card claims
data was lost, it should key that claim off something that records the
loss (a format, a pending backup), never off a field that is merely
missing. Separately, a verb that cannot harm the state a card is warning
about should never be hidden by that warning.
