---
status: fixed
found: 2026-10-10      # how: the connected feel gate's preview (?emu=tab, an in-page C6 just installed); Yona's first round, 2026-10-14
fixed: this change
area: lpa-studio-core `device_layout_view` (`lost_files`) × the firmware bar's restore row
class: state-conflation
related:
  - docs/defects/2026-10-06-an-unnamed-board-was-told-its-files-were-missing-and-lost-update.md
  - lp2025/2026-10-08-2330-connected-in-the-card (the connected feel gate, findings 1 and 11)
---
# A freshly installed board is told its files need restoring

**Symptom** — every board Studio had just installed said "Its files need
restoring" on its firmware bar, orange, with the status corner raising it.
Yona, on his own board in the preview: "a board saying 'its files need
restoring' is confusing. what am I supposed to do there? what if I just want
to reset it?" The bar offered Restore from a backup file… and nothing else:
no way to say "leave it empty".

**Root cause** — the card read the board's hello `fs: formatted` as "came
back without its files". The 2026-10-06 fix made a mounted board need
evidence, but kept `formatted` as evidence on its own. Every fresh install's
first boot formats its storage, so a board that never held anything was told
it had lost its files. Two states — "formatted because it is new" and
"formatted because it lost what it had" — read as one.

**Fix** — a formatted boot is a loss only on evidence the board held files:
a backup pending for it in this browser, or this browser remembering it
running a project (seen in this tab, or a project this browser gave it), or
a stamped identity its hello no longer names (`FilesEvidence`,
`BoardFilesMemory`). Never on a board running a project. When the loss is
real, the bar says "Its files are gone" and the notice says what to do:
Restore files (a backup here), Restore from a backup file…, or **Start
fresh**, a Routine offer (`devices/<board>/start-fresh`) that lets the files
go and leaves the board empty for Add a project. The choice is kept per
board in this browser until the board runs a project again, and a factory
reset done here counts as one.

**Regression coverage** —
`device_layout_view::tests::a_formatted_boot_is_a_loss_only_on_evidence_the_board_had_files`,
`board_card_tests::a_freshly_installed_board_is_not_told_its_files_are_gone`,
`studio_device_e2e_tests::a_board_that_formatted_offers_its_restore_and_its_update`
(a board seen running, then power-cut back to formatted), and Start fresh
pressed by path in `board_card_tests::a_formatted_boards_firmware_action_restores_its_files`.

**Lesson** — a boot-time fact ("this boot formatted") is not a history ("this
board lost something"). The loss needs memory of what was there, and the
notice needs a way out for the person who does not care.
