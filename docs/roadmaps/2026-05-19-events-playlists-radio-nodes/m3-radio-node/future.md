# Future Work

## Ack And Retransmit

- **Idea:** Add optional acknowledgements and resend-until-ack windows for selected radio event
  channels.
- **Why not now:** Requires per-peer identity, pending windows, timeout policy, and failure
  semantics that are larger than the first fyeah event bridge.
- **Useful context:** First slice uses fixed repeated broadcast plus `(id, seq)` dedupe in
  `ControlRadioNode`.

## TTL And Rebroadcast

- **Idea:** Let nodes rebroadcast received events with a TTL/hop budget to improve range.
- **Why not now:** Rebroadcasting without TTL or seen-origin policy can flood symmetric projects.
- **Useful context:** M3 explicitly says `ControlRadioNode` does not rebroadcast remote events or its
  produced `output`.

## Ownership And State Sync

- **Idea:** Add ownership or lease rules for stateful distributed values.
- **Why not now:** The fyeah sign behavior is event-driven and self-healing because playlist entries
  return to idle.
- **Useful context:** Do this after the playlist/radio event path is proven on hardware.

## OSC-Shaped Messages

- **Idea:** Extend `ControlMessage` toward OSC-style address and args.
- **Why not now:** The immediate path only needs trigger identity. Address/args would complicate
  no_std payload encoding and shader materialization.
- **Useful context:** Roadmap notes in `../notes.md` capture OSC as the long-term conceptual shape.
