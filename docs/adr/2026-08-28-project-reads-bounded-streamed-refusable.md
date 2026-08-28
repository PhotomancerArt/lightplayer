# Project reads are bounded, streamed, and refusable

- Status: accepted
- Date: 2026-08-28
- Plan: `lp2025/2026-08-28-1424-wire-protocol-evolution` (PRs #457, #458)
- Fixes (with the G1 bench walk):
  `docs/defects/2026-08-26-project-read-assembly-oom-resets-classic.md`
- Related: `docs/adr/2026-08-25-classic-uart-io-task-executor-isolation.md`,
  `docs/adr/2026-07-04-envelope-streaming.md`,
  `docs/adr/2026-07-14-wire-hello-versioning.md`

## Context

The classic ESP32 (~70 KB free heap with a dome-scale project loaded)
reset on Studio's initial ProjectRead, every time. The bench empirics
matrix showed every *piece* of the read fits while the monolith never
does — and code reading found the mechanism: value-returning sync query
APIs (whole-registry shape snapshots, whole-forest slot clones,
whole-tree delta vecs) bolted onto a streaming sink one layer too late,
so peak memory was O(project) while the wire only ever needed O(one
item). A "wire v2" was considered and rejected: the protocol already
had chunked multi-frame streams, budgeted flushing, and a per-node
selector; the *usage contract* was the defect. (The full
examined-and-parked alternatives — message-mode lease, seq framing,
retransmit, binary encoding — live in the planning directory's
vision.md.)

## Decision

Four rules, all landed with no breaking wire change (no
`WIRE_PROTO_VERSION` bump):

1. **Producers stream per-atom; nothing materializes O(project).** The
   read pipeline emits one item at a time — one slot root, one shape
   entry, one bounded `TreeDeltas` batch — materialized, sent, dropped.
   The sink flush transfers (`mem::take`) rather than clones its batch.
   The atom is the unit that must fit; the frame budget is the transport
   grain, not the API bound. Enforced by
   `lp-core/lpc-engine/tests/project_read_peak_memory.rs`: a tracking
   allocator streams the Studio-shaped read and asserts both
   held-vs-streamed separation and an absolute peak ceiling — a
   regression to materialize-first fails CI on the host, not on silicon.

2. **An unaffordable read is refused, never a reset.** `LpServer`
   carries an optional embedder probe for the **largest free block**
   (fragmentation decides what is allocatable on a small arena, not
   total free; `fw-esp32v3`/`c6` register
   `recovery::panic_path::largest_free_block`). Below
   `PROJECT_READ_MIN_HEADROOM_BYTES` (32 KiB, provisional until the
   bench walk), the read is answered with the existing terminal
   `ProjectReadEvent::Error` naming the remedy — the connection and the
   engine stay alive. Hosts leave the probe unset and are never
   refused. Alternatives rejected: client-side budget guessing from a
   hello-advertised number (stale the moment a project loads — only the
   server knows its live heap) and fallible allocation at the event
   layer (invasive; the gate at the entry point covers the filed
   failure).

3. **Clients right-size requests: the staged initial sync.** The
   monolithic everything-at-once read is no longer constructible in
   Studio. Initial sync runs skeleton (no slots, no probes) → per-node
   slot detail in `NodeReadSelection::ByIds` pages (the selector was on
   the wire all along; the engine now honors it) → one probe per read.
   Trap, test-pinned: pages must send `since: None`, because the
   per-root revision gate would exclude every root after stage 1
   advanced the mirror's revision.

4. **Loss and pressure are observable.** Every serial drop site counts
   (parse failure, RX error, queue-full, stale-partial flush) into
   `LinkCounters` riding the Heartbeat; `MemoryStats` carries
   `largest_free_block`/`oom_retry_saves`; parse drops WARN with
   evidence; heartbeats are not suppressed while responses stream; read
   limbs stamp OOM-context breadcrumbs. All wire additions are
   **fields** on existing structs — additive-safe both directions.
   **The asymmetry worth remembering: a new field is compatible; a new
   *variant* is not** (an old peer's serde rejects it into the inbound
   drop path), so new verbs must be feature-gated on the hello.

## Consequences

- Peak read memory on the device is O(largest atom) + one frame batch,
  by construction; the host CI exercises the same bounded path (host
  embedders now declare an explicit 1 MiB budget instead of opting out).
- The refusal message is UX-visible text; the 32 KiB threshold and the
  16-node page size are provisional constants the G1 bench walk
  revisits with real heartbeat numbers.
- A read spread over stages spans engine ticks; cross-stage drift is
  reconciled by the existing revision-gated refresh, not by new
  machinery.
- The defect's on-device verdict (previously always-reset full read on
  the bench classic) is owed at the plan's G1 bench walk; the defect
  stays open until then.
