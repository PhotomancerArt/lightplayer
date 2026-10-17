---
status: fixed
found: 2026-10-15      # how: the board card's words, round 1; named as P16's bad example in docs/ux/README.md
fixed: 2026-10-16      # PR #1159, the board card's words round 2
area: lpa-studio-core `board_card/access_bar.rs`, `connection_bar.rs` (`UiStatusKind::Warning` on the card)
class: borrowed-meaning
related:
  - docs/ux/README.md (P16: one treatment per meaning)
  - docs/style/ui.md ("Status hues never move")
---
# The access bar wore the unsaved-edit yellow

**Symptom** — a board open to edit with no password read "You can edit ·
anyone can edit" in yellow, and a board no password reached over the air
read "No password here · set one over USB" in yellow. A board that stopped
answering ("USB · not responding") was yellow too. Yellow is the colour of
an unsaved edit everywhere else in Studio; on the card it meant "worth
knowing" one moment and "a device needs you" the next, while the same
card's Update was blue, the live family.

**Root cause** — the card's bars picked tones by how loud a state should
feel, not by the family that state belongs to. `UiStatusKind::Warning`
(documented as the unsaved-edit yellow) and `UiStatusKind::Live` (blue,
"live") were borrowed because they sat between quiet and orange. Nothing
checked a bar's tone against ui.md's families.

**Fix** — the card uses ui.md's families and no others: an open board is
quiet (O4: open is how boards start); a board no password reaches is
orange (a device needs you); not responding is orange; an update on offer
is untinted, its button carrying it; blue is the lit cloud's alone; red is
broken (a shader error, crashing firmware, a connection that failed).
`UiStatusKind::Live`'s doc says so.

**Test** — `access_bar::tests::an_edit_grant_says_password_and_an_open_board_is_quiet`,
`access_bar::tests::a_board_no_password_reaches_is_usb_only_in_orange`,
`connection_bar::tests::a_board_not_responding_offers_retry`,
`firmware_bar::tests::the_update_line_sets_the_version_apart`.
