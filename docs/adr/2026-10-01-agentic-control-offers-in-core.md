# ADR: Agentic control is a core concept — every user verb is an offer built in core

- **Status:** accepted (2026-10-01, by Yona: "agentic control needs to be a
  low-level concept in the frontend architecture"). The rule and its
  ratchet land now. The design of "offers" is the work of the roadmap
  `lp2025/2026-10-01-1255-agentic-ui-roadmap` and will amend this ADR.
- **Deciders:** Yona
- **Refined by:** `docs/adr/2026-10-01-offer-tree-and-consequence-levels.md`
  (the consequence level, path ids, and the offer tree itself)
- **Evidence:** planning dir `2026-10-01-0126-app-agent-harness/`
  (`design-offers-as-a-core-concept.md`, `p08` Implementation Result);
  PR #888 (the app agent) and #889 (`act`, cards, this ratchet).

## Context

Studio follows the humble-view pattern: state and decisions live in
`lpa-studio-core`, and `lpa-studio-web` renders view models and forwards
`UiAction`s. The app agent (PR #888) is the first consumer of that
contract that is not the web. It sees the app through a readout built
from the view model, and it acts by pressing offered actions (`act`,
#889). Building it showed two ways the contract has drifted:

1. **Core's offers are scattered.** What the user can press on a surface
   lives in several DTO fields: pane actions, the project header's
   Save/Revert, each node card's header at every depth, the add-node menus,
   and the device card's facts. Nothing lists them, so every consumer must
   know every field. The agent's first readout missed the project header,
   and nothing failed.
2. **The web builds actions itself.** `lpa-studio-web` constructs 83
   `UiAction`s across 29 files (57 direct `UiAction::from_op` calls). The
   device card composes its verbs from core decision functions. Platform
   facts such as Bluetooth reach are asked in the web. An action the web
   builds is one no other consumer can see or press.

Nothing caught either: core's end-to-end tests build ops directly rather
than pressing what the view offers. The pattern had no second consumer to
protect it.

## Decision

- **Every verb the user can press is an offer built in core** and
  published on the view model. Each offer carries its `ActionMeta`,
  including the new `gesture` (who may press it: anyone, or only the
  user's own click) and its confirmation. The web renders the offers it is
  handed and builds none of its own. The app agent reads the same offers
  and presses them through the same dispatch. An offer only the user may
  press becomes a card whose click is that same action.
- **A ratchet holds the line while the rework happens:**
  `just lint-web-actions`, in `check-lint` and so in CI. It counts the
  actions the web layer builds, per file, against
  `scripts/web-actions-ratchet.txt`. A file may not build more, and a new
  file may build none. A drop is locked in with `--bless`.
- **The rework is incremental**, surface by surface, under a roadmap. It
  is not one rewrite. The intended shape is in the design note: one offer
  tree with stable paths, typed parameters for offers that take a value,
  and core e2e tests that press offers by path instead of building ops.
  That shape is a proposal, not yet decided.

## Consequences

- New UI code that needs a button adds the action to a core view model.
  Today that is the nearest existing field. Once the roadmap lands, it is
  the offer tree.
- The ratchet will flag refactors that move a web-built action to a new
  file. Bless the move in the same change. The total must not rise.
- The app agent can press only what core publishes. Until the device
  card's verbs move to core, the agent can't connect, flash or push on its
  own; it offers the add slot's USB path as a card. This is accepted:
  shipping the agent is not urgent, and the agent is what drives the
  architecture.
- Not yet guarded: core e2e tests that build ops directly. That second
  ratchet needs offer paths first; the roadmap owns it.
