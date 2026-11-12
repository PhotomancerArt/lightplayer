# Future Work

## Richer Demand Root Scheduling

- **Idea:** Let demand roots express cadence/readiness such as every frame, when hardware has data, or while pending retries exist.
- **Why not now:** Output and radio can start as every-frame demand roots.
- **Useful context:** `Engine::demand_roots`, `OutputNode`, `ControlRadioNode`.

## Radio Delivery Protocol

- **Idea:** Add ack, TTL, retransmit windows, ownership, and mesh routing semantics.
- **Why not now:** Current work is runtime causality, not delivery protocol design.
- **Useful context:** `ControlRadioNode` pending/recent-message state and `lpc_shared::hardware::RadioMessage`.

## Split Produce And Consume Contexts

- **Idea:** Replace shared `TickContext` with narrower `ProduceContext` and `ConsumeContext`.
- **Why not now:** Reusing the existing context keeps this refactor small.
- **Useful context:** `lp-core/lpc-engine/src/node/contexts.rs`.
