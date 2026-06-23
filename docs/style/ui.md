# Studio UI Style

Studio UI should be shaped around what the user is doing, not around the
internal shape of the data.

## Less Is More

The default rule is to show nothing we do not have to show.

Every visible label, border, icon, heading, badge, and metric competes with the
thing the user is trying to understand or change. Add UI only when it improves
orientation, comparison, decision-making, or action.

When in doubt:

- prefer fewer labels;
- prefer one strong focal surface over several decorated containers;
- prefer quiet inline text over badges;
- prefer whitespace over panel chrome;
- remove explanatory copy once the interaction itself is clear.

## Avoid Data-Shaped Nesting

Do not add a visual container every time the underlying model has another
object, enum, record, slot root, or view node. Deep nesting makes the UI feel
like a schema browser instead of a tool.

Prefer the shallowest presentation that preserves meaning:

- If a node has one primary produced visual, show the visual directly.
- If a section has one child, do not wrap it in an extra titled box just to
  mirror the data structure.
- If several technical facts describe one thing, prefer a single quiet caption
  over nested labels and badges.

Extra borders, cards, and panels should indicate a meaningful interaction or
separate workspace, not merely another layer of data.

## One Concept, One Frame

A visible frame should usually mean one user-facing concept:

- A node window frames a node.
- A product preview frames the image, control strip, or other produced output.
- A modal frames a temporary focused task.

Avoid putting a framed product inside a framed presentation section inside a
framed node unless each frame has an obvious job from the user's perspective.

## Progressive Technical Detail

Main UI should present the useful surface first. Debug panes, source panes,
inspectors, and tooltips can expose exact internal detail.

For example, main node UI can show:

```text
output visual 128 x 72
```

The debug pane can show:

```text
state.output = ProductRef::Visual(node=8, output=0)
revision = 102
slot root = node.8.state
```

The user can still inspect the system precisely, but the everyday view stays
calm.

## Details On Demand

Technical detail should usually be available, not always visible.

Prefer a small details affordance, inspector, source tab, debug tab, popover, or
tooltip over permanently showing implementation facts in the main surface. A
details affordance is useful when the information is sometimes important but
would distract from the normal workflow.

Good candidates for details-on-demand:

- revision numbers;
- slot roots and exact slot paths;
- internal product references;
- binding resolution details;
- source file locations;
- transport/probe diagnostics;
- raw serialized values.

Main UI can show the edited or observed value. Details UI can show why it has
that value, where it came from, and how the runtime represents it.
