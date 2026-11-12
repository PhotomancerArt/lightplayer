---
status: fixed
found: 2026-10-07     # e2e (walk-ota-emu --steps install-older)
fixed: 9b3cad436
area: lpa-studio-web `app/home/device_roster_card.rs` (the firmware zone's verb row)
class: assumed-context
related:
  - lp2025/2026-10-06-2307-ota-install-another-version
---
# Three firmware chips painted over each other in a narrow card

**Symptom** — the walk's `install-older` step, on a board on a published
release, showed "Install dev …", "Other version…" and Factory reset drawn on
top of one another in the device card's firmware row, so the chips could not
be read or reliably hit. The same card with Update and Other version… beside
it (no install chip) fit.

**Root cause** — the firmware row used `verb_row_class()`: a fixed 30 px-high
flex row with `whitespace-nowrap` and no wrap. That class was written when
the zone held at most two firmware chips, and its own doc comment promises the
flexible spacer collapses first in a narrow card. With "Other version…"
offered beside another install verb there were three chips and no spacer left
to collapse, so the nowrap chips overflowed their row and overlapped. The
restore face had met the same shape earlier and already had a wrapping row
class (`restore_verb_row_class()`); the new offer did not opt into it.

**Fix** — the row takes `restore_verb_row_class()` (the same look, free to
wrap onto a second line) whenever `install-firmware` sits beside
`update-firmware` or `reinstall-firmware` (`firmware_row_crowded`).

**Regression coverage** — none automated: the wrap is a layout fact and no
test sweeps the card's width. The walk found it at the card's narrow width;
the picker's stories (Yona's visual gate, 2026-10-08) show the row.

**Lesson** — a fixed-height nowrap row assumes how many things it will ever
hold. Every offer added to a zone can be the one that crosses that number, and
the row cannot say so: it paints them over each other. When a verb row gains a
verb, check the narrowest card with every verb of the zone offered at once,
not just the new verb beside the old default.
