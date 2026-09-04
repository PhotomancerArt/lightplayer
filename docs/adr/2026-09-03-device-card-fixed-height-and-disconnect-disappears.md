# ADR: The device card's rows are fixed so board events never move it; an unplugged board is not a card

- **Status:** Accepted
- **Date:** 2026-09-03
- **Deciders:** Photomancer
- **Supersedes:** None (amends the zone/row ruling `device_roster_card.rs`'s
  own module doc has carried since P4, refined by P9 below)
- **Superseded by:** None

## Context

The device card (`lp-app/lpa-studio-web/src/app/home/device_roster_card.rs`)
carried six annoyances into this work: the armed-confirm chip reflowed the
whole row on arm; the terminal was low-contrast and unreadable; project/example
picking was an inline list that grew with the library; boards offered to flash
were not filtered to the plugged-in chip; the chip was never shown; and there
was no preview at all. A three-round spike
(`spikes/device-card-v2/index.html`, commits `3bdd9d481` → `90a8fbe7b` →
`f6812d703`, gates 2026-09-02) converged on one box with full-bleed
separators and a fixed-row layout, then two production gates
(G1 2026-09-03 morning, G2 2026-09-03 afternoon — both visual + bench walks
with Yona on the C6) refined the zoning and named two more product rules.

Two failure modes motivated the "never move" rule specifically. First, a
board event — a heartbeat, a fault, a new terminal line, a flash's percent
ticking up — used to change how much text a card's state area held, which
made a grid of cards visibly jump while nothing the user did caused it.
Second, the roster used to render an offline (registry-rehydrated, no live
link) device as a dimmed card sitting in the grid alongside live ones,
which reads as "something is wrong with this card" rather than "this board
is not here right now."

## Decision

### 1. Board events never change a card's height

Every row in every zone is fixed height, in every state, whether it holds
content or not:

- **info line** — 17px, one line, ellipsised (`title` carries the full
  text on truncation).
- **bar slot** — 4px, unlit when its zone has no activity.
- **preview slot** — 120px, present only in the Project zone; an honest
  sentence ("no picture yet", "nothing loaded", "a blank chip has no
  picture") stands in until the preview feed (a later milestone) has
  something to show.
- **verb row** — 30px, whether it holds verbs or is empty (withdrawn
  during an activity, except the Device zone's escapes — Forget must work
  mid-flash).

The card is one box: the `article` carries no padding, and each zone is a
`section` with its own `tw:px-4` and (for every zone but the header) a
full-bleed `tw:border-t tw:border-border-strong` hairline that runs edge to
edge — the same convention the node cards use
(`node_card_section.rs::section_container_class`). Nothing inside a zone
draws its own frame; a bordered sub-panel would put a box inside the box.

The middle of the card is three subjects, not one undifferentiated state
area (P9, ruled at G1 after the walk read the shipped zones as "Flash
firmware sits in the project section"):

| zone | fixed rows | measured height (400px column) |
|---|---|---|
| header | title · status chip (24), then the identity as two fixed 16px mono rows: board · chip / MAC · firmware (amended 2026-09-04 — one 16px line before, 74px measured; this table's original 72 predates the 24px chip) | 90px |
| Project | preview (120) · info (17) · bar (4) · verbs (30) | 220px |
| Firmware + Terminal | info (17) · bar (4) · verbs (30), then the terminal flush as the zone's last block | 252px |
| Device | info (17) · verbs (30) | 80px |

All six idle/active states (Running, Nothing-loaded, Needs-firmware,
Flashing, Sending, Degraded) measured **626px** total at every column width
tested (320/400/420px+), confirmed by a CDP measurement pass against the
served `devices_card_states` story — **644px** (header 90) since the
2026-09-04 amendment below gave the header its second identity row,
re-measured the same way on both `devices_card_states` and
`devices_card_firmware_faces`. No zone ever grows past its own fixed
rows; the only two cases where the card DOES reflow are a user-triggered
popover pick panel (which floats in the browser's top layer and never
touches in-flow layout) and the footer-style wrap of the Device zone's
escapes below roughly 412px, which the row-column layout tolerates rather
than forbids (spike Q9).

The terminal shares the Firmware zone rather than owning a zone of its
own: it is the same subject said twice — what firmware is on this board,
and what that firmware is saying — and it is FLUSH inside that zone (no
padding, no border, no rounded sub-panel), so its dark ground reaches both
of the card's own edges. A terminal drawn as a bordered box inside the
zone would be exactly the nesting the "one box" rule forbids.

The armed-confirm chip (`ActionButton`, app-wide — not only this card)
renders both its resting and armed labels in one grid cell; the armed
label sits at `visibility:hidden` at rest and reserves its own width, so
arming changes text and tone only and never shifts a neighbouring chip.
This is the reserve-width mechanism the spectrum-outline ADR's decision 3
was amended to describe
([2026-08-31-spectrum-outline-primary-voice.md](2026-08-31-spectrum-outline-primary-voice.md)).

### 2. Disconnect → disappear

A device the roster cannot currently see is not a card. The Devices page
splits `RosterView` at `DeviceStatus::Offline`
(`lpa_studio_core::split_roster`): connected devices render as cards in
the grid; everything Studio remembers but cannot currently reach collapses
into one quiet line beneath the grid — "N remembered board(s) not
connected · show" — whose expanded tiles carry the two verbs an absent
board can honestly offer, Reconnect and Forget. Unplugging a board removes
its card; plugging it back in brings the card back, named.

The fold (`lpa-devices`) never wrote an offline card into existence for
its own sake; the page choosing to hide the split rather than delete
anything is what keeps the roster from re-deriving identity on
reconnect — the record row and its learned board id/chip persist under
the line the whole time.

### 3. Auto-name, never a MAC

A registered board with no name earns one the moment its board id is
known and it settles: `"<board> · <Mon D>"`, minted once by
`auto_record_name` (`lpa-devices::record.rs`) and folded at the top of
`settle_device_records` (`lpa-studio-core::studio_controller.rs`). Before
this, only the Flash gesture minted a name — a board that arrived already
flashed (the common bench case) kept its MAC address as its title
indefinitely, which is what the G1 bench walk caught directly ("the card
NAME was still a MAC address"). The card's header always shows a name now,
never a raw transport address, whether the board arrived via Flash or was
already running LightPlayer firmware when it first hello'd.

## Consequences

- A grid of device cards is visually still while boards heartbeat, fault,
  or stream terminal output; only a user action (opening a picker, arming
  a chip, unplugging a board) changes what the page shows.
- Every renderer (this card, `PendingLinkCard`, the remembered tile) that
  wants a preview, a bar, or a verb row must render the fixed-height slot
  even when it has nothing to say, rather than omitting the row — an
  omitted row is exactly what reintroduces the reflow this ADR forbids.
- The three-subject zoning means a verb lives wherever its subject lives:
  `Flash firmware` and `Factory reset` sit under the firmware line,
  `Remove` sits under the project name, `Reset`/`Disconnect`/`Forget` act
  on the device. There is no verb menu and no separate footer.
- Story baselines move whenever a zone's content, a verb's placement, or a
  fixture's board id changes; `devices_card_states` (six states side by
  side at 400px) is the height-determinism gate, captured by CI's story
  comment on every PR that touches this file.
- The remembered line is page-local UI state (its open/closed fold), not
  model state — the device model has no opinion on whether a person has
  chosen to look at what it is not currently connected to.

## Alternatives Considered

- **Aspect-fit preview** — scaling a live frame into the 120px slot with a
  per-layout aspect ratio was cut for this pass; the preview FEED is its
  own later milestone, and an honest "no picture yet" sentence costs
  nothing to maintain in the meantime. Aspect-fit returns with the feed.
- **Collapsing a zone's rows during an activity** — tried in the spike and
  ruled out for the state zone ("I'll need to feel it," Yona, spike gate
  2026-09-02): a section disappearing while its own activity runs reads as
  the card losing track of itself, not as tidiness. Activities keep every
  zone visible, light that zone's bar, and withdraw only the verb row
  (except the Device zone's always-present escapes).
- **A footer verb menu** — deferred rather than built: the six-annoyance
  list did not include "too many buttons," and a menu adds a click to
  every verb for a footer that, per the height table, has not actually run
  out of room yet at normal widths.
- **The dimmed offline card** — kept the disconnected board in the grid at
  reduced opacity with its escapes still reachable. Rejected: it reads as
  "broken," not "not here," and it means the grid's item count never
  matches what a person can actually see plugged in.
- **Labelled sections** ("Project" / "Firmware" / "Device" headers) —
  tried at G1 and ruled out at the walk ("kinda ugly"): each zone is
  recognisable by what it says and what it offers — the way the header
  reads as identity without a label — so P9 re-homed verbs by subject
  instead of adding chrome to name the subjects.

## Follow-ups

- Preview FEED for the 120px slot (own milestone; a standing frame
  conversation that yields to verbs — the M5 editor-lens tap is the
  nearest existing seam). Aspect-fit sizing returns with it.
- Footer/verb menu — revisit if the Device (or a future) zone's verb row
  starts clipping at the widths Studio actually ships at, not before.
- Landscape board renderings in the picker tiles (P10 follow-up) are
  unrelated to card height but share the "board's own picture" seam this
  ADR's Project/Firmware zoning opened up for pickers.

## Amendments

- **2026-09-04 — the identity line is two fixed rows (header 74 → 90px, cards 628 → 644px, measured at the 400px column).**
  The header's identity was one truncated mono line, `board · chip · MAC ·
  fw <label>`. At the 400px column every card on
  `devices_card_firmware_faces` ellipsised at `… · fw fw-esp…`, so the
  clause PR #514 added — the firmware a closed window still remembers,
  marked as memory — was invisible on the one card it exists for. The
  identity now prints as two 16px rows, `board · chip` over `MAC ·
  <label>` (`DeviceIdentityLine::rows`), each truncating on its own and
  the pair a fixed 32px slot whether or not the second row has anything to
  say — so this is a height-table change, never a reflow. The clause also
  lost its `fw ` prefix (every label already starts with `fw-`) and the
  memory mark became its own dotted clause, `· last seen`, in the dim tone
  but the same selectable text (no fold, no tooltip-only: the identity is
  drag-selected and pasted). That row split rather than hardware-over-
  firmware because it is the one that holds the longest catalog board name
  at 400px. Spike: `spikes/device-card-identity-line/index.html`
  (treatment E, board·chip / MAC·fw split).

## Spike and gate record

- Spike: `spikes/device-card-v2/index.html`, rounds 1–3, commits
  `3bdd9d481`, `90a8fbe7b`, `f6812d703` on
  `claude/studio-device-ux-improvements-106d7f`; gates 2026-09-02.
- G1 (end of P7, visual + bench with Yona): 2026-09-03 morning. Ruled the
  fixed-row state zone good, the one-box separators right, the terminal
  readable but wanting the full zone width with no inset, the pickers
  good, and — from the bench walk on the C6 (plug → firmware face → Flash
  → empty face → push → Running → Open → unplug → card disappears →
  replug → card returns) — caught the MAC-as-name defect and the
  hard-to-read zoning that became P9.
- G2 (end of P10, visual + bench re-walk with Yona): 2026-09-03 afternoon.
  Confirmed the three zones read as Project/Firmware/Terminal/Device
  without labels, the flush terminal, the bench C6 carrying an auto-name
  instead of its MAC, and the board renderings making the picker's pick
  obvious; re-walked flash/push/unplug/replug on the bench C6.
