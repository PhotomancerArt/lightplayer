# P5 — Sim multi-node probing

Size: sm. Depends on: P1 (canvas previews make many previews cheap to
render) and P3 (pacing keeps many-probe pulls from starving the UI).

## Scope

On sim sessions, probe the products of **all non-collapsed nodes**; device
sessions keep today's focused-node + always-primary-visual behavior.

Out of scope: wiring `ProjectProductSubscriptionIntent` to a user-facing
op/UI toggle (the enum stays as the durable per-node override seam);
display-driven probe sizing; any wire changes.

## Current state

- `lp-app/lpa-studio-core/src/app/project/project_controller.rs:1783-1789` —
  `node_subscribes_products`: `Default => self.is_focused_node(node)`;
  `Subscribed`/`Unsubscribed` intent variants exist
  (`node/node_controller.rs:17-27`) but nothing in production sets them.
- `:1791-1816` — `subscribed_products()` walks the tree via
  `collect_subscribed_products`, then always unions the primary visual
  (ADR 2026-07-16-primary-visual-product).
- Visibility proxy: `NodeControllerState.collapsed`
  (`node_controller.rs:32`).
- Runtime kind reaches studio-core policy the same way as cadence
  (`RefreshCadence::for_kind`, `refresh_cadence.rs:93-98`); P4 already
  threads kind to probe-frame selection — reuse that plumbing.

## Implementation

1. Thread the session runtime kind into the subscription policy (however P4
   threaded it for probe sizing — keep one mechanism).
2. `node_subscribes_products` `Default` arm becomes:
   - Sim: `!node.state().collapsed` (a collapsed node's children: follow the
     tree walk — if `collect_subscribed_products` recurses into children of
     collapsed nodes, decide whether collapsed subtrees are hidden in the UI;
     if their cards are not rendered, exclude the whole subtree and say so in
     the phase result).
   - Device: `self.is_focused_node(node)` (unchanged).
   - `Subscribed`/`Unsubscribed` intent still overrides both.
3. Keep the primary-visual union unconditional (both kinds).
4. Bandwidth sanity: N nodes × ~4.2 KB per visual probe per pull at 32×32 —
   fine in-memory on sim; no cap needed now, but if a project in the test
   fixtures has pathological node counts, note observed pull sizes in the
   phase result.
5. Tests: extend the existing subscription tests
   (`project_controller.rs:3506,4705-4710` show the intent-based tests) with
   kind-aware cases: sim probes non-collapsed + skips collapsed; device
   unchanged; intent overrides still win.

## Conventions

- This is policy code in studio-core, not wire code; no lpc-* changes
  expected at all.
- Tracking badge derivation (`node_controller.rs:609-622`) reads the same
  intent — verify the UI badge still makes sense when sim auto-subscribes
  (the `Tracking`/`Paused` states derive from focus today; adjust the
  derivation if it would now mislabel, and note it).

## Validation

- `cargo test -p lpa-studio-core`, `just check`.
- Story/visual: a sim story with two unfocused nodes should now show live
  previews on both (if a story fixture covers this, great; otherwise verify
  live and note it — Yona's PR-review sim check covers the feel).

## Agent reminders

Do not commit unless asked. Do not expand scope. Do not suppress warnings or
disable tests. Stop and report if blocked. Report changes, validation, and
deviations.

ADR: node-scope policy covered by P6 ADR. Review gate: quick sim visual
check, batched with PR review.

## Definition of done

Sim sessions probe all non-collapsed nodes' products; device behavior
byte-identical to before; intent overrides respected; tests cover the
matrix; checks green.

## Implementation Result

Status: done
Completed: 2026-07-27
Commit: e287c3d5d

- Changed: `node_subscribes_products` `Default` arm is runtime-kind-aware
  (sim → `!collapsed`, device/unknown → focused); lens kind reaches
  `ProjectController` via `set_lens_runtime_kind` at the P4 chokepoints;
  tracking badge takes the real subscription decision via a `subscribes`
  closure through the DTO build chain.
- Validated: test
  `sim_lens_subscribes_unfocused_nodes_device_stays_focused_only`;
  `just check` + `just test` green.
- Deviations: none, but note `collapsed` is never true in production (web
  collapse is view-local), so sim probes ALL nodes until the ui-state-audit
  re-homes collapse state — commented at the policy site and recorded in the
  ADR. Details in [handoff.md](handoff.md).
