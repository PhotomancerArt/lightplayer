# ADR: One consequence level, and an offer tree addressed by path

- **Status:** accepted (2026-10-01, by Yona: "all yes" on D7 and Q1–Q7 —
  see `notes.md`)
- **Deciders:** Yona
- **Refines:** `docs/adr/2026-10-01-agentic-control-offers-in-core.md`
- **Evidence:** planning dir
  `lp2025/2026-10-01-1255-agentic-ui-roadmap/m1-offer-model/` (`plan.md`,
  `notes.md`, P1–P3 Implementation Results); PR #892.

## Context

The parent ADR named two problems: core's offers were scattered across DTO
fields, and the web built actions no other consumer could see. Building the
app agent against that contract surfaced two more, narrower problems in how
one offer describes itself and how a second consumer finds it.

**Four overlapping knobs decided "how serious is this."** `ActionMeta`
carried `destructive: bool`, `confirmation: Option<ActionConfirmation>`
(itself carrying an `inline` flag), and `gesture: ActionGesture`. No
single field answered "should this tint, should it ask twice, and may the
agent press it" — the answer was assembled from up to three fields at each
call site, and sites disagreed. Worse, the two components that draw these
buttons didn't agree with each other: `ActionButton` honoured
`destructive`, `confirmation`, and `inline`, but `PaneActionButton` — the
component that draws every project-header and node-card-header icon —
ignored `destructive` and `inline` entirely and always opened the
browser's `confirm()` dialog. The same verb looked and behaved
differently depending on which component happened to draw it.

**Revert to saved had none of the four knobs set.** It discards every
unsaved edit in the project, with one click and no confirmation of any
kind — and the agent could press it as freely as Save. Whatever semantic
flag replaced the four knobs, this verb needed to land at the top of it,
and its absence from any of the four is itself evidence the old model was
assembled ad hoc rather than designed.

**The agent's readout saw only the root card's buttons.** Since the
flat-root reversal, every node other than the project root is a nested
child in the tree, and `app_agent_readout` only walked the root's
`header_actions`. Remove or Revert on a pattern or fixture several levels
down was invisible to the agent — not refused, just absent, so nothing
failed loudly. The readout had no single thing to iterate that was
guaranteed to hold everything a surface published.

## Decision

### Three consequence levels, plus one platform fact

`ActionConsequence` replaces `destructive`, `confirmation`,
`confirmation.inline`, and `ActionGesture` with one enum
(`lp-app/lpa-studio-core/src/core/action/action_consequence.rs`):

| Level | What it means | UI treatment | Agent access |
|---|---|---|---|
| `Routine` (default) | Nothing is lost. | Plain button. | The agent presses it freely. |
| `Undoable` | Removes something Studio can still get back (Revert restores a removed node until the project is saved). | Error tint, dispatches on one click. | The agent presses it and says what it did. |
| `Lasting(ActionConfirmation)` | Gone for good, from Studio or from a board. Carries the copy saying what is lost, by construction — a `Lasting` cannot exist without it. | Error tint, arms on the first click, acts on the second. No dialog. | The agent never presses it; it becomes the user's own button in chat (a card). |

Separately, `ActionMeta::needs_user_activation: bool` is kept as its own
field, not folded into the levels, because it names a **browser** fact —
`navigator.serial.requestPort()` and `navigator.bluetooth.requestDevice()`
only work from a real click — not something the action itself costs the
user. A `needs_user_activation` button looks plain (nothing is lost) but
the agent still hands it to the user as a card, exactly like a `Lasting`
one. `ActionMeta::needs_user()` is `consequence.arms() ||
needs_user_activation`.

**No browser `confirm()` dialog remains on any action.** `Lasting`'s
two-click arm replaces it everywhere, in `ActionButton`, the icon-sized
`PaneActionButton`, and the session control's Save/Revert — one look per
level, wherever the button is drawn, closing the gap that motivated this
work. The one `confirm()`-shaped dialog left in Studio is the
unsaved-edits navigation guard (`unsaved_gate.rs`, opening or leaving a
project with pending edits): that is a navigation guard, not an action's
confirmation, and it is out of scope here — it belongs to place (M7).

**A level can depend on state.** Remove node is `Undoable` ordinarily —
Revert on the parent brings it back — but `Lasting` when removing it would
sweep pending edits on the subtree that no revert restores, because the
preflight already told the user those edits are gone either way. This is
why `ActionMeta::with_consequence` exists alongside the `.undoable()` /
`.lasting(copy)` builders: some call sites compute the level from data the
builder shorthand can't see.

Per Yona's D7, the full assignment:

- **Lasting:** Revert to saved, Forget sim, Dismiss port, Forget device,
  Factory reset, Remove project from board, Flash firmware, Delete library
  project, and Remove node when it would sweep unsaved edits.
- **Undoable:** Remove node (ordinarily) and Revert node subtree.
- **Needs a real click:** Add via USB, Add via Bluetooth, Reconnect.
- **Routine:** everything else.

### Path ids, and one tree beside the view

Every offer is addressed by a stable `OfferPath`
(`lp-app/lpa-studio-core/src/core/offer/offer_path.rs`): a sequence of
segments written `a/b/c`, for example `project/save` or
`project/demo.module/orbit.shader/remove`.

A node's tree path (`/demo.module/orbit.shader`) sits inline, unescaped,
inside a verb path. That only works because of one rule, enforced by a
`debug_assert` in the verb constructors: **a node segment always contains
a dot** (it is rendered `name.kind`), and **a verb or namespace segment
never does**. `project/demo.module/orbit.shader/remove` reads unambiguously
as the `project` namespace, the node at `/demo.module/orbit.shader`, the
verb `remove`.

`UiOffer` (`core/offer/ui_offer.rs`) pairs a path with the dispatchable
`UiAction` and a required icon; everything else a renderer or the agent
needs — label, summary, priority, enablement, consequence — is read
through to the wrapped action's `ActionMeta`, so that stays the one place
metadata lives.

`UiOfferTree` (`core/offer/ui_offer_tree.rs`) holds every offer a view
publishes, indexed by path, **in publish order** — never sorted by path,
because a surface's buttons must render in the order core published them
(Save before Revert). `verbs_of(prefix)` yields the offers directly under
a prefix (exactly one segment past it), which is what a node card asks for
its own header, and what the agent's readout walks for every node at every
depth. `UiStudioView.offers: UiOfferTree` carries the whole tree for the
view; the project header and every node card, root or nested, publish into
it during the same build that constructs the rest of the view model.

The agent reads this same tree for its readout and presses offers by path:
`act { action: "<path>", why }`. An unknown path is refused with the
current offers listed; a `Lasting` or `needs_user_activation` offer
becomes a card instead of dispatching. There is exactly one tree, so the
web and the agent can never see a different set of buttons for the same
view — the gap that let the agent miss nested Remove/Revert is closed by
construction, not by remembering to walk one more field.

### Migrated surfaces lose their DTO fields

The project header's `ProjectEditorView.header_actions`, and the node
views' `UiNodeView.header_actions` / `UiNodeChild.header_actions`, are
deleted rather than kept alongside the tree. A surface that has moved to
the tree has nowhere else to look, so there is no way for the web and the
agent to drift onto two different sources for the same buttons again.
`UiPaneAction`, the DTO those fields held, is deleted entirely now that
nothing constructs it.

### A second ratchet

`just lint-core-action-fields` (`scripts/check-core-action-fields.py`, in
`check-lint` beside `lint-web-actions`) counts `pub` struct fields on core
view types whose type mentions `UiAction` or `UiActions` (excluding
`core/action/` and `core/offer/`, the types themselves). The count may
only go down, recorded per file in
`scripts/core-action-fields-ratchet.txt`. Where `lint-web-actions` holds
the line against the web building its own actions, this ratchet holds the
line against core re-scattering them into DTO fields instead of the tree —
P2 recorded the drop from 25 to 22 as the project and node headers moved
over.

## Alternatives

- **Uniform action fields on every DTO** (roadmap D1 option b): give every
  surface a `Vec<UiAction>` field in a conventional place instead of one
  tree. Rejected: it still leaves each consumer walking N different fields
  to find everything offered, which is exactly the scattering the parent
  ADR named as the first problem. A single tree with a single walker
  (`iter()`) is what lets the agent's readout and a future palette (M2)
  share one implementation.
- **A fourth, heavier level** for board-wiping verbs (factory reset, wipe
  device), such as a typed-confirm or hold-to-confirm treatment stronger
  than the two-click arm. Deferred: nothing in M1's two surfaces needed it,
  and a fourth level should be justified by a walk finding the two-click
  arm too light, not designed ahead of that evidence. Noted as future work
  in `notes.md`.
- **Typed offer parameters now** (an offer that carries a value to edit,
  e.g. which board to flash). Moved to M3 (Q3): M1's two surfaces are all
  parameterless verbs, and the first real user of a typed parameter is
  flash-which-board, which arrives with the devices-page migration.
  Designing the parameter shape ahead of that first user risked guessing
  wrong.

## Consequences

- A new action on the project header or a node card header is added to the
  tree at construction, with a path; it is visible to the web and the
  agent by construction, not by remembering to wire a second consumer.
- `ActionMeta` call sites that built `destructive`, `confirmation`,
  `confirmation.inline`, or `ActionGesture` all had to migrate in the same
  change (P1); there is no gradual-adoption path for the old knobs because
  they no longer exist as fields.
- **Known gaps, named rather than hidden:**
  - The devices page, add-node menus, playlist, slot editors, and home
    still build actions on DTO fields, not the tree. Only the project
    header and node card headers migrated in M1; the rest is incremental
    work under the roadmap, surface by surface.
  - The blank-board Flash `RowCta` in `device_roster_card.rs` still
    dispatches in one click, ignoring its `Lasting` level, because the
    devices page hasn't moved to the tree-rendering components yet. This
    moves to M3 with the rest of that surface; semantically, flashing a
    *blank* board loses nothing, so M3 may give that instance `Routine`
    instead of carrying the gap forward.
  - The unsaved-edits navigation `confirm()` (`unsaved_gate.rs`) is
    unchanged by this ADR. It is a place concern (M7), not an action.
- The ⌘K palette (M2), place (M7), and the chat window (M5) are explicitly
  out of scope; they will read the same tree once they exist, rather than
  inventing their own.
