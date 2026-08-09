# SpaceAnswer2::Default is UI-retired but still in the model

**Status:** open condition (deliberate debt)
**Filed:** 2026-08-09, dimensionality plan-B (post-G1b "Default-tile
redundancy" ruling)

## The condition

`lpc_model::SpaceAnswer2` (a 1D shader's answer for 2D consumers,
`shader_def.rs` → `shader_space.rs`) still declares `Default` as its
default variant, but the studio surface no longer offers it: the
dimensionality section's inline choice tiles filter `"Default"` out of
the `ProducerIn2d` cell's choices
(`lpa-studio-core/src/app/project/node/node_space_section.rs`), so any
pick authors a real shape and nothing can author `Default` back.

Why it was retired from the UI (Yona, post-G1b): "It also feels weird
to have two options 'extrude default' and 'extrude' — why are there two
options?" — after G1 killed "consumer decides", `Default` is
behaviorally identical to authored `Extrude` in every UI-reachable
state: the deferral chain producer-`Default` → consumer-`Auto` resolves
to extrude, the fill-silence rung (`Policy { force: false }`) is
unreachable from the one-dropdown consumer UI, and a forcing consumer
beats both equally. Only the caption ORIGIN differs (`consumer
default` vs `declared`), which the D11-honest captions still surface —
that is why the VALUE must keep existing for unauthored cells.

## Why the model variant stays (for now)

Removing the variant is a FORMAT-BREAKING change, not additive:

- every persisted `"space": { "kind": "OneD", "in_2d": … }` whose cell
  is absent or `"Default"` parses into it (the additive-compat tests in
  `shader_space.rs` / `node_def.rs` pin this);
- removal needs a `PROJECT_FORMAT_VERSION` bump with a migration that
  rewrites `Default` → `Extrude` (behavior-identical, so mechanical),
  plus an example sweep and schema regeneration;
- the default variant is also what an UNAUTHORED cell reads as — the
  card's honest `extrude · default` summary depends on distinguishing
  "never said" from "said extrude" until a replacement signal exists.

## The resolving move

Fold the removal into the next deliberate format bump (with its
migration), or into D15's declared-fixture-space work if that reshapes
the answer cells anyway. Until then: the UI filter in
`node_space_section.rs` and the label fallback in
`space_section.rs::active_variant_label` are the two places that keep
the retired variant rendering honestly.
