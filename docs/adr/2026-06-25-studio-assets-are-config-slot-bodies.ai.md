# ADR: Studio Assets Are Config Slot Bodies

## Status

Accepted

## Context

The Studio node UI promotes several kinds of slot data into first-class visual
sections. Products and produced values are shown above the config tree, while
asset slots such as GLSL sources need top-level treatment so they can eventually
open richer editors.

The first asset model used a separate `UiConfigAsset` DTO and a dedicated web
component path. That made assets look and behave like a parallel concept even
though they are slots underneath. It also duplicated detail affordance,
expansion, and story coverage that config slots already need.

## Decision

Promoted Studio assets are represented as config slots with an asset body:
`UiConfigSlotBody::Asset(UiSlotAsset)`.

The controller/projection layer may still extract asset slots into a dedicated
node section for layout, but the section items remain `UiConfigSlot`s. Web
rendering uses the same slot row, slot detail button, slot affordance priority,
and expansion behavior as other config slots.

Asset-specific UI belongs inside the asset body expansion. The current web
surface is a compact editor-like `pre` block; richer GLSL, SVG, or resource
editors can replace that expansion without introducing a second asset DTO path.

## Consequences

Node UI code has one config-slot detail model for normal values, records, and
assets. This keeps validation, binding, dirty-state affordances, and story
coverage aligned as the node editor grows.

Asset sections can still be visually prominent, but they should not use a
separate boxed asset panel or compatibility wrapper. If asset editing later
needs additional state, add it to the asset slot body or to web-local editor
state rather than recreating a parallel config asset model.
