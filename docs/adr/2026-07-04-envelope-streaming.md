# ADR: Envelope Streaming and Progressive Project-Read Apply

- **Status:** Accepted
- **Date:** 2026-07-04
- **Deciders:** Photomancer
- **Supersedes:** [2026-06-27-project-read-event-frames.md](2026-06-27-project-read-event-frames.md)
- **Superseded by:** None

## Context

`2026-06-27-project-read-event-frames.md` made project reads a streaming
operation: a request was answered by one or more same-id `ProjectReadFrame`
messages, each carrying a batch of `ProjectReadEvent` values, terminated by a
frame containing an `End`/`Error` event. That ADR left three seams:

1. **Streaming was a project-read-specific concept.** Finality and ordering
   lived inside `ProjectReadFrame.sequence` and the terminal `End`/`Error`
   event, so the correlation logic that decided "is this request finished?"
   was duplicated per streaming operation. Both client loops
   (`client.rs`, `tokio_client.rs`) reimplemented the same accumulate-frames
   state machine.
2. **The wire response was an aggregate DTO.** `ProjectReadResponse` +
   `ProjectReadCollector` reconstructed a full snapshot on the client, so a
   *gated* (delta) stream — the M5 payoff — could not be applied without
   fighting the collector's whole-snapshot assumptions. Consumers read a DTO,
   not project state.
3. **Probe results were monolithic.** One probe emitted one `Result` event;
   a probe payload larger than the 16 KiB frame budget was a serialization
   error with no split path — the same failure mode the frame budget was
   meant to eliminate for every *other* payload.

Milestone M6 (protocol consolidation) closes all three, and its phase P7
finally flips the live Studio client onto the revision-gated read
(`2026-07-03-revision-gated-project-reads.md`) so the M5 bandwidth win reaches
the app.

## Decision

### E1 — Streaming is an envelope capability, not a project-read one-off

`ServerMessage` gains two fields:

```rust
pub struct ServerMessage {
    pub id: u64,
    #[serde(default, skip_serializing_if = "seq_is_default")]
    pub seq: u32,   // frame number within a stream, from 0
    #[serde(default, skip_serializing_if = "fin_is_default")]
    pub fin: bool,  // final message of the stream (default true)
    pub msg: ServerMsgBody,
}
```

Serde defaults are `seq = 0`, `fin = true`, both skipped when default. A
single-response message therefore encodes **byte-for-byte** as the
pre-streaming envelope (`{"id":..,"msg":..}`) — zero cost on the common path.
Only non-final stream frames pay `"seq":N,"fin":false` (≤ ~23 bytes); the
final stream frame pays `"seq":N` only. `ClientMessage` is unchanged —
requests never stream.

### E2 — One generic collect-until-fin rule

Correlation accumulates messages for a matched id while `fin == false`,
requiring `seq` contiguous from 0 (a gap is a protocol error, the same
strictness the old collector enforced), and completes the request on the
message with `fin == true`. A single-response op is the degenerate case: its
first matched message has `seq == 0, fin == true`. Unsolicited messages
(`id == 0`, e.g. heartbeats) always carry `fin == true`. Both client loops
collapse onto this one rule.

### E3 — `fin` owns finality; `End`/`Error` stay semantic

Envelope `fin` is the **only** signal correlation uses to end a stream. The
domain events `ProjectReadEvent::End { revision }` and `Error { message }` are
kept — they carry meaning the envelope does not (the authoritative final
revision; error text) — and the server guarantees the frame containing
`End`/`Error` is the frame with `fin == true`. The applier (E5) independently
validates the event grammar, so a `fin`/`End` mismatch surfaces as a protocol
error rather than a silent truncation.

### E4 — Body shape and the generic sink

`ServerMsgBody::ProjectReadFrame { frame }` becomes
`ServerMsgBody::ProjectRead { events }`; the `ProjectReadFrame` type and its
`sequence` field are deleted (the envelope now owns sequencing). The budget
constants (`PROJECT_READ_FRAME_MAX_BYTES = 16 KiB` and the derived
serial-margin / runtime-chunk values, M3's single-knob derivation chain) move
to `lpc-wire`'s `budget.rs`. Server-side, the bounded batcher
(`lpc-shared`'s `ProjectReadStreamSink`) measures each event once with the
wire serializer, flushes at budget, and stamps `seq`/`fin` on the envelope —
project reads are its first user, and a second streaming op reuses it.

### E5 — Progressive apply replaces the aggregate

`lpc-view` gains `ProjectReadApplier`, which owns every invariant the deleted
collector enforced — Begin-once, per-query kind/opened/ended state, chunk
reassembly with offset/length checks, terminal detection, End-revision ==
Begin-revision — and applies each event **directly** onto `ProjectView` as
families close. The view's revision advances last, so a mid-stream failure
never claims a new revision. Consumers (Studio, lp-cli debug UI, fw-browser
tests) read state from the view; the aggregate `ProjectReadResponse` /
`ProjectReadResult` / `ProjectReadCollector` (~1720 lines incl. dead
`ClientApi`, `Engine::read_project`, and their tests) are deleted outright.

### Probe identity and chunking

Every probe result variant now carries its subject
(`RenderProductProbeResult::{Unsupported,Error}` gain `product`;
`ExplainSlotProbeResult::{Unsupported,Error}` gain `node`/`slot`;
`ControlProductProbeResult` already did). Positional probe matching
(`requested_product_previews` indexed by position in `project_sync.rs`) is
deleted — results are matched by identity. Bulk probe payloads (control
samples, render textures) stream as chunks keyed by the enclosing
`Probe { index }`: `ResultBegin { byte_length, header }` → N ×
`ResultBytes { offset, bytes }` → `ResultEnd`, sized by the same M3 budget
constants as runtime-buffer payloads. Structured headers (e.g.
`ControlLayout2d`) ride whole in `ResultBegin`.

### Studio flips to gated reads

Studio's refresh sends `since = view.revision`; a fresh or reconnected
session (no trusted mirror) sends `since = 0`/`None` (a full bulk sync). On
any applier protocol error the mirror is discarded (`view.revision` reset to
0) and re-read from `since = 0` — a self-correcting resync, logged at warn.

## Consequences

- **Streaming is transport-generic.** Adding a second streaming server
  operation costs one `ServerMsgBody` variant and reuses the sink, the
  collect rule, and the envelope — no new correlation loop, no new frame type.
- **Steady-state reads are near-empty.** An idle gated refresh at
  `since == R` transfers no shape/slot/node/resource payload — only the
  Begin/Runtime/End spine at `R` (proven by
  `studio_steady_state_read_carries_no_payload` in `lpc-engine`, the
  studio-request analogue of `read_at_since_r_sends_no_payload_items`).
- **No aggregate DTO.** State lives in `ProjectView`; there is one apply path
  and no snapshot-reconstruction step to keep in sync with the stream.
- **Probes cannot exceed the frame budget.** A large probe payload chunks
  like any other bulk payload instead of failing serialization.
- **Self-correcting client.** A malformed or lost delta cannot wedge the
  mirror; the resync-from-0 rule rebuilds it from a full read.
- **The compatibility aggregate is gone**, so the intermediate "clients
  reconstruct `ProjectReadResponse`" consequence of the 2026-06-27 ADR no
  longer holds; that ADR is superseded.

## Alternatives Considered

- **Keep streaming project-read-specific.** Rejected: it perpetuates the
  duplicated correlation loops and blocks any second streaming op from reusing
  the machinery.
- **Delete event-level `End`/`Error`, put finality only in the envelope.**
  Leaner vocabulary, but the authoritative final revision and error text then
  need a new home in the envelope body, and the applier loses the grammar it
  validates against. Rejected in favor of E3 (fin owns finality; End/Error
  stay semantic).
- **Keep the aggregate as a compatibility shim.** Rejected: it cannot apply a
  gated delta stream without re-deriving a whole snapshot, defeating M5.
- **Chunk only probe `bytes`, leave the layout header monolithic.** Accepted
  for now (see Follow-ups); the control display-layout header grows with lamp
  count and is not chunked.

## Follow-ups

- **Layout-header growth escalation.** The `ControlLayout2d` display-layout
  header carries ~a 5-tuple per lamp and is not chunked. At the 241-lamp
  fixture it is comfortably within the 16 KiB budget (guarded by
  `fixture_sized_control_preview_fits_project_read_frame_budget`). If fixtures
  grow ~4×+, split the layout semantically into per-lamp-range events rather
  than growing the transport budget.
- **Sub-root slot patching.** Slots are gated per-root (M5 G6a); sub-root
  progressive patches remain future work.
- **Real-hardware Studio smoke.** The end-to-end gated multi-frame read over
  real serial is now exercisable through Studio for the first time; a
  hardware smoke run should confirm it after merge.
