# ADR: Project Reads Send Only What Changed

- **Status:** Accepted
- **Date:** 2026-09-23
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Studio's lens re-issues a project read every 150 ms while a lens is open —
the tightest of any wire consumer, on the link that matters most for BLE
(USB serial today, BLE next, the emulator/tab too). The BLE spike (Run D in
`~/.photomancer/planning/lp2025/2026-09-23-1428-ble-remote-control/spike-results.md`)
measured one steady lens read of the PLAYFUL choker (73 LEDs) at ~11.1 KB,
of which:

- ~5.4 KB was resent **unchanged** on every read: the binding-graph
  structure (`bindings` + `channels`, 4,562 B, 16 distinct values across
  570 reads) and `sample_layout` twice — once for the output frame, once
  for the control product (1,214 B each, 1 distinct value ever);
- pixels travelled **twice** (the output frame and the control product
  carry the same 73 lamps) at 16 bits per channel, ~586 B base64 each,
  though the pixels are only ~1.2 KB of the 11 KB — most of the reply is
  structure;
- BLE notify measures 5–12 KB/s (same BLE vision spike), so an 11 KB read
  is one lens update per 1–2 s over BLE even before payload framing
  overhead.

None of this is wire-protocol dishonesty — every byte is a faithful,
independently-decodable answer — it is that a steady read keeps re-deriving
and re-serializing state nothing asked it to change. `lean-wire`
(`~/.photomancer/planning/lp2025/2026-09-23-1501-lean-wire/`) is the pass
that measured and cut this, phase by phase (P1 baseline and tooling, P2 an
unrelated emulator backlog fidelity fix, P3 buffer geometry, P4 the binding
graph, P5 the pixel-copy ask policy, P6 ledger-driven cuts to what remained).
This ADR records the rule the phases converged on and its bandwidth
consequences, so the next probe follows the same shape without re-deriving
it.

## Decision

### The rule: static half under a revision, moving half every read

**Anything static about a buffer or a graph rides a revision gate; values
ride every read.** Concretely, `lpc-wire::project_read::probe::revision_gate`
introduces one generic pair used by every probe with a static/moving split:

```rust
enum RevisionGateRead {
    None,
    Always,
    // Proto 23 (lean-wire follow-ups F1): a list, so the per-output
    // output-frame gate is this same type. `node` names the output; a
    // single-half probe lists at most one entry with no node.
    IfChanged { known: Vec<KnownRevision /* { node?, revision } */> },
}

enum RevisionGateResult<T> {
    Omitted,
    Unchanged { revision: Revision },
    Changed(T),
}
```

A client asks `Always` once, caches `T` and its revision, then asks
`IfChanged` with the revision it holds on every later read. While the revision
holds, the engine answers `Unchanged { revision }` — a handful of bytes —
and only the moving half travels. **One revision covers the whole gated
half**: it moves whenever any piece of it does, and it is never the
engine's per-tick revision (a value that moves every frame would defeat the
gate before it started).

Two probes carry this pattern (P3, P4):

- **Buffer geometry** (`geometry_gate.rs`): a control product's or an
  output's `sample_layout`, `display_layout` and (for outputs) `placements`
  bundle under one revision (`ControlProductGeometry` /
  `OutputFrameGeometry`). A display layout the engine refuses for size
  (`GeometryDisplayLayout::Unsupported { reason }`, the dome-scale case) is
  still geometry — the refusal itself is cached per revision, so a refused
  layout is measured, refused, and cached exactly once, not on every read
  (`docs/defects/2026-08-04-oversized-display-layout-wedges-project-read.md`
  named this loop; P3 closes it, see the amendment below).
- **Binding graph** (`docs/adr/2026-07-06-binding-graph-probe.md`, amended
  by P4): `bindings` + `channels` gate on a content-hashed structure
  revision (FNV-1a 64 of the structure's own wire bytes); each channel's
  resolved value travels every read, positionally, against the structure
  revision it was resolved at.

P6 extended the same instinct without a formal gate type where a gate
would have been the wrong shape: a project's slot-root state (`node.<id>
.state`) now rides only when its content changes (a stamp-free hash), not
on the tree's own per-tick revision stamp, and Studio's read request stopped
asking a `resources` summary query nothing displayed. Both are cuts of the
same shape as the gate — resend only on change — applied where the payload
was a value, not a probe.

### The one-copy, U8 ask policy

Two questions are conflated in "how many bytes does a preview cost": how
many **bits per sample**, and how many **copies of the same lamps**. P5
answered both as a Studio-side *ask* policy, not an engine capability
change — the engine already supported `U8`
(`WireChannelSampleFormat::{U8, U16}` predates this plan); nothing asked
for it.

- **8-bit by default.** Every live preview asks for
  `PREVIEW_SAMPLE_FORMAT = U8` (`frame_feed/preview_sample_format.rs`).
  Samples stay linear on the wire; Studio's decode widens back to u16
  (`round-trip via ×257`, not a lossless one) before the sRGB transfer, so
  the *screen* draws the same picture it always did, at half the wire
  bytes per sample. `CLOSE_INSPECTION_SAMPLE_FORMAT = U16` is kept as a
  named constant for a future surface that inspects raw samples rather
  than shows them; nothing asks for it yet, and asking is what would cost
  the second precision, not the constant's existence.
- **One copy of a given set of lamps per read, by default.** The PLAYFUL
  choker's 73 lamps are fully covered by the primary control product *and*
  the output it is patched to — before P5, Studio asked both, at U16, on
  every read. The ask policy (`ProjectController::always_live_products`,
  `output_lamp_coverage.rs`) picks **one** by default and only asks for the
  second on an explicit request:
  - a device lens defaults its one copy to the **output frame** (root
    module picture, play mode, the output patch bay, fixture patch rows) —
    the output frame is what the ADR on output-frame semantics already
    treats as canonical, and it is the copy every one of those surfaces
    wants regardless of which fixture is selected;
  - a fixture's own preview is **not live** by default once every one of
    its lamps sits on a streaming output — it shows "Show live" instead,
    and clicking it (or selecting the fixture) is the explicit ask that
    streams the second copy;
  - a **device lens opens on the root module selected**, not on the
    fixture the project happens to default to. This was a deliberate
    correction mid-implementation: an automatic fixture selection is still
    a selection, and AC4 promises the second copy only on an *explicit*
    request — a lens that streamed two copies before the user clicked
    anything would have quietly broken that promise on every device lens
    open. The automatic root-module selection asks for nothing extra; the
    first user click is the first explicit ask.
  - sim lenses are excluded from this ask policy (no cable, so no wire
    bytes to save) and keep their existing default.

  This is a client-side ask policy, not server-side deduplication — the
  engine still answers each probe independently and does not notice that
  two probes would carry the same bytes. That noticing is future work (see
  Alternatives).

### Measure with the tap and the read-size test

Two instruments, both landed in P1, are how every number in this ADR and
in `_measurements.md` was produced, and are the tool for any future
wire-size claim (`AGENTS.md` names the tap recipe near the emulator
commands):

- **The deterministic host oracle**
  (`cargo test -p lpc-engine --test lens_read_wire_size -- --nocapture`,
  `lp-core/lpc-engine/tests/lens_read_wire_size.rs`) builds Studio's exact
  lens request against the host engine and sizes each frame with the wire
  serializer — no emulator, no board, byte-identical across repeated runs.
  It is what the ratchet consts (`CHOKER_CEILING`, `SMALL_DOME_CEILING`)
  gate on, and it is exact.
- **The live wire tap** (`LP_EMU_WIRE_TAP=<dir> just studio-dev-emu`, then
  `just wire-tap-stat <dir>/c6-a.tap --ledger --skip-seconds 25`) records
  the real bytes a real Studio session puts on an emulated C6's link, and
  `tapstat.py`'s ledger breaks a read down by JSON path with a
  distinct-value count per path — the tool that found, in order, the
  binding-graph churn (P4), the resend loop wherever `distinct == 1` across
  hundreds of reads (P1, P6), and the state-root stamp-vs-content gap (P6).

The oracle is exact and reproducible; the tap is what a real session
actually sent, and its "how many times did this value change" column is
what the oracle alone cannot show. Neither instrument alone was enough to
find every cut this plan made. Emulated **byte counts are exact**; emulated
**rates** (KB/s, reads/s) are the emulator's own timing model and are
reported as such, never claimed silicon-equivalent (`AGENTS.md`,
"Emulator-first device validation").

### Numbers

| Read | Before (baseline.md) | After | Cut |
|---|---:|---:|---:|
| Choker lens, steady | 10,997 B (oracle) / 11,094 B (tap median) | 2,369 B (oracle) | 78% |
| Choker lens, first | 12,078 B | 9,951 B | 18% |
| small-dome lens, steady | 128,544 B (11 frames) | 28,556 B (2 frames) | 78% |
| small-dome lens, first | 130,555 B (11 frames) | 77,670 B (7 frames) | 40% |
| Dome scale (25,000 lamps), from small-dome steady | ≈509 KB/read | ≈113 KB/read | 78% |

The choker's steady read meets AC1 (≤3 KB) with headroom; the saving holds
at small-dome scale (AC2). See `_measurements.md` for the full before/after
table, the live-tap corroboration, and the BLE-rate estimate.

## Consequences

- Every future probe with a static/moving split reaches for
  `revision_gate::{RevisionGateRead, RevisionGateResult}` rather than
  inventing its own cache-and-diff idiom.
- A client that wants two copies of the same lamps (say, a future
  side-by-side output-vs-fixture view) pays for the second copy explicitly,
  by asking — the wire has no notion of "the same bytes twice" to exploit
  automatically. That exploitation is future work, not foreclosed by this
  decision.
- `WIRE_PROTO_VERSION` moved once for the whole plan (20 → 21, P3), and
  every fallout site the bump touches (manifests, emulator hello pins, the
  reference-client walk test, transcripts) was walked in `_measurements.md`
  under "wire-bump fallout" rather than repeated per phase.
- Persisted formats did not change: every gated or ask-policy type lives in
  `lpc-wire` as a wire-side wrapper; the `lpc-model` types they wrap
  (`ControlSampleLayout`, `ControlDisplayLayout`) kept their serde impls.
- The dome-scale refusal loop this ADR's geometry gate closes has its own
  defect record (`docs/defects/2026-08-04-oversized-display-layout-wedges-project-read.md`);
  this ADR is the follow-up that stopped the refusal from being recomputed
  every read, not a reopening of that defect.

## Alternatives Considered

- **Binary encoding now.** Ruled out at the start of the plan (R1, Yona,
  2026-09-23): "this is not about binary, its about leaning the data in
  general. binary is a later question." A follow-on spike (`spike/ion-wire`,
  `~/.photomancer/planning/lp2025/2026-09-23-1528-ion-wire-spike/findings.md`)
  measured a compact Ion-inspired binary format (LPBJ) at 2,874 B for the
  *pre-send-less* choker reply and modelled 1,024 B layered on top of this
  plan's send-less structure — real headroom, not a substitute for it, and
  it is sequenced as its own plan (`lp-json-pack`,
  `~/.photomancer/planning/lp2025/2026-09-23-1701-lp-json-pack/plan.md`)
  after this one, so its dictionary is generated from the shapes this plan
  leaves behind rather than shapes about to move again.
- **Server-side pixel de-duplication.** The engine could notice that two
  probes' buffers hold identical bytes and answer one by reference. Deferred
  (D1 follow-on, Yona): correct in principle, but it is a second mechanism
  layered on top of the ask policy above, and the ask policy alone removes
  the second copy in every case this plan measured (a fixture whose lamps
  are fully covered by a streaming output). Worth revisiting once a UI
  surface actually wants two copies of the same lamps live at once.
- **Compression** (e.g. a stream-level deflate). Not attempted here: the
  cheapest bytes to cut are the ones a probe should never have re-sent in
  the first place, and a general-purpose compressor spends flash and CPU
  fighting an already-JSON, already-repetitive-on-purpose wire shape rather
  than fixing why it repeats. The Ion spike's own evidence keeps this
  question live for later (a +4 KB window deflated the *pre-send-less*
  choker transcript 17.9×, which says most of that redundancy was exactly
  the resend loops this plan removed structurally) — but spending flash on
  a compressor is a binary-encoding-adjacent decision and stays with that
  later question (R1), not this one.
- **A per-request explicit copy count / query parameter**, instead of a
  default-plus-explicit-ask policy: rejected as needless API surface. The
  existing selection and "Show live" affordances (PR #786) are already the
  explicit-ask signal; adding a second knob would let a client ask for the
  same thing two ways.

## Follow-ups

- `lp-json-pack` (binary encoding, R1's later question) is scoped and
  sequenced after this plan; its dictionary generation depends on the wire
  shapes this ADR describes staying put.
- Server-side pixel de-duplication (above) is future work if a UI surface
  ever needs two live copies of the same lamps at once.
- The heartbeat (546 B / 5 s, ~50 B/s) was measured and left alone (P6):
  `identity`, `recovery` and `link` are read by Studio and by
  `lpa-devices` evidence, gating any of them needs per-connection heartbeat
  state, and a shape change there needs silicon transcript re-captures for
  a saving that is two orders of magnitude below the lens's own rate. Not
  worth it at this plan's scale; revisit if BLE's own budget ever makes the
  heartbeat material.
- ~~Tree deltas (368 B, every read: `entry_changed` fires because every
  engine call takes a runtime out and puts it back with
  `set_state(.., frame)`) were investigated and left alone (P6)~~ —
  **resolved** by the lean-wire follow-ups plan's F3 (see the 2026-09-24
  amendment below): content-stamped `entry_changed`, with the mid-read
  status stamped past the served revision so
  `a_declared_space_mismatch_is_repaired_by_declare_space_end_to_end` stays
  green without the per-frame re-bump this follow-up used to need.

## Amended 2026-09-24 (lean-wire follow-ups, #804)

`~/.photomancer/planning/lp2025/2026-09-23-2120-lean-wire-followups/plan.md`
(F1–F3) closed three of this ADR's own follow-ups: the per-output revision
gate's own request shape, the U8 ask policy's darks trade-off, and the tree
deltas follow-up above. One more wire bump, `WIRE_PROTO_VERSION` 22 → 23 (21
→ 22 was this same plan's F1; 22 was taken by the BLE M3 access-core plan
landing first).

### One list-shaped revision gate (F1)

The output-frame probe used to carry its own per-output gate type
(`OutputFrameGeometryRead` / `KnownOutputFrameGeometry`), separate from the
single-half `RevisionGateRead` every other gated probe used. F1 deleted it:
`RevisionGateRead::IfChanged` now carries `known: Vec<KnownRevision>`
(`KnownRevision { node: Option<NodeId>, revision }` —
`lp-core/lpc-wire/src/messages/project_read/probe/revision_gate.rs`), and
the output-frame probe lists one `KnownRevision` per output it holds, naming
the node; every single-half probe (a control product's geometry, the
binding graph's structure) lists at most one, with no node. The engine
answers every probe through the same `RevisionGateRead::holds(node,
revision)` check. This is the "one gate, one or many gated halves" shape
this ADR's Decision section already named as the target; F1 is what made it
literally one Rust type instead of two answering the same question. A
steady single-half request grows ~10 B (`known: [{"revision":N}]` vs the old
`known_revision: N`), but requests never rode the read-size ratchet, and one
request shape is also one deserializer on the device — 1,584 B of ESP32-C6
flash recovered. `#[inline(never)]` on the two largest probe bodies,
`Engine::read_project_output_frame_probe` and
`read_project_binding_graph_probe`, recovered another 7,738 B: both are
inlined into the project-read `async fn`, and through it into the
firmware's `tick_and_send` state machine more than once, so each was being
monomorphized/copied at every one of those call sites. The pair is
load-bearing together — either alone saved only 194–728 B — so the doc
comment on `read_project_output_frame_probe` warns not to drop one without
re-measuring (`just fw-esp32c6-size-check`). Between the two changes, F1 won
back 9,328 B of the 14,064 B lean-wire itself had spent, headroom
714,848 B → 724,176 B.

### Srgb8 previews, the shared codec (F2)

Studio's live previews now default to `WireChannelSampleFormat::Srgb8`
(`PREVIEW_SAMPLE_FORMAT = Srgb8` in `frame_feed/preview_sample_format.rs`),
not the linear `U8` this ADR's "one-copy, U8 ask policy" section described.
Linear `U8` is unchanged as a wire format — it is still what a buffer
*published* at 8 bits carries — but nothing asks Studio to display it
raw any more; `CLOSE_INSPECTION_SAMPLE_FORMAT = U16` is unchanged. This
closes the trade-off P5's own U8 ask policy left open: 8-bit-linear halves
the wire bytes per sample but crushes shadow detail, because a linear ramp
spends most of its 256 codes on highlights. On the PLAYFUL choker (60 s at
brightness 0.2, the darkest part of a typical scene), Studio's on-screen
darks measured:

| | U16 | linear U8 | Srgb8 |
|---|---:|---:|---:|
| distinct on-screen levels | 125 | 52 | **125** |
| lowest on-screen steps | 0, 1, 2, 3, 4 … | 0, 13, 22, 28, 34 … | **0, 1, 2, 3, 4 …** |
| samples differing on screen from U16 | — | 63.2% | **0%** |

Srgb8 costs the same bytes as U8 (one byte per sample) but recovers all 125
of U16's on-screen levels by spending those 256 codes on an sRGB-shaped
curve instead of a linear one — exactly the curve the display already uses,
so decoding it back to linear and re-encoding for the screen is a lossless
round trip in practice (0% of samples differ from what U16 would have
shown). One codec serves both the wire and the engine's own render-texture
preview path: `lpc-wire::{linear16_to_srgb8, srgb8_to_linear16}`
(`lp-core/lpc-wire/src/project/srgb8_sample_codec.rs`), replacing the
engine's separate 4,096 B `srgb8_lut.rs` (which was within 1 LSB of exact,
not exact) — texture previews are now correctly rounded as a side effect.
Net firmware cost: +2,800 B (the 4 KB LUT left, 766 B of new encode tables
plus the new match arms came in), headroom 724,176 B → 726,976 B.

`lp-fw/fw-browser/src/texture_convert.rs` still builds its own 64 K f32
lookup table for the in-browser sim's sRGB conversion — a second copy of
the same transfer function, host/wasm-only. It is proven to agree with the
wire encoder bit-for-bit (`lamp_view`'s test compares the two across all
65,536 inputs), so it is a known duplicate rather than a correctness risk.
Left alone: unifying it would mean `fw-browser` taking a dependency on
`lpc-wire`'s codec (or extracting a third crate) for a host-only path that
costs it nothing on any device image, so it stays out of scope here as F2
originally scoped it.

### Content-stamped tree deltas (F3)

The tree-deltas follow-up above is resolved: `entry_changed`'s
`change_frame` is no longer the per-tick stamp every take/put-back of a
node's runtime used to re-bump — new `TreeEntryStamps`
(`lp-core/lpc-engine/src/engine/tree_entry_stamps.rs`) hashes what a delta
would actually carry (status, plus wire state with `Executing` read as
`Alive`) and keeps one `ContentStamp` per entry, refreshed once per read
right before the tree streams. A steady read now sends no `entry_changed`
for an unchanged entry at all.

The mid-read case — a render probe compiling a shader and setting `Error`
status *after* the read has already sent its tree for revision `R` — is
what broke every earlier attempt at this cut (P6's note above). F3's fix is
a fence: `TreeEntryStamps` remembers the newest revision a refresh has
`served`. A change a refresh finds is stamped `now` only when no read has
served `now` yet; otherwise it is stamped `served + 1`, one revision ahead,
so the client's next `since` (which is `R`) is older than the stamp and the
change is guaranteed to arrive on the next read instead of being silently
absorbed into a revision already sent. This is the same "one revision
ahead" idea `state_root_stamps` already used for P6's slot-root gating,
applied to tree deltas. With the fence disabled, both of its own unit tests
fail, and `a_declared_space_mismatch_is_repaired_by_declare_space_end_to_end`
is the end-to-end proof it still holds: a render probe compiling mid-read
at revision 12 has its `Error` land on the *next* read at that same
revision, not the one it compiled during.

Studio's own agent verdict wait was rewired off this stamp rather than kept
coupled to it: `AgentEngineStatus.revision` is now the synced
`ProjectView::revision` of the read the status is as of, not the entry's
`change_frame` (which, now that it only moves on a real change, would never
advance for an edit that leaves the node `ok`). The bridge's
`pre_stage_revision` became a `VerdictFence` (`Open | FirstRead { staged_at
} | SecondRead { first_read }`): the verdict is read from the read *after*
the first one past the stage, because that first read's tree can go out
before its own render probe has compiled the edit. This costs one extra
lens read (~150–250 ms) inside the wait's 1,500 ms budget, and is correct
by construction rather than by poll timing. No wire shape changed for any
of this — `WIRE_PROTO_VERSION` did not move for F3.

Read sizes (`lens_read_wire_size`, the same oracle this ADR's Numbers table
used), after all of F1–F3:

| Read | This ADR's "After" | After F1–F3 | Cut |
|---|---:|---:|---:|
| Choker lens, steady | 2,369 B | **2,003 B** | 15% more |
| Choker lens, first | 9,951 B | 9,588 B | 4% more |
| small-dome lens, steady | 28,556 B (2 frames) | **27,968 B** (2 frames) | 2% more |
| small-dome lens, first | 77,670 B (7 frames) | 77,085 B (7 frames) | 1% more |

Firmware: F3 cost 464 B (the stamps, the refresh walk, two FNV
instantiations), headroom 726,976 B → 726,512 B. Total across F1–F3 against
the ebf63d463 baseline this ADR was written against: +8,864 B of headroom
recovered, most of the shape this ADR already described, at smaller bytes.

### Wire-bump fallout (F1, proto 22 → 23)

Same fallout list this ADR's own Consequences section names, walked again
for this bump: the four `manifest-core.expected.json` files, the
`usb_attached.rs` / `usb_control.rs` hello pins, `hello.rs`'s proto history.
No walk `.script` file or committed transcript under `lp-emu/esp/*/walks/`,
`lp-emu-validate/walks/` or `lp-emu/transcripts/` embeds the old
`known_revision` shape or a request naming `u8`/`srgb8` sample formats —
confirmed by grep, not just by the diff being empty for those paths. The
handful of transcripts that do carry a `"sample_format"` field
(`lp-emu/transcripts/esp32c6/{upload-walk,meteor-walk-usb,shader-oracle-walk}/*.txt`)
are pre-lean-wire historical captures (commits `d6cfaa205`, `681ea97ca`,
dated 2026-09-07) all reading `"u16"` — untouched by this plan's ask-policy
or gate-shape changes, and, as this ADR's own P7 fallout pass already
established for the same files, replayed by `m4_replays.rs` / `m5_replays.rs`
/ `m6_replays.rs` as historical facts about those old captures, never
against a live current build. `lp-cli/tests/emu_serve_walk.rs`'s pinned
proto-20 reference client is unaffected by any of F1–F3's wire changes for
the same reason P7 recorded: it is built from a fully detached historical
commit. The host-side heap-budget ratchet (`scripts/heap-budget-record.json`)
was re-baselined for the same reason P7 re-baselined it for lean-wire
itself: the new wire types (`RevisionGateRead`/`Result`, `KnownRevision`,
the `Srgb8` variant, `TreeEntryStamps`) reach the host engine's own
project-load path, moving `project-load.{retained,largest_alloc,alloc_bytes}`
by 100–200 B and every stage's `largest_free_at_close` by ~128 B across all
three catalog fixtures — the same shape of drift, not a new one. The chip
halves of the heap-budget ratchet were left to CI's `LP_EMU_BUILD_FW=1`
jobs, which build the firmware images this workspace gate deliberately does
not build itself.
