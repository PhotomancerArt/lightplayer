---
status: open
found: 2026-10-10      # how: the connected feel gate's preview, in the app's browser pane at about 800 px wide; finding 3
area: lpa-studio-web board card × `BoardPickPopover` (Install's board pick)
class: assumed-context
related:
  - lp2025/2026-10-08-2330-connected-in-the-card (the connected feel gate, finding 3)
---
# The Install board picker is clipped in a narrow pane

**Symptom** — in a pane about 800 px wide, pressing Install on a board card
opens the board-model picker clipped on its left edge. After a click inside
it, it lays itself out again and fits.

**Root cause** — not established. The picker is placed against the Install
button at the card's right end, and its first placement seems to use a
width or an anchor measured before the popover's own content was laid out;
the second layout, after the click, measures again. The card stack's width
at that pane size is likely the trigger.

**Not fixed** — filed from the gate; held for its own change. A story at
800 px with the picker open would pin it.
