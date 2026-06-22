# ADR: Studio Device UX Workflow

- **Status:** Accepted
- **Date:** 2026-06-22
- **Deciders:** Photomancer
- **Builds On:** [2026-06-21 Studio UX Layer](./2026-06-21-studio-ux-layer.md),
  [2026-06-22 Studio Link Management Workflow](./2026-06-22-studio-link-management-workflow.md)

## Context

The first Studio UX slice exposed separate Link, Server, and Project panes. That
matched lower-level service boundaries, but it did not match the user's mental
model. A user chooses how to connect to a device, waits for that device to open,
waits for LightPlayer firmware/server readiness, and then opens a project. Link
and server are meaningful implementation layers, but they are not usually the
primary product concepts.

Studio also needs to handle blank ESP32 devices, firmware provisioning, reset to
blank, boot logs, progress, and project attach in one discoverable flow. Keeping
Link and Server as adjacent primary panes made those recovery states feel split:
the action lived in one pane while the failure or boot output often lived in
another.

## Decision

Make `DeviceUx` the user-facing controller for the connection workflow.
`StudioUx` owns `DeviceUx` and `ProjectUx`.

`DeviceUx` owns the lower-level `LinkUx` and `ServerUx` controllers internally.
It exposes one semantic `UiStackView` with step sections for:

- select connection;
- connect device;
- connect LightPlayer;
- open project.

Actions are attached to the view section that offers them. A UX node does not
have a separate global action list; actions are part of the presented view.
`StudioUx::actions()` derives the currently available actions from the current
`StudioView`.

Project remains separate from Device. It is hidden until LightPlayer is
connected or the project state is otherwise meaningful. Project is expected to
grow into the node tree, file tree, and project editing surface, so folding it
into Device would mix two product concepts that will evolve differently.

`UiStackView`, `UiStackSection`, and `UiStepState` are reusable UI-independent
view primitives. They are intentionally small and client-side only. They are
used for web rendering, textual agent rendering, tests, and future CLI/desktop
surfaces, not as a serialized client/server protocol.

## Consequences

- The active Studio shell now presents a Device pane first, with Project shown
  only once it is useful.
- Link and Server remain real implementation controllers, but they are no
  longer top-level user-facing panes.
- Blank-device detection, provisioning, reset-to-blank, reconnect, boot logs,
  and LightPlayer server attach appear in one continuous Device workflow.
- Web UI rendering becomes more generic: it renders stack sections, section
  bodies, section-local actions, and terminal output without knowing link/server
  policy.
- Agents and future CLI shells can inspect the same tree and describe both the
  state and the available requests in a product-shaped language.
- Some transitional code still maps Link/Server progress updates into the
  Device pane while lower-level progress emitters are migrated.

## Alternatives Considered

- Keep Link and Server as separate primary panes.
  - Rejected. It preserves implementation boundaries at the cost of a fractured
    user workflow.
- Merge Project into Device.
  - Rejected. Project will become a larger editing/navigation surface, and it
    should remain its own Studio concept.
- Build a highly generic component schema immediately.
  - Rejected. A small stack/body/action vocabulary covers the current workflow
    while leaving room to grow from real use.
- Serialize the UX tree as a protocol.
  - Rejected. This is an in-process client-side model. Textual rendering for
    agents is useful, but Studio already has a real client/server boundary in
    `lpa-client` and `lp-server`.

## Follow-Ups

- Migrate lower-level progress emission so Device actions publish section-aware
  activity directly instead of relying on Link/Server node-id fallback mapping.
- Continue shaping the Project pane around the future project node tree and file
  tree.
- Add more focused tests for Device stack state/action placement as hardware
  workflows settle.
