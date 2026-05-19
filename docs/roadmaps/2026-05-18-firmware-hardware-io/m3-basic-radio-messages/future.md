## Radio Nodes And Event Semantics

- **Idea:** Add node/runtime integration that consumes radio channel messages and turns them into LightPlayer events or control values.
- **Why not now:** This plan only makes the firmware provide the radio capability by default; node design is the next step.
- **Useful context:** `lpc-shared::hardware::RadioDevice`, `HardwareSystem::open_radio...`, and the M3 single-consumer channel API.

## Button-To-Radio Default Behavior

- **Idea:** Broadcast physical GPIO button events over the default radio channel from normal firmware.
- **Why not now:** Without node/event semantics, hardwiring button behavior into firmware would bake in product behavior too early.
- **Useful context:** M2 button input types and M3 `RadioMessageKind::ButtonPress`.

## Multiple Radio Consumers

- **Idea:** Add a root-owned fanout or broker so multiple runtime features can subscribe to radio channels.
- **Why not now:** Roadmap decision says the first radio API is single-consumer.
- **Useful context:** Revisit when radio nodes, server diagnostics, and any future sync layer all need concurrent access.
