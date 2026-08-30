---
status: fixed
found: 2026-08-26
fixed: 2026-08-29
area: lpa-server ProjectRead assembly vs classic ESP32 heap; surfaced by PR #448's transport fix
related:
  - ../debt/shared-uart-io-task-starvation.md
  - ../adr/2026-08-25-classic-uart-io-task-executor-isolation.md
  - 2026-08-26-inbound-frames-longer-than-a-tick-lossy.md
---
# Studio's initial ProjectRead OOMs the classic — and an unservable read resets the board

**Shape** — With a dome-scale project loaded (~70 KB heap free), the
classic cannot assemble Studio's monolithic initial sync read. Response
assembly materializes every event before framing; the abort-on-OOM
posture then RESETS THE BOARD mid-read. Studio saw "Syncing project…"
forever (now a loud 12 s failure after the gated-sync fix); the wire
saw a reboot with an OOM recovery breadcrumb (`alloc 92 bytes failed,
free=32`, faulting in `SlotShape::clone` during node-event assembly).

**Empirics matrix** (bench dig2go, project "studio" at ~111 ms ticks,
serial-lab, 2026-08-26 — all on the PR #448 transport, which is not the
problem: C7's 61-frame skeleton stream passes):

| Read shape | Result |
|---|---|
| Full initial (detail+slots, both probes) | RESET, always |
| Detail+slots, ONE probe (either) | RESET |
| Detail+slots, no probes | MARGINAL — passed 4×, then reset on later boots; a few KB of history (fs files, log state) flips it |
| **Detail, `include_slots:false`** | PASS |
| Nodes summary only | PASS |
| Probe-only (output_frame or binding_graph) | PASS |

`include_slots:true` is the dominant cost (per-node `SlotShape` clone
forest); probes stack on top. The monolith is the problem — every
individual piece fits.

**Two defects in one:**

1. **lpa-server**: an unservable read must FAIL (error event / refused
   read), never abort-reset the device. Assembly is infallible-alloc
   today; a budget-aware assembly (or fallible allocation at the event
   layer) is the fix. Note the wire already supports chunked probe
   results and multi-frame streams — the protocol is not the blocker.
2. **Sync architecture**: the client requests everything at once. The
   evidenced fix is a STAGED initial sync — skeleton without slots
   (fits), slot detail in bounded follow-up reads, probes one per read
   (each fits) — which also caps peak device RAM per read permanently.
   This belongs with the device-model rewrite's wire-up milestone
   rather than a patch here.

**Mitigation in place** — Studio's initial sync is deadline-gated
(12 s quiet-gap) and the pull loop logs frame counts, so this fails
loudly with evidence instead of hanging. The classic still cannot sync
this project until (1) or (2) lands.

**Regression probe** — the full-initial-read shape should join
`starvation-bench.py` as an advisory check once a fix direction is
chosen (today it would only document the reset).

**Progress (2026-08-28, wire-evolution round 1 — PR #457 + #458):**
both defects have fixes on the branch; status stays `open` until the
bench walk (G1) delivers the on-device verdict.

1. *lpa-server half*: every whole-project materialization in the read
   path is now per-item streaming — slot roots (#457), the shapes
   registry (which was deep-cloned TWICE per read; the breadcrumb's
   `SlotShape::clone` faulted here), tree deltas (now bounded
   `TreeDeltas` batches), and the sink's per-flush batch clone. A
   host-side allocator probe (`lp-core/lpc-engine/tests/
   project_read_peak_memory.rs`) pins the shape: the Studio-shaped read
   held 130 KB materialized vs 13.9 KB streamed on mini-dome, and fails
   on regression to materialize-first. Unservable reads now REFUSE with
   a terminal error (largest-free-block gate,
   `PROJECT_READ_MIN_HEADROOM_BYTES`) instead of abort-resetting, and
   each read limb stamps an OOM breadcrumb.
2. *Sync architecture half*: Studio's monolithic initial read is no
   longer constructible — the initial sync is staged (skeleton →
   `ByIds` slot pages of 16, `since: None` → one probe per read), each
   stage a PASS-row shape from the matrix above.

**On-device verdict (2026-08-29 G1 bench walk, dig2go classic):**
FIXED. The full monolithic read (detail + slots + both probes) that
previously always reset the board now **completes 5/5** against a
loaded project with headroom (small-dome, 64 KB largest block: 4–6
frames, 78–120 events, ~1 s, zero resets) and is **refused 5/5** in
0.8 s with the structured remedy-naming error against a genuinely
starved board (zook-dome, ~19 KB largest — a heap so tight its own
shader JIT OOMs). Deliberately lowering the gate to 16 KiB re-admitted
the monolith and reproduced the OOM reset — with the new breadcrumb
naming the limb (`alloc 480 bytes failed in "project read: shapes"`) —
so the shipped 32 KiB floor is silicon-calibrated, not guessed. Studio
end-to-end staged sync completed against the live classic (first
successful classic sync ever), surfacing per-node engine statuses.
Residuals tracked separately: probe results still materialize-then-
chunk (ladder #9/#10), and flash writes wedge under multi-wire playback
(`2026-08-29-flash-write-wedges-under-zook-playback.md`).
