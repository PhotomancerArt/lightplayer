# Defect registry

A durable record of defects worth remembering. ADRs record decisions;
defects record failures. Where an ADR captures "we chose X among
plausible alternatives," a defect entry captures "the system did Y when
it should have done Z, and here is the mechanism" — so the same
mechanism is recognized the next time it dresses up in a different
symptom.

Entries live in this directory, one dated file each:
`YYYY-MM-DD-slug.md`, dated by when the defect was **found**.

## The filing bar

File a defect when at least one of these holds:

- It **reached a user or a hardware walk** — someone observed the
  failure outside a test run.
- It **revealed a contract or model gap** — the bug is evidence that
  two components disagree about an interface, or that the domain model
  conflates things it shouldn't.
- It **produced (or should have produced) a regression test** — if the
  fix deserved a named test, the failure deserves a record; if coverage
  was impossible, that gap is itself worth recording.
- The **lesson outlives the fix** — the entry would change how someone
  writes the next feature, not just how they read this diff.

Fix-forward trivialities — typos, off-by-ones caught in review, build
breakage — stay commit messages. The registry is for defects whose
*shape* recurs.

Write the entry **at fix time, riding the fix commit**: the same change
that fixes a qualifying bug adds its entry (and updates the index
below). `status: open` entries are legal and expected for
found-not-yet-fixed defects — hardware-walk and live-debugging findings
get a home immediately, before anyone decides when to fix them.

## Entry template

```markdown
---
status: fixed          # open | fixed | wontfix
found: YYYY-MM-DD      # how: hardware-walk | live-debugging | ci | e2e | report
fixed: <commit>        # absent while open. NOTE: an entry cannot cite
                       # its OWN commit (the hash doesn't exist yet, and
                       # amending changes it) — write `fixed: this change`
                       # at commit time and fill the real hash in the NEXT
                       # commit that touches the registry.
area: <crate/module>
class: <one from the vocabulary>
related: []            # other defects, ADRs, plan dirs
---
# <one-line title>

**Symptom** — what was observed, verbatim error text included.
**Root cause** — the mechanism, not the patch.
**Fix** — what changed and where (the commit is the diff; this is the shape).
**Regression coverage** — named tests, or "none: <why>".
**Lesson** — one paragraph; what this implies beyond the fix.
```

## Class vocabulary

Every entry carries a `class` — the failure's mechanism, not its
surface. The vocabulary is extensible: add a class when a defect
genuinely fits none of these, and define it here in one line.

- **`backend-contract-divergence`** — two implementations of one
  contract disagree on details only real hardware surfaces.
- **`lifecycle-ownership`** — two layers both believe they own a
  resource's lifecycle.
- **`partial-knowledge-loss`** — an error path discards facts already
  learned.
- **`policy-leak`** — one context's policy applied in another.
- **`assumed-context`** — code presumes state instead of asking the
  source of truth.
- **`state-conflation`** — one state models two different facts.
- **`stand-in-divergence`** — a stand-in (placeholder, mock, fallback)
  meant to be equivalent to what it replaces diverges in a dimension the
  substitution didn't model.
- **`inline-emit-stack-imbalance`** — a code-emitter leaves the operand
  stack unbalanced, and a downstream construct hides it from validation.
- **`untested-path`** — a variant of a fixed bug survives in a sibling
  code path the fix and its tests never reached.
- **`stale-measurement`** — a cached measurement outlives its validity
  because the events that invalidate it aren't all observed.
- **`budget-exhaustion`** — a hard resource budget is enforced only by
  a tool outside CI, so growth crosses the limit silently and the wall
  surfaces on whoever builds next.
- **`ungated-variant`** — a build configuration no gate ever compiles,
  so upstream API drift accumulates in it invisibly until someone
  reaches for it.
- **`nondeterministic-capture`** — a capture records one of several
  reachable renderings, because the thing being photographed had not
  reached a single settled state.
- **`config-masked-defect`** — shared code is correct only under
  incidental properties of the *one* configuration that exercises it, so
  no test can falsify it until a second configuration arrives.
- **`split-source-of-truth`** — one fact is derived two ways from two
  sources, both derivations are used, and nothing checks they agree.
  Each producer can have a passing test asserting its own opinion; what
  is untested is the *hand-off*. Note this classes the **defect**,
  whereas `config-masked-defect` classes the **masking** — the two are
  orthogonal axes and a given entry may sit on both (see the note below).
- **`unsynchronized-shared-artifact`** — two steps share a filesystem
  artifact, but the lock that would order them is scoped narrower than
  the artifact, so a reader observes a writer's intermediate state.
- **`opt-in-degradation`** — an absent or unusable input is modelled as a
  legal degraded *value* rather than an error, so the intended graceful
  behaviour (skip, fall back, no-op) holds only for consumers that
  explicitly interrogate it. Consumers that do not get the raw failure,
  worded as the subject's fault and surfacing at first use rather than at
  the point of substitution. The population of callers grows; the guard
  does not.
- **`model-conflation`** — a model represents two things the real system
  keeps separate as one resource, so they contend for something that on
  hardware they never share. Presents as a *capacity* failure in the units
  of the conflated resource, which is why it invites resolutions that all
  preserve the wrong model (make it bigger, split it, shrink what goes in
  it). The diagnostic question is not "how do we make it fit" but "why are
  these in the same place here when they are not on the device".
- **`invented-encoding`** — a binary format's numbering (an instruction
  encoding, a relocation type) is inferred from the shape of its
  neighbours instead of read from the spec, and the invented value
  collides with, or is renamed from, a real one. Toolchain output is the
  only falsifier, so the collision surfaces whenever a compiler first
  emits the stolen form — which is usually at some optimization
  threshold nothing in the suite crosses.
- **`incomplete-subset`** — a normative instruction/opcode subset is
  assembled by enumerating what an external tool (objdump, an
  assembler) is believed to assign, and the enumeration silently omits
  a member that tool actually does assign — so the gap survives
  exactly as long as nothing exercises that one member. Real toolchain
  output (a sequence transcription, not memory) is the only falsifier.
- **`retired-surface-still-reachable`** — a surface believed replaced is
  still rendered, because its replacement can be absent and the old
  surface is the fallback branch.
- **`unenforced-test-precondition`** — a test depends on a condition it never
  establishes (a scheduling order, a timing window, an invocation count), so
  the nondeterminism decides both outcomes: sometimes the assertion fails
  spuriously, sometimes it passes having exercised nothing. The silent half is
  the expensive one. The fix shape is to hold the nondeterminism rather than
  hope — throttle the adversary by *quantity of work*, never by elapsed time
  (a time budget is the same defect wearing a control knob), establish the
  precondition by construction, then check an invariant that would be false if
  the intended interleaving never occurred.
- **`reclaim-ordered-behind-its-own-rebuild`** — a resource is released to
  make room for a transient, but the releasing component rebuilds it
  earlier in the same pass than the transient runs, so net reclaim at the
  moment that matters is zero and the release only adds a second
  allocation. The design's load-bearing claim is about *ordering*, and the
  tests around it pin the protocol (it fired, output is unchanged) rather
  than the ordering, so a mechanism that does nothing passes everything.
- **`timeout-scoped-to-sub-phase`** — a bound named for the whole operation
  actually guards one phase of it, so any *other* phase can wedge forever
  behind an option the caller reasonably believes covers them. The name is the
  defect: the flag advertises the operation, the code scopes it to the step it
  was written next to. Presents as "my timeout did nothing", and the fix shape
  is to bound the command and let sub-phases refine, never the reverse.
- **`lock-held-across-foreign-latency`** — a lock taken to guard a local
  invariant is held across work that invariant does not cover (a network
  round trip, a user prompt), so its hold time is set by an unrelated
  latency budget. Presents as contention that is real but meaningless,
  and it is worse when the refusal is *also* a user-facing claim ("open
  in another tab"): the product then states something false. The fix
  shape is to snapshot under the lock and do the foreign work outside it.
- **`fixed-budget-over-variable-work`** — a fixed elapsed-time budget
  bounds an operation whose duration is set by something outside the
  process (a network fetch, a shared device queue), usually because the
  budget was sized for a smaller operation and the work grew into it. The
  timeout then reports the environment rather than a fault, and fires
  hardest exactly when the system is already slowest. The honest bound is
  on *progress* — a phase that has not advanced in N seconds — with the
  variable-length step reporting itself.
- **`newest-only-inflight-memory`** — state meant to recognize the
  completions of a pipeline's own in-flight async operations remembers
  only the most recent one, so an older operation's completion reads as
  external input and triggers the external-change path. Latent while
  operations complete faster than they are issued; a scheduling hop
  added anywhere in the pipeline (a deferred queue, a render-cycle
  bounce) turns it routine. The fix shape is a queue of everything still
  in flight, never a bigger window on a single slot.
- **`deadline-margin-by-accident`** — a real-time deadline is met only by
  incidental margin (cache warmth, a light workload, a small image) rather
  than by construction, so unrelated growth elsewhere crosses it silently
  and the failure is quantised at one specific deadline, every time. The
  fix is placement or budget, verified by measurement, not by attributes.
- **`toolchain-miscompile`** — a compiler between the validated IR and
  the hardware drops or rewrites a construct that is legal at every layer
  above it; nothing host-side can see it, only a device run of the exact
  construct. The fix is a shape the compiler handles and a device test
  that holds the shape.
- **`unclassified`** — an open defect whose mechanism is not known yet;
  reclassify it when the cause is found.
- **`contract-gap`** — content is authored against a guarantee one tier
  enforces (a fuel meter, a bound, a trap) and a second implementation of
  the same interface never enforces it, so the same input is safe on one
  tier and lethal on another. Unlike `backend-contract-divergence` the two
  do not disagree on a detail — one of them simply has nothing where the
  other has a guard, and nothing in the interface says which. The fix
  shape is to make the guarantee part of the interface: enforce it on
  every tier, or refuse the input on the tier that cannot.
- **`wait-on-a-lossy-signal`** — a wait's exit condition is a signal sent
  on a channel that may drop it (a best-effort log line, a datagram), so a
  lost signal turns the wait's ceiling into its duration. Presents as a
  test or walk that passes but sometimes takes its whole budget, which
  under a generous ceiling is a timeout somewhere else. The fix shape is to
  key the wait on a reliable message, or on a fact the lossy channel cannot
  carry away.
- **`arena-retained-transient`** — a value that is transient by intent
  (an intermediate of a recursive build, a type copied "for convenience"
  onto every node) is pushed into an arena whose lifetime is the whole
  pass, so the working set scales with references × payload size instead
  of with the result. Presents as an allocation failure far larger than
  the input could justify.
- **`shared-namespace-collision`** — two independent producers mint
  identifiers from the same namespace onto one shared channel, and a
  consumer correlates on the identifier alone, so one producer's frame
  satisfies the other's pending request. Disjoint bases (start your ids
  high) are the usual mitigation and are the trap: they make the
  collision rare rather than impossible, so the defect presents as flake
  landing on a different test each time. The fix is a shape check — ask
  whether the frame *could* answer this request before asking whether
  its number matches.
- **`write-ordering`** — a single batch of writes to one store contains
  both a delete and a persist that resolve to the same key, and applying
  the batch in a fixed within-batch order (every persist, then every
  delete) lets the delete win over a persist for the identity the same
  batch just wrote — discarding a record the batch itself believed was
  current.
- **`bound-in-a-foreign-unit`** — a collection is trimmed by a bound in one
  unit (elapsed wall time, bytes) while it is appended in another (one entry
  per loop iteration, per event), so its size is bounded only by the
  incidental exchange rate between the two — and is unbounded the moment
  they decouple. Presents as a cost that scales with a quantity nothing in
  the code names, and is worst when the trim is written as a per-element
  removal from the front: enforcing the bound is then O(n²) in exactly the
  size nobody was watching. The fix shape is to make the feed and the bound
  share a unit, or to retire the expired prefix in one operation.
- **`open-path-wait-without-wakeup`** — a wait for a resource assumes its
  owner (the world, another process) will eventually supply the event
  that ends it, but the resource is actually owned by the waiting program
  itself, so nothing outside the wait can ever produce the wake-up and
  the hold is indistinguishable from "any second now".
- **`unexplained-transient-stall`** — a request or transfer goes quiet for
  tens of seconds with the process still alive (heartbeats keep arriving)
  and then either resumes or expires; reproduces intermittently and the
  arresting mechanism has not yet been named. Filed for visibility and
  recurrence-tracking, not as a diagnosed defect — the entry should be
  reclassified the day a mechanism is found.
- **`fidelity`** — a stand-in for real hardware (an emulator, a model)
  matches every case a committed transcript checks, but implements a
  coarser mechanism than the real boundary it stands in for — a wholesale
  restore standing in for a narrower one, say — so a corner nothing yet
  exercises silently diverges from what the real system would do there.
  Not a wrong answer today; a gap named before something depends on the
  answer it would give.
- **`dense-over-monotonic-ids`** — storage indexed by position with an id
  that is never reused is sized by every id ever minted, not by what is
  alive, so removal leaves a tombstone that is never reclaimed. Harmless
  while removal is rare; a leak the day a feature makes removal routine.
- **`capped-store-without-eviction`** — a store with a hard cap is fed
  by an identity that multiplies on its own (one per origin, per install),
  has no rule for what leaves when it is full, and its refusal at the cap
  is swallowed — so it fills on a schedule nobody chose and then quietly
  stops accepting anyone new.
- **`wake-quantum-throttle`** — a task that services a stream takes a
  fixed quantum (one packet, one line) per scheduler wake, so throughput is
  bounded by how often it is woken rather than by the link, and anything
  that makes wakes rarer (a longer frame, a slower emulator) turns into
  latency proportional to message size.
- **`absence-from-incomplete-search`** — a search over a candidate set
  that never contained the answer reports "not found", and the result is
  recorded as a fact about the thing searched for, not about the search.
- **`rounded-measurement-at-threshold`** — a measurement precise enough
  for a report is compared against a threshold it is not precise enough
  for, and it was rounded to the threshold's wrong side, so a value exactly
  at the threshold fails it. The tell is a refusal that misses by less than
  the measurement's own granularity (65,535 B against a 65,536 B floor).
- **`two-clocks`** — two sides of one channel each trust their own clock
  (a baud rate, a revision counter) instead of a value the other side
  actually confirmed, so one side's bytes are real and correct and the
  other still misreads them — the mechanism is a disagreement about time,
  not a corrupted value.

## Index

Grouped by class, because a class that keeps recurring is the
model-smell signal: one `backend-contract-divergence` is a bug, two in
a week is an argument for a conformance suite. When a class accumulates
entries, say so out loud — that is an architecture finding, not a
bookkeeping fact.

Saying it out loud: **`config-masked-defect` took five entries on
2026-07-30**, all in `lpvm-native`, all latent for the entire life of the
rv32-only era, and all made observable within hours of the Xtensa corpus
landing. The finding is not "the allocator had bugs" — it is that a
single-configuration test suite cannot falsify configuration-dependent
code, however large it is (31,587 rv32 cases did not). The mitigation is
a second configuration that overlaps where the first is disjoint, which
is what the Xtensa targets now are.

Four of the five are in the shared register allocator. The fifth — the
integer div-by-zero trap — is worth separating, because it says the class
is not confined to `regalloc/`. That one is in *lowering*, and the
incidental property it leaned on was not a register layout but a
**hardware semantic**: RV32M defines `x / 0` and `x % 0`, so emitting the
bare divide was correct on rv32 for free. Its falsifying test also already
existed — the corpus has pinned that contract for as long as it has
existed; what was missing was a backend to run it against, plus
documentation that told the backend author the guard obligation was
somebody else's. The generalizable rule: when a contract is satisfied for
free on the reference target, that is exactly when it must be stated as an
obligation behind a named capability hook, because nothing in the code
will ever remind you it was a choice.

Saying it out loud again, one axis over: **`split-source-of-truth` and
`config-masked-defect` are describing the same 2026-07-30 defects from two
directions.** `xtensa-sret-pointer-clobber` (`FuncAbi::allocatable` computed
the withheld register, `RegPool` ignored it) and `jit-sret-return-count-zero`
(`ret_count` from the IR, `is_sret` from the ABI) are the *same bug twice* —
one fact, two derivations, no check on the hand-off. In both, the producer had
a passing unit test asserting its own opinion; neither had a test on the
consumer honouring it.

They are filed under different classes because the existing entries are classed
by *how they survived* (register-layout accident vs a `cfg` boundary no host
test can cross) rather than by *what went wrong*. That is a real distinction
worth keeping — but it splits a recurrence across buckets, which defeats the
point of grouping. If a fourth of these lands, collapse the axes: class by the
disagreement, record the masking mechanism as a field.

A sharper sub-lesson from the fourth: three of the four were the *same
invariant* — a call-boundary register transfer must behave as a parallel
move — applied at three of its four sites. Each fix was correct and
under-scoped. When a fix establishes an invariant, enumerate every place
it applies before closing the entry; here that enumeration was one
sentence (arguments in, returns out; registers and stack).

**The 2026-08-01 entry moves the masking axis off the ISA.**
`xtlpn-f32-loses-writes-to-value-parameters` is the same shape — shared code
whose fast path was safe only for the configurations anyone ran — but what
masked it was the **frontend**, not the register layout: Naga copies parameters
into fresh locals, `lps-glsl` reuses the parameter's own vreg, and only the
second shape can make a lowering shortcut read a stale copy. So the mitigation
generalizes past "a second ISA": the falsifying configuration is the *product*
of the axes a compile is parameterized by (frontend × ISA × float mode), and a
target that exists in the matrix but not in the suite is a configuration nothing
can falsify. It needed all three axes at once, and it was found within hours of
the combination first being registered as a target.

**2026-08-05 makes it two on the frontend axis, pointing the other way.**
`generated-palette-header-dies-on-naga` is the mirror of the entry above: there,
Naga's parameter copying masked a defect only `lps-glsl` could expose; here,
`lps-glsl`'s native `sampler2D` masked one only Naga's textual bridge could
expose. Two entries, one axis, opposite directions — which retires the idea that
either frontend is the reference the other is checked against. They are each
other's blind spot, so a contract carried by both (textures, uniforms, anything
in the shared header) is untested until it runs through both, and the cheap
mitigation is to parameterize the *existing* suite by frontend rather than to
grow a second one. Worth watching: if a third lands, the argument stops being
"register the target" and becomes "the frontend axis belongs in the default
matrix".

**2026-08-09 puts `backend-contract-divergence` at five, and the two open
ones rhyme.** `q32-native-vs-wasmtime-last-bit` and
`gpu-render-pass-floors-the-fragment-center` are both *tier-seam
convention* divergences whose guard is a tolerance rather than a pin — a
last-bit bound in one, a mean-divergence bound in the other — and in both
the tolerance was calibrated while the divergence was present, so the
suite enforces "no worse than the defect" rather than the convention
itself. The pattern to watch: wherever two tiers implement one rendering
contract, the exact conventions (coordinate handed to the entry, rounding
of the final channel value) deserve identity tests with known expected
values; statistical diffs are for the arithmetic in between.

**2026-09-01 names a shape that had already produced three entries under
three different classes.** `silent-black-under-node-quarantine` (this one,
`state-conflation`: `Error`/"Running" each modeled two facts as one) is the
same underlying mechanism as `classic-rmt-open-fault` (`misattributed-
symptom`, 2026-08-01), `boot-compile-oom-crash-loop` (`silent-drop`,
2026-08-07), and `shader-jit-compile-transient-starves-classic-heap`
(2026-08-29, closed 2026-09-06 by a silicon bracket) — in every one, an OOM at a compile safe point read
as something else entirely (an RMT fault, a vanished board, a heap-starved
device, a healthy card) because nothing between the recovery ledger and the
eyes looking at the result carried a typed "fault" signal. They are classed
differently because each names *how* the symptom was misread rather than
*why* the underlying signal never arrived — which is the same split noted
for `split-source-of-truth`/`config-masked-defect` above. Four in a month,
now with a fix (`docs/adr/2026-09-02-fault-is-never-black.md`): watch whether
a fifth still lands somewhere the new `Fault` status and pattern don't reach.

| Class | Date | Entry | Status | Area |
| --- | --- | --- | --- | --- |
| assumed-context | 2026-10-09 | [a-new-boards-card-never-asked-for-its-picture](2026-10-09-a-new-boards-card-never-asked-for-its-picture.md) | fixed (this change) | lpa-studio-web board card: the picture lease was decided once, at mount, and a new board's card leases nothing; the home page keeps the same card as the board turns Online (adoption keeps its handle), so it never asked for a picture. Found by `walk-ble-emu`'s card step. The lease follows the card's presence now |
| unclassified | 2026-10-09 | [the-editor-never-opens-on-an-emu-serve-usb-board](2026-10-09-the-editor-never-opens-on-an-emu-serve-usb-board.md) | **open** | project sync over an `lp-cli emu serve` USB board: `walk-drop-emu`'s editor step waits forever on "project sync failed: protocol error: expected project read frame seq 1, got 0", with today's card and with the board card alike; the tab backing and Bluetooth open the editor. Found by the board card's walk pass (P09) |
| partial-knowledge-loss | 2026-10-09 | [a-phones-bluetooth-link-leaves-the-choker-under-the-read-gate](2026-10-09-a-phones-bluetooth-link-leaves-the-choker-under-the-read-gate.md) | **open** | lpa-server's 40 KiB total-free read gate × a connected central's ~17.5 KB on the C6 × the card feed: with Yona's iPhone on Bluetooth the choker had 40.6 KB free as a read arrived (2.8 KB over the line even on a fresh boot), so every read was refused (`free 40636 B … needs 40960 B free`) while the card said only "Waiting for the first frame…" and Studio dropped the board's reason. Not fragmentation: the largest block (23 KB) cleared its 8 KiB floor |
| state-conflation | 2026-10-08 | [the-device-play-address-loses-play](2026-10-08-the-device-play-address-loses-play.md) | fixed (this change) | lpa-studio-web lens sync (`web_app.rs` × `router.rs`): "this `/device/` address must heal" also threw away its view, because the heal wrote the lens's own route, which always reads non-play. A `/device/<uid>/play` load landed on `/p/…?on=mac:…`, out of play. No walk ever loaded one. The heal keeps the device route's view now (`router::lens_sync_target`, a test per row) |
| assumed-context | 2026-10-08 | [shader-edits-over-wi-fi-are-refused-board-memory-busy](2026-10-08-shader-edits-over-wi-fi-are-refused-board-memory-busy.md) | fixed (#1047 the gate, #1057 the wire; silicon re-check owed) | fw-esp32-common `request_refusal` × Studio's `ReplaceBody` edit: the request gate asked for a block of 3/4 of the message (the base64 rule), but a shader edit is a JSON byte array (~3.5 chars a byte) that decodes into a block of its byte count, so a Wi-Fi-fragmented C6 refused a 7 KB edit needing 2 KB (`largest block 5216 B … a 6380 B block`). The gate reads the block off the request's shape; wire 41 sends the body as text (choker edit 7,134 → 2,239 B) |
| silent-drop | 2026-10-08 | [a-lan-request-past-8-kb-goes-unanswered-on-a-fragmented-heap](2026-10-08-a-lan-request-past-8-kb-goes-unanswered-on-a-fragmented-heap.md) | **open** | lp-link `Inbox::grow_partial` × the C6 LAN link: past 8 KB, reassembly's next growth needs the old buffer and a new one at once; on a fragmented heap the message is dropped as oversize and never answered, so the host waits out its deadline |
| budget-exhaustion | 2026-10-08 | [a-recompile-on-a-fragmented-wi-fi-heap-resets-the-board](2026-10-08-a-recompile-on-a-fragmented-wi-fi-heap-resets-the-board.md) | **open** | lpc-engine shader compile on the C6 over Wi-Fi: a compile needs one ~2.5–3× source block (6–7 KB) and ~30 KB of room, all infallible; with Wi-Fi's ~27 KB gone and edits' kept objects in the free tail, it OOMs (`requested=6028 … largest_free=5568 … context=shader node: compile`) and resets |
| write-ordering | 2026-10-08 | [littlefs-rust-cross-directory-rename-cut-loses-the-next-entry](2026-10-08-littlefs-rust-cross-directory-rename-cut-loses-the-next-entry.md) | **open** | littlefs-rust 0.1.0 `rename` across directories: a clean power cut in the rename's last ops, then a mount, loses the source directory's NEXT entry (`/.lp` with `access.json`); found by the storage testbed (lp-nor-sim), pinned by a test there; same-directory renames unaffected; no product path renames today |
| lock-held-across-foreign-latency | 2026-10-07 | [a-usb-host-on-the-board-closes-its-bluetooth-update-link](2026-10-07-a-usb-host-on-the-board-closes-its-bluetooth-update-link.md) | fixed (ed15e1091) | lpa-update `BackupSession`/`ServeConfig::BLE` × fw-esp32-common `release_radio_holders`: a Bluetooth backup's 4 KiB read-back answers held the board's one shared frame buffer, so every reply waited on the air (heartbeat frames 2.2–3.6 s) and a slow central or a USB host got the link closed mid-backup (`reply deadline`). Over Bluetooth the backup now reads 1016 B pieces that fit the link's own send ring; an unanswered piece is asked again after 20 s |
| assumed-context | 2026-10-07 | [a-reconnected-link-refuses-the-backup-before-its-login-lands](2026-10-07-a-reconnected-link-refuses-the-backup-before-its-login-lands.md) | fixed (81077deee, 6f3fc621b) | lpa-update `UpdateDriver` × the engine's channel-3 tier: a reconnected Bluetooth link's `G`s beat Studio's channel-1 login, were refused `N`/`A`, and the driver stopped `NeedsEngineLogin`; the board's `[OTA] refused` printed nothing (`-Zfmt-debug=none` blanks `{:?}`). The driver now waits up to 30 s for the login on any later link, and refusals log in words |
| lifecycle-ownership | 2026-10-06 | [core-only-drops-the-wifi-controller-and-bluetooth-goes-dark](2026-10-06-core-only-drops-the-wifi-controller-and-bluetooth-goes-dark.md) | fixed (this change) | fw-esp32c6 `split_boot` core-only branch: destructuring the boot dropped the ESP-NOW driver, which owns the Wi-Fi controller; its drop deinitializes Wi-Fi and, in coexistence, took Bluetooth off the air — core-only logged "advertising" and no central ever saw it (M7 pre-walk, fixture C6). Core-only keeps the driver |
| untested-path | 2026-10-06 | [a-typed-password-unlock-cannot-log-in-to-core-only](2026-10-06-a-typed-password-unlock-cannot-log-in-to-core-only.md) | fixed (this change) | lpa-studio-core update host credentials: only held keys, and a Bluetooth unlock with a typed password installs none, so the core-only half's own login was refused (`LoginRefused`, board left core-only). Remembered passwords now follow the keys |
| untested-path | 2026-10-06 | [the-card-holds-finishing-after-a-refused-login](2026-10-06-the-card-holds-finishing-after-a-refused-login.md) | **open** | lpa-studio-core update standing × the remembered miss: after `LoginRefused` the card keeps "Finishing the update… 0%" with no offer and no word about a password |
| assumed-context | 2026-10-06 | [a-bluetooth-update-never-opens-the-reconnected-link](2026-10-06-a-bluetooth-update-never-opens-the-reconnected-link.md) | fixed (this change) | lpa-devices `UpdateActivity` × `Roster::attach_link`: a board reset over Bluetooth is a GATT drop; the reconnect's sweep attaches a NEW, closed link, which only Identify opens, and Identify does not spawn on a device the Update holds, while the gap's knock never opens a Bluetooth link (P3: it would fight the provider's loop). The update sat at "Attached — not listening" until the gap ran out (`walk-ota-ble-emu`'s first runs). The knock now opens a closed Bluetooth link attached after the gap began |
| stand-in-divergence | 2026-10-06 | [ble-emu-did-not-model-a-board-reset-as-a-gatt-drop](2026-10-06-ble-emu-did-not-model-a-board-reset-as-a-gatt-drop.md) | fixed (this change) | lpa-studio-web `virtual_bluetooth.js`: the emulated board's USB link survives a chip reset, so `?ble=emu` showed an update's resets as lp-link resets inside one connection, where a board's radio drops. On a board SYN after its link carried data, the polyfill now reads the registry's reboot count and drops the GATT connection on a reboot (`resetDrops`), keeping the board's byte channel open |
| partial-knowledge-loss | 2026-10-06 | [a-restore-on-a-pending-link-narrates-to-no-terminal](2026-10-06-a-restore-on-a-pending-link-narrates-to-no-terminal.md) | **open** | lpa-devices `PendingLinkView` × `UpdateHost` narration: a core-only board's no-click restore runs on its pending link, which draws no terminal, and the card it becomes starts a fresh one, so the restore's reconnect times and rates are never shown (USB and Bluetooth; `walk-ota-ble-emu` engine-less) |
| unexplained-stall | 2026-10-06 | [the-io-task-goes-silent-for-2-s-under-paced-lan-traffic](2026-10-06-the-io-task-goes-silent-for-2-s-under-paced-lan-traffic.md) | **open** | fw-esp32c6 recovery watchdog × `lp-io` × LAN traffic: under a host-pacing patch for the virtual LAN (catch-up variant, 3 of 3 runs) an emulated C6 serving `link rtt lan:` logged `[RECOVERY] io task silent > 2000 ms`, halved its frame rate, reset its USB host link, and one request took ~3 s; not in the unpaced run. Cause not named (the shared priority-1 port lock, an undrained USB IN endpoint, or the wait itself); a candidate for silicon |
| unexplained-stall | 2026-10-07 | [emu-run-elf-with-reboot-on-reset-does-not-come-back-after-a-reset](2026-10-07-emu-run-elf-with-reboot-on-reset-does-not-come-back-after-a-reset.md) | **open** (undiagnosed) | lp-cli `emu run --elf … --reboot-on-reset` × lp-emu-esp32c6 `restart()`: found by the Wi-Fi relay cell (PR #1019, P9). After a reset the board printed `rebooting into strap app` and then no console for 400 s; the cell boots twice over a `--flash` file instead |
| fidelity | 2026-10-06 | [an-emulated-boards-clock-outran-its-lan-host](2026-10-06-an-emulated-boards-clock-outran-its-lan-host.md) | fixed (this change: an explicit pace, `--pace realtime\|max` / `pace=`; unset = realtime while a LAN host is connected) | lp-emu-esp-common `seam::net` (`SharedLan`) and the USB door: an idle emulated board's guest clock ran ~8× a wall-clock host's, so a 1.2 ms host round trip was ~10 ms of the board's and its lp-link probe/resend timers fired on frames TCP had delivered (74–95 resent per LAN upload; the host drops them as duplicates). Holding the board to a connected host's clock cut it to 9–14; the residue is the board's Nagle × the peer's delayed ACK (firmware/preset). A set pace is in the label (`…@pace=realtime`) and never records. USB over `serial:tcp://` the same shape (54 per upload), paced only through the LAN |
| wait-on-a-lossy-signal | 2026-10-07 | [the-uart-soak-waited-on-a-log-line-its-own-faults-lose](2026-10-07-the-uart-soak-waited-on-a-log-line-its-own-faults-lose.md) | fixed (this change) | lp-cli `emu_uart_link` soak: after the rounds it waited for the last shader compile's `compilation succeeded` log line, a best-effort channel-2 record the soak's own faults can drop. On every PR image's byte path one was dropped, so the wait ran its whole 120 s budget with the board rendering (12 min of CI, past the v3 job's timeout). It waits on the heartbeat now, a resent proto message |
| rounded-measurement-at-threshold | 2026-10-06 | [a-wifi-joined-c6-refuses-every-project-switch](2026-10-06-a-wifi-joined-c6-refuses-every-project-switch.md) | fixed (this change; silicon re-check owed) | lpa-server's 64 KiB load gate × first-fit placement: with Wi-Fi joined the C6's largest block after a stop was 38–58 KB (FC6 re-check, N7), while a choker's largest ask is 8 KB; and a project that loads then runs out on its first frame passed the gate and reset the board every boot. No gate: loads are tried and recovered across the reset (RTC load record until 3 frames, previous project back with a plain-words notice, a failed startup load not retried) |
| assumed-context | 2026-10-07 | [the-emulated-c6-cleared-rtc-fast-memory-on-every-reset](2026-10-07-the-emulated-c6-cleared-rtc-fast-memory-on-every-reset.md) | fixed (this change) | lp-emu-esp32c6 `restart()` restored LP SRAM on an HP reset, so the firmware's RTC recovery region never survived one: no crash reported, no interrupted load known. LP SRAM kept on an HP reset, cleared on a power cycle |
| reclaim-ordered-behind-its-own-rebuild | 2026-10-06 | [a-refused-project-switch-leaves-the-board-dark](2026-10-06-a-refused-project-switch-leaves-the-board-dark.md) | fixed (this change) | lpa-server `handle_load_project`: a switch unloads (and an upload stops) the running project before the 64 KiB load gate asks, so a refusal left the LEDs dark until a reboot (G1 desk walk, N7; on main too). A refused or failed load restores what the last unload stopped |
| fixed-budget-over-variable-work | 2026-10-06 | [the-first-join-after-boot-gets-no-address](2026-10-06-the-first-join-after-boot-gets-no-address.md) | fixed (this change; silicon re-check owed) | fw-esp32c6 `station_task` × smoltcp's DHCP `discover_timeout` (10 s) = the join policy's address timeout (10 s): a first DISCOVER lost to the WPA handshake was retried just as the policy gave up, so boot to address was ~22 s on silicon (G1). DHCP retries every second |
| assumed-context | 2026-10-06 | [a-silent-lan-socket-holds-the-one-slot](2026-10-06-a-silent-lan-socket-holds-the-one-slot.md) | fixed (this change) | fw-esp32-common `expire_unauthenticated`: on an open board every link holds a tier from its first frame, so a LAN socket that never handshook was never expired and held the C6's one slot (PR C's walk: frames in 0 out 534). A link whose session never comes up is closed at the deadline |
| assumed-context | 2026-10-06 | [a-card-row-shows-through-its-own-popover](2026-10-06-a-card-row-shows-through-its-own-popover.md) | fixed (this change) | lpa-studio-web `base::popover`: the top-layer trigger copy was skipped only when a clamped panel covered the whole trigger; a card row is wider than the panel, so the Wi-Fi row drew over the network list (Yona's G1 walk). Skipped on any overlap |
| timeout-scoped-to-sub-phase | 2026-10-06 | [a-stalled-radio-link-holds-the-tick-past-the-watchdog](2026-10-06-a-stalled-radio-link-holds-the-tick-past-the-watchdog.md) | fixed (this change) | fw-esp32-common `LinkMuxTransport::release_radio_holders` × the 8 s RWDT: the wait for a radio link to let go of the shared frame buffer was 5 s per link, on top of whatever the tick did, so a project load plus a stalled peer reset the emulated C6 on every redial (PR C's walk, 12 resets). Each wait is its link's deadline (Bluetooth 5 s, LAN 1 s) cut by the tick's 5 s budget |
| budget-exhaustion | 2026-10-06 | [a-message-the-heap-cannot-reassemble-resets-the-board](2026-10-06-a-message-the-heap-cannot-reassemble-resets-the-board.md) | fixed (this change) | lp-link `Inbox::push_fragment` × fw-esp32-common `decode_client_payload`: an 8 KB write (`lp-cli link rtt`'s default) reset an emulated C6 with a project loaded, first growing its reassembly buffer infallibly, then copying its base64 text and decoding the blob with no heap check; on silicon a 10 KB write with three links open reset the board (`alloc 10242 bytes failed`). Reassembly fallible, no text copy, a request gate and an fs-read gate that refuse "board memory busy" |
| lifecycle-ownership | 2026-10-06 | [a-redialled-lan-link-leaves-the-card-not-listening](2026-10-06-a-redialled-lan-link-leaves-the-card-not-listening.md) | fixed (this change) | lpa-studio-core `device_effects::port_is_gone` × lpa-link `BrowserWebsocketLink`: a LAN socket's `wi-fi link lost` closed the link but was not read as a departure, and the page's session redialled before any sweep saw it leave, so the card stayed `Attached — not listening · quiet` on a board holding a live session (PR C's emulated walk, W6/W9). The pump detaches on it now |
| newest-only-inflight-memory | 2026-10-06 | [a-scan-asked-during-the-status-read-is-dropped](2026-10-06-a-scan-asked-during-the-status-read-is-dropped.md) | fixed (this change) | lpa-studio-core `NetworkController`: a scan asked while the board's status read was out (or before the first) returned early and was never asked again, so the Nearby list stayed empty now and then (PR C's emulated walk). The scan is owed and the read's answer sends it |
| budget-exhaustion | 2026-10-06 | [a-lan-link-strands-the-heap-below-the-load-floor](2026-10-06-a-lan-link-strands-the-heap-below-the-load-floor.md) | fixed (this change) | fw-esp32-common `LinkMuxTransport` × lpa-server `AccessState` × fw-esp32c6 LAN endpoint: the first LAN link grew the mux's and the access state's per-link lists above its own session (first fit), splitting the main tail when it closed; an open link held a 14.5 KB session plus a 6.9 KB learned table; the endpoint's buffers were placed at the first address. An emulated joined C6 refused every LAN upload, and two links open during a load ran the compile out of memory (PR C's walk). Lists reserved at boot, window 2, LAN replies JSON, buffers at boot, one LAN slot |
| rounded-measurement-at-threshold | 2026-10-06 | [the-largest-block-probe-reads-a-64-kib-hole-as-65535](2026-10-06-the-largest-block-probe-reads-a-64-kib-hole-as-65535.md) | fixed (this change) | fw-esp32c6/s3/v3 `largest_free_block` × lpa-server `check_load_headroom`: the bisection stopped within 16 B and returned the lower edge, so the C6's 64 KiB second heap region read as 65,535 B and a board whose largest hole was exactly the project-load floor was refused (`load refused: heap headroom too low (largest free block 65535 B < 65536 B)`, PR C's emulated Wi-Fi walk). Exact now (`fw_esp32_common::largest_block::largest_fitting`); fixes main too |
| state-conflation | 2026-10-06 | [an-unnamed-board-was-told-its-files-were-missing-and-lost-update](2026-10-06-an-unnamed-board-was-told-its-files-were-missing-and-lost-update.md) | fixed (this change) | lpa-studio-core `device_layout_view` × lpa-studio-web `device_roster_card`: "mounted, no uid" was read as "came back without its files", but Studio no longer stamps `/.lp/device.json`, so a board that was never named (loose-c6, carried across by `lp-cli hardware lpfs migrate`) was told "This board needs its files back", and the restore row replaced the firmware row, so it had no Update. The claim now needs evidence (`fs: formatted`, or a backup pending for the board in this browser, which keeps W7b), and the card draws Update beside the restore verbs |
| stand-in-divergence | 2026-10-06 | [the-bundles-ota-files-are-shadowed-by-the-firmware-lookup](2026-10-06-the-bundles-ota-files-are-shadowed-by-the-firmware-lookup.md) | fixed (this change) | lp-cloud-server router × the Studio bundle: #996 put the bundle's update files at `firmware/<target>/ota/…`, three segments deep, where lightplayer.app's `/firmware/{target}/{release}/{file}` route answers first — `ota` is a reserved word, so a 404 — and the Pages smoke's plain static server cannot see it. They now sit beside `manifest.json` (`firmware/<target>/ota-manifest.json`, `core.z`, `engine.z`), two segments deep, and a router test proves the bundle answers them |
| assumed-context | 2026-10-06 | [an-update-hangs-when-the-boards-reset-keeps-the-port-open](2026-10-06-an-update-hangs-when-the-boards-reset-keeps-the-port-open.md) | fixed (639edfcde) | lpa-studio-core `UpdateHost` × the link pump: a leg ended only when its link closed, but a board's restart over the emulator's door, and over a C6's USB on a Mac, is an lp-link session reset on a port that stays open. After the core's commit and reset, the driver waited for its old session forever (`just walk-ota-emu`'s first run). The pump now reports the reset (`on_link_reset`), and the board's `M`, or a hello that announces channel 3, brings the driver back on the new session |
| lifecycle-ownership | 2026-10-06 | [an-updates-markers-drop-when-its-card-merges](2026-10-06-an-updates-markers-drop-when-its-card-merges.md) | fixed (fa7a99e7e) | lpa-devices `Roster` × lpa-studio-core `UpdateHost`: identity reconciliation folded the card an install ran on into a remembered record, and the update's markers, still addressed to the merged-away id, were dropped. The card sat on "Finishing the update… 0%" (walk step `cant-get`). The roster records merges and routes markers to the surviving id, and the host re-keys its run (`follow_merge`) |
| state-conflation | 2026-10-06 | [an-identify-clears-how-an-update-ended](2026-10-06-an-identify-clears-how-an-update-ended.md) | fixed (2cff12264) | lpa-devices evidence `last_update_outcome`: any activity's start cleared it, so keeping a core-only board (its identify) erased the "can't get" its restore had ended on, and the card said "Restoring firmware…" with no Install (walk step `cant-get`). It is now cleared only when firmware is written |
| unsynchronized-shared-artifact | 2026-10-06 | [a-cached-test-binary-is-recopied-under-its-reader](2026-10-06-a-cached-test-binary-is-recopied-under-its-reader.md) | fixed (this change) | lp-riscv-emu `test_util::ensure_binary_built`: parallel test threads both missed the in-process map; the second re-copied the cached ELF in place while the first's caller read it ("Invalid ELF section header" in recovery_emu); now re-checked under the lock and staged + renamed |
| stale-measurement | 2026-10-06 | [a-board-out-of-range-flashes-a-transport-error](2026-10-06-a-board-out-of-range-flashes-a-transport-error.md) | fixed (this change) | lpa-studio-core `StudioActor::run_refresh_tick` × the lens tap × plan D13's reconnect rule: a Bluetooth board walked out of range under Play goes quiet without a GATT drop, and the editor's pull timed out (`Transport error: the device did not respond over Bluetooth within 5.0s`) on link health that did not yet include the stall note its own tap had heard, which waited behind the batch. The batch showed the red sync failure, and the next one showed the curtain (PR #880's desk walk). The actor now folds what the link said during an unanswered pull before judging it, and a pull failure while the link is reconnecting is withdrawn like a held lens's |
| state-conflation | 2026-10-06 | [a-typed-unlock-is-not-tried-again-after-a-drop](2026-10-06-a-typed-unlock-is-not-tried-again-after-a-drop.md) | fixed (this change) | lpa-studio-core `AccessSession`: `auto_spent` (set when an automatic try does not unlock) was cleared by nothing but a page load, so a board first unlocked by a typed, remembered password (the held keys matched nothing) never had that password tried again: after a power cut the sheet rose, the board dropped each new link at its 10 s deadline and the editor's hold ran out (PR #880's silicon re-check, walk-2). A grant now re-arms the automatic tries. Same path (walk-5): the challenge held for the sheet was keyed by window, so a second hello on the same link made the typed password begin again, which the board refused while its challenge lived, spending the password; the challenge is the link's now |
| lifecycle-ownership | 2026-10-05 | [a-bluetooth-reconnect-reads-the-old-links-loss](2026-10-06-a-bluetooth-reconnect-reads-the-old-links-loss.md) | fixed (this change) | lpa-link `BrowserBleLink` × `browser_ble.js`'s session error queue × the departure sweep: the sweep detached a dropped Bluetooth link before its pump read `bluetooth link lost`, so the loss waited in the session and the link the reconnect attached read it on open and closed at once. No edge came again, so a board that was connected and saying hello had no model link: no login, the board's 10 s drop, the editor's 45 s hold ran out to `/devices` (PR #880's desk check, a locked C6 after a power cut and a reboot; `walk-ble-emu` `drop-back`). `Open` now discards a loss recorded before it |
| stand-in-divergence | 2026-10-06 | [the-walks-fw-board-boots-the-split-image-core-only](2026-10-06-the-walks-fw-board-boots-the-split-image-core-only.md) | **open** | scripts/emu `{fw}` × `emu serve kind=elf` × #971's split image: the packaged ELF direct-loaded over a blank chip boots core-only ("engine does not fit"), so a walk's `{fw}` board reads "Unrecognized firmware". `walk-ble-emu` now boots `{merged},kind=rom-up`; `walk-wifi-emu`, `walk-drop-emu` and scenarios s2/s3/s7/s8/s9 still use `{fw}` |
| fidelity | 2026-10-04 | [emu-restart-drops-pad-drives](2026-10-04-emu-restart-drops-pad-drives.md) | fixed (this change) | lp-emu-esp-common `SocBus::restore_scalars`: a reset/power-cycle restore overwrote the whole pin `Fabric` from the power-on snapshot, so a pad an outside driver held (the `pin` control verb, `--pin-script`, a `--wire` tie) read `drv=-` after every reboot or power cycle — on silicon neither touches a pad's outside wiring. Shared by `lp-emu-esp32c6`/`s3`/`v3`; `Fabric::restore_chip_side` now keeps `driven`/`tie` across the restore and re-settles. Found making the `?emu=` banner's switch-mode power button for PR #963, which worked around it by re-driving the pin after every reboot |
| assumed-context | 2026-10-05 | [a-requested-reboot-crashed-the-c6-bootloader](2026-10-05-a-requested-reboot-crashed-the-c6-bootloader.md) | fixed | fw-esp32c6 heap placement × the IDF second-stage bootloader: with Bluetooth on, a warm reset of the C6 (RTS `rst:0x15` or requested reboot `rst:0x3`) crashed the bootloader (`Illegal instruction`, PC `0x4087073a`, MTVAL `0x10`) on 11/22 resets on main (0/20 with Bluetooth off). Cause: `c_heap` put the radio's DMA descriptors and buffers in `dram2_seg`, where the bootloader loads its code after a reset, and the controller keeps running across an HP-only reset. The radio now has its own 48 KiB region in main RAM, `dram2_seg` is Rust-only, and firmware resets hold the modem in reset first. Silicon: 0/44 with the fix (mono and split), 5/12 on unfixed main in the same sitting. The ledger's watchdog misattribution is a follow-up |
| silent-drop | 2026-10-05 | [the-host-flasher-dropped-the-split-images-last-bytes](2026-10-05-the-host-flasher-dropped-the-split-images-last-bytes.md) | fixed | tools/lp-fw-split `app_image` + lp-cli `firmware package` + lpa-link host flasher: PR #971's packaged split image ended at `0x2F55FE` (not a multiple of 4), espflash 3.3.0's stub never wrote its last 254 bytes, and nothing checked, so the bench C6 faulted in the engine on every boot (`Illegal instruction mepc=0x425bd5f0`). esptool-js pads to 4, so Studio and `walk-no-board` never saw it. Every packaged image now ends on a 4 KiB sector, and the host flasher checks each write's MD5 before it resets the board |
| assumed-context | 2026-10-05 | [the-split-tool-missed-a-negative-addend-reference](2026-10-05-the-split-tool-missed-a-negative-addend-reference.md) | fixed | tools/lp-fw-split `section_graph`: a relocation was attributed to the section holding `S + A`, and codegen addressed a core-read table as `sym - 12`, so pass 1 placed the table in the engine region (a core-only board never maps it) and pass 2's verifier flagged an unrelated string. The symbol's own section is now an edge too |
| stand-in-divergence | 2026-10-04 | [a-legacy-tab-board-cannot-finish-its-update](2026-10-04-a-legacy-tab-board-cannot-finish-its-update.md) | **open** | lpa-studio-core `EmuDeviceTransport` `InspectLayout`: a mode-A tab board written before the C6 repartition keeps its files at `0x310000` through an update now, and the firmware holds them (`LegacyHeld`). This transport has no layout step, so Finish update flashes plain again and the board stays held. Its files are safe but unusable. The tab port's `getFlash`/`writeFlash` are what an executor of the existing `FlashPlan` needs. Read from the code |
| budget-exhaustion | 2026-10-03 | [studios-editor-read-costs-the-classic-a-frame-and-a-half-of-cpu](2026-10-03-studios-editor-read-costs-the-classic-a-frame-and-a-half-of-cpu.md) | **open** | lpa-server project read × fw-esp32v3: with Studio's editor open the classic's LEDs hitch — each lens read (~2.6–3/s; shapes+nodes detail, runtime, output_frame + binding_graph include_values probes) costs ~140–175 ms of CPU in the render's tick, so ~1 frame in 4 runs ~200–270 ms on main and on every PR #943 arrangement; messages-first turned it into ~180 ms judder. The runtime query also logs `[MEM]`/`[JIT]` and scans the stack on every read. Measured on the DOM-Z-102 with `frame_pace_diag` under a recorded real Studio |
| fidelity | 2026-10-03 | [the-emulated-classic-renders-14x-faster-than-silicon-and-hides-the-link-threads-frame-rate-effects](2026-10-03-the-emulated-classic-renders-14x-faster-than-silicon-and-hides-the-link-threads-frame-rate-effects.md) | **open** | lp-emu-esp32v3 `t1` (no memory-cost model) × frame-relative claims: on `quad-wire-oracle` silicon renders 30.9 fps where the emulator renders 447.7 (14.5×), so PR #943's link thread reads −0.04 % idle fps emulated and **+7.6 %** on the DOM-Z-102, and its per-request render cost (0.49 → 0.93 % per request/s on silicon) and request latency in frames were never visible emulated. Memory still transfers (heap delta byte-exact). Found by PR #943's classic desk sitting. `lp-emu:esp32v3:t1`, lp-emu `c042102d9` |
| two-clocks | 2026-10-03 | [the-classic-loses-its-boot-text-when-a-host-holds-the-link](2026-10-03-the-classic-loses-its-boot-text-when-a-host-holds-the-link.md) | fixed (this change) | fw-esp32v3 `boot_firmware`'s first `[INIT]` line × lp-link deframer: not a second writer — the ROM/bootloader print at 115,200 baud and a host reads UART0 at 921,600 throughout, so it misreads them (almost all `0x00`/`0x80`) and is left mid-frame as often as not when the app's own, correctly-read text starts, swallowing it as a frame body — PR #943's torn line, byte for byte. `boot_firmware` now writes lp-link's text mark (`0xFF`) just before that first line, resetting any host deframer whatever state the misread ROM text left it in. Found by #884's owed classic walk (check j) |
| fidelity | 2026-10-03 | [the-emulated-s3-shows-no-frame-rate-cost-for-link-load](2026-10-03-the-emulated-s3-shows-no-frame-rate-cost-for-link-load.md) | **open** | lp-emu-esp32s3 time grade `t1` (its only grade): PR #942's desk sitting found the S3's frame rate falls 7–9 % (main) and 19–20 % (link thread + messages-first) on silicon while `link rtt` answers requests, where the emulator showed +0.2–0.3 %; request latency in frames carries over within ~0.1 frame. Silicon renders `shader-oracle` 4.3× slower than `t1` (55 vs 239 fps). Mechanism unproven — likely the C6's cold-code-path blind spot (no cache cost at `t1`), but no S3 cache fill has been measured. Also: main's frame-bound transfer moves 1.7–1.9× more bytes a frame emulated. `lp-emu:esp32s3:t1`, lp-emu `ab8345d38`; silicon `D8:3B:DA:47:29:70` |
| capped-store-without-eviction | 2026-10-02 | [a-full-device-store-refuses-new-access-silently](2026-10-02-a-full-device-store-refuses-new-access-silently.md) | fixed | lpa-studio-core access sync + lpc-access cap: every dev-server origin is its own browser key, so Yona's desk C6 filled with 16 × "Brave on Mac" and the 17th USB connect's `TooManySecrets` was only logged. A full board now drops its oldest other browser key (never this browser's, an account's, or a password) and says which; retired keys go first; a failed sync shows in the panel |
| stale-measurement | 2026-10-03 | [a-studio-reload-re-pushes-the-project-the-board-runs](2026-10-03-a-studio-reload-re-pushes-the-project-the-board-runs.md) | fixed | lpa-studio-core `attach_lens`: a reload of `/p/…?on=mac:` re-dispatches the open, which pushes unless the mismatch rule stops it — and that rule read "is anything running" off the roster's heartbeat, which a fresh page does not have yet. The classic got its whole project re-sent on every refresh, and a different project would have gone over the running one with no page. The open now asks the board (loaded list + package hash) once the lens is up: its own head → bind, no push; another library project → the mismatch page |
| absence-from-incomplete-search | 2026-10-03 | [xiao-c6-d4-d5-unmapped](2026-10-03-xiao-c6-d4-d5-unmapped.md) | fixed | lpc-hardware `boards/seeed/xiao-esp32-c6.json`: D4/D5 were `not-found` with no GPIO, so no project could put LEDs on them (a user asked for D5). The 2026-05 calibration only pulses GPIOs the manifest already lists, and the first profile listed GPIO0–21; D4/D5 are GPIO22/23 (Seeed pin list). Now assigned, with resources, sidecar pins and a regression test |
| stand-in-divergence | 2026-10-03 | [a-held-board-runs-a-fresh-access-store-in-ram](2026-10-03-a-held-board-runs-a-fresh-access-store-in-ram.md) | fixed | fw-esp32c6 BLE start × lpa-studio-core AccessController: a board held for the C6 layout change runs on a RAM fs, read its missing store as `fresh()` (Bluetooth on) and took Studio's USB key add into RAM ("Who has access 1" while its real 16-entry store waited at 0x310000). More open than its own list when that list said Bluetooth off. A held board now boots `locked()` (`access_store::device_store_at_boot`), and Studio neither syncs nor lists access for it. Nothing reached flash before or after |
| partial-knowledge-loss | 2026-10-03 | [a-full-device-store-loses-the-list-studio-read](2026-10-03-a-full-device-store-loses-the-list-studio-read.md) | fixed (`d71091bf3`) | lpa-studio-core `sync_access`: on a device store already at its 16-entry cap, the USB sync's add of this browser's key was refused, and the refusal replaced the list the sync had just read. With no cached record the panel stayed "still reading" (`ble_enabled: None`): "Who has access 0" and the Bluetooth switch locked — G1's spare C6 after its migration (the migration was not the cause; any connect did it). The list now survives a refusal and the panel says the list is full |
| partial-knowledge-loss | 2026-10-02 | [a-wire-32-board-reads-as-pre-hello-firmware](2026-10-02-a-wire-32-board-reads-as-pre-hello-firmware.md) | fixed (`1afe61e7b`) | lpa-link demux × lpc-wire hello: wire 33 made `hardware.fs` required, so a wire-32 hello failed the full decode and was dropped as an anomaly with its `proto` in it; every fielded C6 read "pre-hello firmware" at G1. `lpc_wire::hello_proto` now reads that one field, and the fold calls the board older LightPlayer firmware (Update firmware when its board resolves). `lp-cli`'s `DeviceSession` had the same gap (2026-10-03 rehearsal: "predates the wire hello"); it reads the proto too now |
| assumed-context | 2026-10-02 | [studio-reading-a-boards-files-stalls-on-a-mac](2026-10-02-studio-reading-a-boards-files-stalls-on-a-mac.md) | fixed (`650299682`) | lpa-link `browser_esp32_flash.js`: esptool-js's `readFlash` streams 4 KB packets of mostly-`0xFF` erased flash, and macOS Web Serial (`PARMRK`) drops `0xFF`-heavy bytes the page reads late, so G1's layout read stalled at "Reading the board's files" (silicon: 310,706 of 983,040 B, then esptool-js's 100 s timeout). Every read is now `readFlashSafely`: 384-byte packets, one in flight, length + MD5 checked. The 09-26 Web Serial loss, met by a second protocol |
| fidelity | 2026-10-02 | [the-emulated-serial-path-never-drops-a-byte](2026-10-02-the-emulated-serial-path-never-drops-a-byte.md) | fixed | lpa-studio-web `virtual_serial.js`: the emulator's `navigator.serial` shim was a lossless pipe, so the Mac-only loss above passed `walk-migration-emu`. `mac_tty_model.js` (the tty's doubled `0xFF`, the wrapping free count, Chromium's pipe) now sits between emulated boards and a page on a Mac (`?emu-tty=mac|none`), read at most every 16 ms; the walk then reproduces the stall with the old reader (34 drops, `No serial data received.`) and passes W1/W4 with the new one, 0 drops |
| assumed-context | 2026-10-02 | [the-host-filesystem-read-throws-away-in-flight-packets](2026-10-02-the-host-filesystem-read-throws-away-in-flight-packets.md) | fixed (`244d3d59c`) | lpa-link host_serial_esp32: the bootloader read pipelined 1024 packets while espflash's per-packet ack clears the port's input buffer, so the next packet's head was thrown away ("truncated at 8185 of 983040 bytes"); one packet in flight now. Found by the C6 repartition's emulator walk |
| fidelity | 2026-10-02 | [the-emulated-c6-power-cycle-inherited-the-download-strap](2026-10-02-the-emulated-c6-power-cycle-inherited-the-download-strap.md) | fixed (`e9f26ea96`) | lp-emu-esp32c6 `power-cycle`: sampled the LAST reset's strap (a download dance's) instead of the pins, so a cable pull after an update came back in the ROM downloader |
| stand-in-divergence | 2026-10-02 | [updating-a-tab-hosted-board-erases-its-files](2026-10-02-updating-a-tab-hosted-board-erases-its-files.md) | fixed | lpa-studio-core `EmuDeviceTransport` × `emulator_tab_bridge.js` `flashEmuPackage`: a user's tab-hosted emulated board (mode A) updated through `putFlash`, which erases the WHOLE chip before writing the image, so its files went too (reproduced on the real tab module: `lpfs@0x350000` all `0xFF` after an update). The update now writes the merged image with `writeFlash`, which erases only the sectors the image covers, as esptool does. The `?emu=tab` polyfill lane (esptool-js) migrates like a board (W12) |
| write-ordering | 2026-10-02 | [a-closed-tab-mid-stamp-leaves-hardware-json-truncated](2026-10-02-a-closed-tab-mid-stamp-leaves-hardware-json-truncated.md) | fixed (`0db3e4798`) | the board-manifest stamp after every update (and `lp-cli hardware stamp`) wrote `/hardware.json` in chunks; a tab closed mid-stamp left 6,144 of 6,802 bytes (emulated) and the board lost its stamp. On main too. Now journaled through `/hardware.json.next`, settled by the firmware's loader at boot; no wire change |
| untested-path | 2026-10-02 | [ble-asks-password-when-open](2026-10-02-ble-asks-password-when-open.md) | fixed | lpa-studio-core `AccessSession::checked` vs `logged_in`: `logged_in`'s `Outcome::Granted` arm already clears a stale `prompt`/`last_refusal` on a grant it just earned (except `NeedsEdit`); `checked` set `phase = Granted` on the hello's own grant but never cleared them. A sheet raised while a board was locked survived the board being opened, a disconnect/reconnect, and the next hello — Bluefy on an iPhone asked for a password on an open-play board. The emulated BLE path (`?ble=emu`) is Studio's trusted link, so no walk ever drove `checked`'s `(required: true, ...)` branch |
| stand-in-divergence | 2026-10-02 | [the-ble-emu-polyfill-relays-lp-link-bytes-as-m-lines](2026-10-02-the-ble-emu-polyfill-relays-lp-link-bytes-as-m-lines.md) | fixed (#880) | lpa-studio-web `virtual_bluetooth.js` (`?ble=emu`): the polyfill piped the emulated board's lp-link USB bytes to Studio's `M!` Bluetooth link, so an emulated Bluetooth board never identified. #880 put Studio's Bluetooth on lp-link and made the polyfill translate datagram frames to the board's stream framing; the BLE conformance suite pins it in CI |
| state-conflation | 2026-10-02 | [a-dropped-link-sends-the-editor-to-devices](2026-10-02-a-dropped-link-sends-the-editor-to-devices.md) | fixed | lpa-studio-core `drop_device_lens_if_wireless`: "the link is gone" and "the session is over" were one event, so every routine Bluetooth drop (Bluefy phantom drop, hidden page, radio) or USB re-seat closed the editor, and `web_app.rs` routed to `/devices` even though the provider reconnected within a second. A wire link that goes away now HOLDS the session behind the Reconnecting strip and rebinds it on the board's new link; only 45 s of awake time away ends the open. Reported by Yona on Bluefy |
| backend-contract-divergence | 2026-10-02 | [bluefy-writes-a-views-whole-buffer](2026-10-02-bluefy-writes-a-views-whole-buffer.md) | fixed (phone-proven) | lpa-link browser_ble.js `write`: chunks were `subarray` views and Bluefy sends a view's WHOLE buffer, so every 180 B chunk carried the entire request; past 512 B the board refused the long write, the page tore the link down (0x13) and Bluefy alerted — pinning a palette dropped the link every time. Chunks are `slice` copies now; the `?ble=emu` polyfill models Bluefy (`wholeBufferWrites`). Explains the 2026-09-25 "why a long write" mystery |
| state-conflation | 2026-10-02 | [a-bluetooth-reconnect-after-an-unlock-stays-locked-and-flaps](2026-10-02-a-bluetooth-reconnect-after-an-unlock-stays-locked-and-flaps.md) | fixed | lpa-studio-core `AccessSession::logged_in`: a successful automatic unlock marked the automatic tries spent, so a held key was never offered again. On a LOCKED board every silent Bluetooth reconnect came up locked, the board dropped it at its unlock deadline, and Web Bluetooth reconnected: one native Bluefy alert per lap, forever. Only a try that did not unlock is spent now. Found while chasing a Bluefy flapping report, but not that report's cause (that board is open; the cause was bluefy-writes-a-views-whole-buffer) |
| assumed-context | 2026-10-02 | [saved-records-sharing-a-device-id-misroute-the-board](2026-10-02-saved-records-sharing-a-device-id-misroute-the-board.md) | fixed | lpa-devices `Roster::load_records` + lpa-studio-core `DeviceRoster::is_already_known` / auto-name: two registry rows wearing one `device_id` (ids are minted per page; a row was skipped as "loaded" whenever any device wore its id) loaded as two entries with one `DeviceId`. The hello merged by MAC into the right record, the link then routed by id to the other board (`IdentityConflict` on every frame, card never Ready), and each auto-name `SetName` renamed the neighbour, ping-ponging "… · Oct 2" / "… · Oct 2 2" forever. Reached Yona's M1 feel gate on his saved-profile Chrome; link speed irrelevant |
| fidelity | 2026-10-01 | [the-emulated-c6-charges-a-cold-code-path-10x-less-than-silicon](2026-10-01-the-emulated-c6-charges-a-cold-code-path-10x-less-than-silicon.md) | **open** | lp-emu-esp32c6 time grades: `t1`/`t2` install no memory-cost model, so a flash-cache-cold path costs what a warm one does — the io-thread spike's idle link pass was 31 µs at `t2` against 252–349 µs on a XIAO C6, and preempting the render cost −0.8 % of frame rate emulated against −9.9 % on silicon. `t3` (`CacheCost`, 338-cycle fills measured 2026-09-08) already charges it: 249–331 µs a pass, −4.8 % of frame rate, ≈150 fills a pass in the link loop. Open: walks pin `t1`, nothing measures frame rate at `t3`, and `t3` closes about half of silicon's frame-rate cost. The other direction of the 40×-slower graphics-stage entry. `lp-emu:esp32c6:t2`/`t3`, lp-emu `28d010762` |
| fidelity | 2026-10-01 | [the-c6-choker-truncates-most-ws281x-frames-on-silicon](2026-10-01-the-c6-choker-truncates-most-ws281x-frames-on-silicon.md) | **open** | fw-esp32c6 RMT / lp-emu-esp32c6: on the desk C6 rendering the PLAYFUL choker, `ws281x_telemetry` reads 74–79 % of frames ending on a guard trip (main and the link-thread branch alike), while the emulator at `t2` decodes the same project's frames 0 errors / 0 incomplete. 2026-10-03: `t3` truncates too (trips 28/1,056, 20 frames of 72 bits); cause = esp-hal's RISC-V dispatcher calling into flash (`change_current_runlevel`, the per-source closure) on the way to the RAM `rmt_isr`; esp-hal fork diff 4 puts it in RAM (`t3`: trips 8/1,067, `entry_max` 34 → 19, no 72-bit frames). Open until the desk run confirms. Found by the C6 link-thread plan's silicon telemetry check |
| emulator-fidelity | 2026-09-10 | [the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon](2026-09-10-the-emulated-c6-builds-a-graphics-stage-40x-slower-than-silicon.md) | **open** (root cause narrowed to the graphics stage; mechanism not yet named) | lp-emu/esp/lp-emu-esp32c6 × lpc_engine graphics construction: pushing `fyeah-sign` (an 8.8 KB `fyeah.map2d.json`) to an emulated C6 never finishes `Project::new`'s graphics-stage step and the RWDT resets it at ≥8 s guest time, where the same binary and project take silicon 0.199 s — ~40× the instructions, with no PC histogram yet to say where they go. A small project (`peach-1d`) goes through cleanly on both. 2026-10-01: a 250-lamp strip with the Fixture node's default `texture_area` sampling trips the same watchdog; `direct` sampling (what the board-project generator writes) does not. `lp-emu:esp32c6:t1`, lp-emu `28d010762`; its inverse direction is the 2026-10-01 cold-path entry above |
| wake-quantum-throttle | 2026-09-29 | [the-classics-log-ring-drops-records-under-a-project-load-burst](2026-09-29-the-classics-log-ring-drops-records-under-a-project-load-burst.md) | **open** (reduced: 14 vs 25 drops emulated; the link thread that fixed it is opt-in since the 2026-10-03 silicon A/B) | fw-esp32-common `log_ring_logger` (4 KiB) × `uart_link_task`: on the classic on lp-link a project-load burst lost log records two ways. **Fixed (P7):** `pump` took records the link's full two-slot datagram queue then refused — lost with no notice a host could see (`datagramsDropped`), which cost the socket-hosted desk walk its lit `[OUT] dump` 3/3; `Link::datagram_room` now gates it. **Fixed (M2 P4, 2026-10-03):** the ring's own counted overflow (five-wire load: 25 records, IO18's open line among them) was the link task getting no pass while the render held the one shared thread; moving the link task to its own core-0 esp-rtos thread (`io-thread`) drains the ring during a load burst too — five-wire drops fell to 0. `lp-emu:esp32v3:t1`, lp-emu `ab8345d38` |
| fidelity | 2026-09-29 | [the-emulated-c6-does-not-perform-a-software-reset](2026-09-29-the-emulated-c6-does-not-perform-a-software-reset.md) | fixed | lp-emu-esp32c6 `LP_AON` (an accept block): the ROM's `software_reset` sets `sys_cfg.hpsys_sw_reset` and returns, nothing resets, esp-hal's `-> !` `software_reset()` falls through into the next function (unmapped writes at 0x0), and the LP watchdog reboots the chip ~8 s later with `cause=watchdog-reset`. Found by the walk's `--request reboot`. Fixed: the `sys_cfg` store now resets at once as `rst:0x3 (LP_SW_HPSYS)`, and the reboot test is back under `--strict-bus` |
| stand-in-divergence | 2026-09-27 | [fragmented-heap-refuses-every-read](2026-09-27-fragmented-heap-refuses-every-read.md) | mitigated (two-number gate; fix = plan `2026-09-27-1218-fragmentation-tolerant-reads` PR B) | lpa-server ProjectRead gate × fw-esp32c6 two-region heap with Bluetooth on: shader edits cut region 0's free tail, Bluetooth holds region 1 to a ~19.5 KB block, and the 32 KiB largest-block floor refused every read on the prod choker with ~90 KB free (emulator: 52 of 96). **Second** `stand-in-divergence` on this gate — the 2026-09-04 classic entry named it and deferred the fix. Now 40 KiB free + 16 KiB block on the C6/S3; `lp-cli/tests/emu_frag_reads.rs` in CI |
| assumed-context | 2026-09-27 | [the-in-endpoint-gate-loses-the-drains-wake-inside-a-free-lag](2026-09-27-the-in-endpoint-gate-loses-the-drains-wake-inside-a-free-lag.md) | **open** | fw-esp32-common `serial/in_endpoint.rs` `InEndpoint::ready` × the lp-link USB task × the emulator's free-lag hypothesis (off by default, unmeasured on silicon): on the lp-link image the gate checks the free bit ~0.7 us before esp-hal would write, so under a lag its clear-then-wait erases the drain's `serial_in_empty` edge and waits out the 250 ms write bound — 0 of 40 replies, 9 write timeouts at `lp-emu:esp32c6:t1`. Since the esp-hal back-port (#855) esp-hal's own post-`wr_done` wait has the same shape, so the ungated image stalls identically and a gate-only fix would not end it. Fix sketched (poll the free bit on a short timer), not applied: wants a desk check |
| wake-quantum-throttle | 2026-09-25 | [c6-takes-requests-in-at-one-packet-per-frame](2026-09-25-c6-takes-requests-in-at-one-packet-per-frame.md) | fixed (`0456f590f`; emu + silicon confirmed 2026-09-26) | fw-esp32c6 `serial/io_task.rs` `read_serial`: the USB read took one 64 B OUT packet per wake, and `io_task` wakes about once per server-loop frame, so a 600 B Studio lens request took ~10 frames to arrive (emulated C6: round trip linear in request bytes, 0.37 ms/B wall; 354 → 92 ms for 600 B once the read drains the burst). Yona's "Studio feels sluggish on a device" on #827's emulated C6 |
| assumed-context | 2026-09-26 | [a-half-written-packed-frame-swallowed-the-next-connection](2026-09-26-a-half-written-packed-frame-swallowed-the-next-connection.md) | fixed (#835) | fw-esp32-common chunked_write × FrameScanner × WireStream: a page closed mid-reply left half a packed frame (no closing `00`) that the next connection received; the board's JSON fallback has no `00`, so the new reader swallowed the Hello reply and every heartbeat ("Nothing from this board yet"). The board now sends a resync marker `00 00 'R' 01 00` after any failed write; the scanner reads text after it from any state |
| fidelity | 2026-09-26 | [an-emulated-board-went-silent-after-a-tab-reload](2026-09-26-an-emulated-board-went-silent-after-a-tab-reload.md) | **open** | lp-cli emu serve × lp-emu-esp32c6 USB-SJ coupling × fw io_task (not separated): after a Studio tab reload mid-lens-read the board answered none of five Hellos for 120 s; a cable pull recovered it; 1 in 5 G1 protocol runs |
| wake-quantum-throttle | 2026-09-25 | [c6-takes-requests-in-at-one-packet-per-frame](2026-09-25-c6-takes-requests-in-at-one-packet-per-frame.md) | fixed | fw-esp32c6 `serial/io_task.rs` `read_serial`: the USB read took one 64 B OUT packet per wake, and `io_task` wakes about once per server-loop frame, so a 600 B Studio lens request took ~10 frames to arrive (emulated C6: round trip linear in request bytes, 0.37 ms/B wall; 354 → 92 ms for 600 B once the read drains the burst). Yona's "Studio feels sluggish on a device" on #827's emulated C6 |
| state-conflation | 2026-09-25 | [turn-on-bluetooth-reports-a-timeout-the-board-answered](2026-09-25-turn-on-bluetooth-reports-a-timeout-the-board-answered.md) | fixed (PR #824) | lpa-studio-core access write × the link's one `ConversationInbox` × the card's frame feed: Turn on Bluetooth said `device did not respond within 5.0s` (4 of 4) while a wire tap showed the board's `error:null` reply 192 ms later; the feed, polling the same inbox, popped and dropped it. Each conversation on a shared link now owns its own id slice, and `receive` takes only its own replies |
| assumed-context | 2026-09-25 | [tag-next-version-tagged-the-tip-not-its-commit](2026-09-25-tag-next-version-tagged-the-tip-not-its-commit.md) | fixed | scripts/tag-next-version.sh: Main push `git pull`ed and tagged main's tip, so two close merges left the first commit untagged and its workflow_run deploy failed `--require-tag`; now tags its own `$GITHUB_SHA`, a tagged commit is a no-op, a lost number race retries |
| assumed-context | 2026-09-25 | [panel-restore-drops-dormant-entry-knobs](2026-09-25-panel-restore-drops-dormant-entry-knobs.md) | fixed | lpa-server `panel_state::restore` × lpc-engine `PanelWriterStore`: restore kept a persisted writer only if a live node inhabited its scope, so a reboot forgot every dormant playlist entry's knobs; and a pattern module's knob is keyed by the module's runtime id (`ScopeRef::Module`), so an unload and reload orphaned it in memory. Restore now accepts every authored entry's sink scope and parks writers for scopes inside a dormant entry by persist path; the residency step parks and re-engages them. File format unchanged |
| assumed-context | 2026-09-25 | [remove-node-orphans-a-dormant-entrys-files](2026-09-25-remove-node-orphans-a-dormant-entrys-files.md) | fixed | lpc-registry `node_authoring::remove_node`: `staged_deletes` was a diff of the EFFECTIVE inventory, so a dormant playlist entry's def/assets — never in that inventory by design (PD1) — were left on disk forever when the entry, or its whole playlist, was removed. `remove_node` now widens residency to every entry before diffing, then narrows back before `changes` is measured, so the sweep reaches a dormant entry's files without ever leaving residency changed or the widen visible to the caller |
| dense-over-monotonic-ids | 2026-09-25 | [node-tree-tombstones-grow-per-reload](2026-09-25-node-tree-tombstones-grow-per-reload.md) | fixed | lpc-engine `RuntimeNodeTree`: entries were a `Vec<Option<_>>` indexed by never-reused `NodeId`, so `remove_subtree` left a tombstone per node and a dormant-entry cycle (remove + attach per switch) would grow slot storage forever (145,152 B over 100 three-node reloads on the host). Now `NodeEntrySlots`, live entries sorted by id with an O(1) id-position guess: removed entries are dropped, ids stay monotonic; `node_tree_reload_memory` pins 0 B growth |
| unenforced-test-precondition | 2026-09-25 | [emu-lab-cooldown-tests-measured-wall-clock](2026-09-25-emu-lab-cooldown-tests-measured-wall-clock.md) | fixed | scripts/emu/lab queue/notify/stability tests × `server.mjs`'s scheduler: the cooldown, spacing, lost, drop and notify-grace tests asserted wall-clock gaps measured from the fake device's side of the socket, so a loaded runner's lag came off the gap (`waited 504 ms` against a 600 ms floor, main red at a7daa7ef5; 5/24 runs locally under load). The scheduler now reads an injected clock (`clock.mjs`) and the tests step a manual one and assert exact waits off the server's own press record |
| unenforced-test-precondition | 2026-09-25 | [a-late-hello-answer-reaches-the-fresh-window](2026-09-25-a-late-hello-answer-reaches-the-fresh-window.md) | fixed | lpa-studio-core device e2e bench × the lpa-link fake board: `an_effect_that_outlives_its_activity…` assumed no hello could reach the window after the eviction's reopen, but the fake's real-thread server answers identify's id-1 ask in real time. Under load that answer was still inside the server when the hung push took the wire, and the reopen flushes only the byte wire, so the card read Ready (CI on #814 and #816; 11/96 locally under load). The test now waits for `FakeEsp32Device::unanswered_requests() == 0` before the push |
| fidelity | 2026-09-24 | [emulated-replug-leaves-the-old-byte-channel-open](2026-09-24-emulated-replug-leaves-the-old-byte-channel-open.md) | fixed | lpa-studio-web virtual_serial.js (`unplug`/`markDead`) × emulator_port.js: a replug's new port generation shares one `EmulatorPort`, and the unplug errored the open stream without closing the byte channel, so the replugged `open()` refused ("already open in this page") and the card sat at "Attached — not listening" beside a talking board. `walk-no-board` had reported it since M3 as a product question |
| assumed-context | 2026-09-24 | [a-departed-page-kept-the-doors-boards](2026-09-24-a-departed-page-kept-the-doors-boards.md) | fixed | lpa-studio-web virtual_serial.js / emulator_port.js `dispose` × index.html `?emu=` seam: `pagehide` cleanup awaited one board's byte-socket close handshake before closing anything else, and a leaving (bfcache-frozen) page never resumes, so the other boards' control channels stayed held and the next load hit a 409 and fell back silently to the real `navigator.serial`. Now every close starts in one turn, install retries a 409, and a failed install shows a banner |
| assumed-context | 2026-09-24 | [board-open-waits-silently-with-no-way-out](2026-09-24-board-open-waits-silently-with-no-way-out.md) | fixed | Studio's opening frame × the device-lens hold × web_app route sync: a board open showed "Opening project…" with no steps and no exit; a fresh `/p/…?on=mac:` page held forever on a board it had no port for (only a click can grant one); a board that rebooted mid-open sent the page to `/devices` silently. Not a firmware crash (the capture's USB write timeout is a dropped heartbeat). Now staged (upload bytes, compile), with Connect / Reset / Cancel |
| backend-contract-divergence | 2026-09-24 | [gpu-tier-refuses-the-control-message-array-idiom](2026-09-24-gpu-tier-refuses-the-control-message-array-idiom.md) | **open** | lp-gfx-wgpu assembly × projects/test/{events,button}: `uniform ControlMessage events[8]` (8-byte struct, natural stride 8) fails naga's uniform stride-16 rule on the GPU tier while every CPU target compiles it — the known `wgpu-refuses-scalar-uniform-arrays` gap already covering shipped content; found by the new example-shader compile gate, which allowlists both pairs against this entry |
| unenforced-test-precondition | 2026-09-24 | [boot-no-radio-asserted-the-window-edge](2026-09-24-boot-no-radio-asserted-the-window-edge.md) | fixed (`bda62a06c`) | lp-emu-esp32c6 `tests/boot_no_radio.rs`: the 3 s gate compared raw tick-handler entries against raw threshold restores. A tick entered in the last ~1,000 cycles before the deadline has no restore, so the result flipped with firmware layout, and main went red at e226fb28. Handlers are now paired in order, and only the last may be in flight |
| budget-exhaustion | 2026-09-24 | [ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut](2026-09-24-ble-enabled-c6-refuses-a-project-switch-after-the-heap-cut.md) | fixed | fw-esp32c6 heap placement × the load gate: with BLE enabled, choker → zook was refused (largest free block 65,534 < 65,536) with 212 KB free. Allocations made during a project and kept past it split the main region: cleared-but-kept tables, a JIT vmctx leak, USB's grown read buffer, and the BLE link layer's blocks, which are re-made when advertising follows the project's name. Fixed by placing the radio's C heap in the reclaimed segment first and releasing emptied tables |
| assumed-context | 2026-09-23 | [bluefy-hidden-page-does-not-see-ble-drops](2026-09-23-bluefy-hidden-page-does-not-see-ble-drops.md) | fixed | Studio's Web Bluetooth link (M5, `bcd6e4dbd`): iOS suspends a hidden Bluefy page, so a BLE drop while the phone is locked is delivered only when the page is shown again, and SSE control streams close. The link survives a 30 s lock; reconnect needs no tap (803–954 ms). Fixed by re-reading every link on `visibilitychange → visible`; the phone half is M7's walk |
| assumed-context | 2026-09-23 | [five-wires-reads-a-report-still-on-the-uart-as-a-lost-one](2026-09-23-five-wires-reads-a-report-still-on-the-uart-as-a-lost-one.md) | fixed (`a3d928d4d`; recurrences 2026-09-24 fixed in PR #810 and 2026-09-25 in PR #808) | lp-emu-esp32v3 tests/five_wires.rs: the "no summary line lost" bound assumes every report is on the wire at the 10 s deadline; on the #794-over-lean-wire image the deadline lands 2 frames after frame 1980 with its five report lines still draining UART0 at 921,600 baud, and the test fails with nothing lost |
| assumed-context | 2026-09-23 | [xiao-c6-rf-switch-never-powered](2026-09-23-xiao-c6-rf-switch-never-powered.md) | fixed | fw-esp32c6 board init: the XIAO ESP32C6's RF switch (GPIO3 LOW = powered, GPIO14 = antenna select) is never driven, so every XIAO radio runs into an unpowered switch; BLE drops 7 in 84 s → 0 once driven. Now a board-quirk table keyed on the manifest in effect (`lpc_hardware::board_quirks_for`) |
| assumed-context | 2026-09-26 | [esp-hals-usb-isr-clears-a-tx-edge-it-did-not-handle](2026-09-26-esp-hals-usb-isr-clears-a-tx-edge-it-did-not-handle.md) | fixed (PR #855) | third_party/esp-hal `usb_serial_jtag.rs` `async_interrupt_handler`: clears `serial_in_empty` AND `serial_out_recv_pkt` whichever fired, so a drain landing between its `int_st` read and `int_clr` write loses the TX edge and the write future waits out its caller's 250 ms timeout. Emulator: 9 stalls in 60 s of two-way soak, 0 with the fix. Fixed by back-porting upstream esp-hal #6089/#6097/#6104 (1.2.0) into the fork: 0 lost wakes in 40 min on silicon vs ~1 per 10 min stock. Before the fix, lp-link USB (PR #854) had already turned each stall into a resend — latency, not a failure |
| assumed-context | 2026-09-26 | [web-serial-on-macos-drops-bytes-of-packed-frames](2026-09-26-web-serial-on-macos-drops-bytes-of-packed-frames.md) | fixed (`86fb8e5e4`; the lp-link USB cut-over, PR #854) | Chromium Web Serial on macOS × xnu tty × IOSerialFamily × the packed encoding: Chromium sets `PARMRK`, so the tty queues every data `0xFF` twice; IOSerialBSDClient counts one slot per byte in a `UInt32` that wraps once the 1,024-slot queue passes 1,020, and the tty then drops whole kilobytes while the page reads late. Packed frames (~1 % `0xFF`) tear; JSON never does. Silicon: native raw reader 0 B lost of ~60 MB; Chromium termios 68 KB lost of 8.7 MB with no stalls; JSON 0. Behind the prod tears of 2026-09-26. lp-link's COBS-FF framing keeps `0xFF` off the wire structurally, so the loss mode is gone regardless of encoding; the underlying Chromium/Apple bugs are unfixed and still worth an upstream report |
| fidelity | 2026-09-24 | [the-real-c6-link-loses-bytes-inside-a-packed-frame](2026-09-24-the-real-c6-link-loses-bytes-inside-a-packed-frame.md) | fixed (`a3c852aa2`) | fw-esp32c6 USB-Serial-JTAG × lp-emu usb_sj link model: at the JSON Pack desk sitting one steady-state packed frame (1 of ~345) arrived ~5 bytes short with nothing logged; every emulator walk decoded all frames. Candidate: the S3 stale-`serial_in_empty` write race on silicon; JSON lost 0 of 1,270 on the same board, so packed-path specific. #805's IN-endpoint gate ported to the C6 (fw-esp32-common); confirmed on the XIAO 2026-09-25: 0 of 1,327 packed frames lost with the gate. The emulator reproduces the loss (6 bytes a frame, ungated only) under the link model's free-lag hypothesis, off by default: esp-hal writes 26 cycles before the gate checks (PR #832) |
| fidelity | 2026-09-23 | [emulated-usb-port-drains-with-no-client-attached](2026-09-23-emulated-usb-port-drains-with-no-client-attached.md) | fixed | lp-cli emu serve (`--usb-host` default) × lp-emu-esp32c6 byte-socket coupling × lp-emu-esp-common `TcpHost` backlog: the door powered boards on with the port open and draining and nobody connected, so the first client was replayed everything (299 heartbeats, ~160 KB); the default is now `attached-idle` |
| untested-path | 2026-09-25 | [a-knob-jump-over-bluetooth-kills-the-c6-ble-host](2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md) | fixed (PR #831) | esp-radio 0.18 (C6 ACL receive) × fw-esp32c6 ble_task × Studio Play over `ble:`: an ATT write of 182–187 B (Studio's knob jump to 4) reached the host as a chained mbuf cut to its first segment, the host runner failed on it, and its restart (an HCI Reset) left the dead link open and the board silent until a USB reboot. Fixed at the cause (esp-radio fork copies every mbuf) and in the recovery (the reset closes every link in the host, advertising restarts) |
| untested-path | 2026-09-25 | [a-long-bluetooth-write-is-acknowledged-and-lost](2026-09-25-a-long-bluetooth-write-is-acknowledged-and-lost.md) | **closed 2026-09-29**: fixed by PR #834 (phone-confirmed); the ATT long-write path itself is deleted by the BLE-on-lp-link cut-over, so the bug class cannot recur | fw-esp32c6 ble × trouble-host 0.6 attribute server × lpa-link browser_ble.js: G4 on iPhone/Bluefy, editing a setting bounced Studio to /devices while the board kept the radio link (it closed only with the tab), and a reconnect "never said hello". Cause (phone-confirmed): a long write (Prepare/Execute) to RX was acked and dropped by trouble-host, a full-MTU reply (ATT MTU 251 on a 251 B controller) restarted the BLE host, and Studio's drop never called gatt.disconnect(). Long writes reassembled, packet pool 251 (MTU 247), every drop and failed write tears the link down, writes in 180 B |
| fidelity | 2026-09-22 | [emulated-reset-restores-rtc-fast-persistent](2026-09-22-emulated-reset-restores-rtc-fast-persistent.md) | **open** | lp-emu-esp32c6 machine.rs (`restore_in`): every memory region is restored on an emulated reboot, so `.rtc_fast.persistent` does not survive one although the ROM's `__pre_init` zeroing it only on POWERON proves silicon keeps it |
| backend-contract-divergence | 2026-09-13 | [the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty](2026-09-13-the-s3-link-drops-the-io-tasks-next-chunk-on-a-stale-serial-in-empty.md) | fixed | fw-esp32s3 io_task (fw-esp32c6 now gated too, via #795) × esp-hal 1.1.1 write_async: two writers on USB-Serial-JTAG's one send buffer; esp-hal writes without checking it is free and wakes on a stale `serial_in_empty`. The link model was right (the TRM: refused while pending); fixed by a firmware gate, upstream issue drafted |
| state-conflation | 2026-09-09 | [browser-crash-reported-as-test-failure](2026-09-09-browser-crash-reported-as-test-failure.md) | fixed | scripts/browser-test-harness.sh: `wasm-bindgen-test-runner` says `Error: some tests failed` whether a test failed or headless Firefox was SIGKILLed mid-run |
| open-path-wait-without-wakeup | 2026-09-08 | [a-held-lens-waited-for-a-sim-nobody-was-going-to-start](2026-09-08-a-held-lens-waited-for-a-sim-nobody-was-going-to-start.md) | fixed | lpa-studio-core studio_controller (try_pending_device_lens, resolve_open_device, seed_device_sim_records) — prod outage, no project could open |
| write-ordering | 2026-10-08 | [littlefs-rust-relocation-cut-leaves-lpfs-unmountable](2026-10-08-littlefs-rust-relocation-cut-leaves-lpfs-unmountable.md) | **open** | littlefs-rust 0.1.0 metadata-pair relocation: with `block_cycles=1` a cut sweep of F1 `save:c20` (lp-nor-sim) leaves `lpfs` unmountable in 18 of 1,388 cases; not reachable at the firmware's `block_cycles = -1`, and the reason `block_cycles` stays unset; F3 (`block_cycles=1`, `save:c40`) loses `/.lp` after the re-run in 16–17 of 3,066 |
| write-ordering | 2026-09-07 | [merge-delete-erased-the-merged-row](2026-09-07-merge-delete-erased-the-merged-row.md) | fixed | lpa-devices roster (reconcile_identities) + lpa-studio-core studio_controller (settle_device_records) |
| assumed-context | 2026-09-06 | [c6-first-flash-bootloader-hang-lp-analog-i2c-clock](2026-09-06-c6-first-flash-bootloader-hang-lp-analog-i2c-clock.md) | fixed | lpa-link flashers + lpa-devices reconnect ladder: a fresh C6's factory firmware gates the LP analog I2C clock; our bootloader hangs after every HP-only reset until a replug (the fix; see [c6-analog-master-wedges-the-bootloader](2026-09-06-c6-analog-master-wedges-the-bootloader.md) for the bench diagnosis, now also reproduced with no board in `lp-emu-esp32c6/tests/bootloader_hang.rs`) |
| stand-in-divergence | 2026-09-10 | [the-emulator-ran-the-rom-reset-path-on-the-app-core](2026-09-10-the-emulator-ran-the-rom-reset-path-on-the-app-core.md) | fixed | lp-emu-esp32v3 `service_app_core_start` modelled the `appcpu_resetting` pulse as a reset through the mask ROM, so the APP core re-unpacked `.data_xtos_pro` and zeroed `.bss_xtos_pro` over the firmware's live heap region 0 and the shipped image died in `LpFs::read_file`. The bench (L2, PR #695) found silicon rewrites **not one byte** of that span: `changed=0`, `verdict=B`, twice. The firmware was never at fault — this entry was first filed against it. Core 1 now starts at `appcpu_boot_addr` with no ROM code run; **why** the pulse does not re-run the ROM is still owed |
| toolchain-miscompile | 2026-09-07 | [metal-drops-atomic-guarded-by-loop-exit-sum](2026-09-07-metal-drops-atomic-guarded-by-loop-exit-sum.md) | fixed | lp-gfx-wgpu loop_bound_pass → Metal: a conditional `atomicAdd` guarded by the loop's pre-store exit sum never executes; storing first and reading back fixes every shape |
| contract-gap | 2026-09-06 | [gpu-tier-executes-unbounded-shaders](2026-09-06-gpu-tier-executes-unbounded-shaders.md) | fixed | lp-gfx-wgpu GPU tiers + catalog: `fault-demo`'s `while (true)` has no fuel meter on a GPU — the driver watchdog resets the device, corrupts sibling surfaces, and can take the OS down |
| nondeterministic-capture | 2026-09-06 | [heap-budget-capture-truncated-by-cycle-cap](2026-09-06-heap-budget-capture-truncated-by-cycle-cap.md) | fixed | scripts/heap-budget-check.sh: the startup capture hit `--max-cycles` mid-compile and recorded the cut as a figure |
| stand-in-divergence | 2026-09-04 | [read-gate-refuses-on-largest-block-proxy](2026-09-04-read-gate-refuses-on-largest-block-proxy.md) | **open** | lpa-server ProjectRead headroom gate vs the classic's two-region heap |
| stand-in-divergence | 2026-09-04 | [unload-leaves-classic-unloadable-until-power-cycle](2026-09-04-unload-leaves-classic-unloadable-until-power-cycle.md) | **open** (not reproducing on the four-region heap 2026-09-06: reload accepted at 72,954 B, margin ~7 KB) | after stopAllProjects the classic's largest block is 39.7 KB and the 64 KiB load gate refuses every load |
| crash-loop | 2026-09-04 | [tlsf-build-hits-stack-guard-at-project-load](2026-09-04-tlsf-build-hits-stack-guard-at-project-load.md) | **open** | fw-esp32v3 with esp-alloc TLSF panics on the stack-guard watchpoint during project load |
| assumed-context | 2026-09-02 | [flash-from-running-board-parks-until-reset](2026-09-02-flash-from-running-board-parks-until-reset.md) | open | lpa-devices Flash activity post-write wait; browser flasher closing reset |
| state-conflation | 2026-09-04 | [pre-flash-hello-stamps-over-a-closed-port](2026-09-04-pre-flash-hello-stamps-over-a-closed-port.md) | fixed | lpa-devices Flash activity ladder → manifest stamp; evidence window hello timestamp |
| assumed-context | 2026-09-04 | [classic-ooms-decoding-the-manifest-write](2026-09-04-classic-ooms-decoding-the-manifest-write.md) | fixed | fw-esp32v3 wire request decode under an auto-loaded project; Flash stamp timeout copy |
| stand-in-divergence | 2026-09-02 | [ignored-emu-fuel-probe-renders-black-on-first-probe](2026-09-02-ignored-emu-fuel-probe-renders-black-on-first-probe.md) | open | lpc-engine compile-window deferral vs render probes; fw-tests recovery_emu (ignored) |
| state-conflation | 2026-09-01 | [silent-black-under-node-quarantine](2026-09-01-silent-black-under-node-quarantine.md) | fixed | lpc-engine node status + output fallback; lpa-devices heartbeat mirror |
| misattributed-symptom | 2026-08-31 | [c6-rmt-ws281x-dark](2026-08-31-c6-rmt-ws281x-dark.md) | harness fixed (#491); app half fixed (#495 heap, #496 fault pattern + card) | fw-esp32c6 harness serial io + lpc-engine shader node under lp-recovery |
| unit-mismatch | 2026-08-24 | [map2d-sample-diameter-unit-mismatch](2026-08-24-map2d-sample-diameter-unit-mismatch.md) | fixed | lpc-engine map2d resolve + ResolvedMappingCompact consumers |
| lifecycle-ownership | 2026-08-14 | [post-acquire-open-failure-leaks-the-project-lock](2026-08-14-post-acquire-open-failure-leaks-the-project-lock.md) | fixed | lpa-studio-web library_host_opfs + lpa-studio-core project_controller |
| lock-held-across-foreign-latency | 2026-08-14 | [sync-holds-the-project-lock-across-the-network](2026-08-14-sync-holds-the-project-lock-across-the-network.md) | fixed | lpa-studio-web cloud/sync + library_host_opfs |
| fixed-budget-over-variable-work | 2026-08-14 | [worker-boot-timeout-races-the-wasm-fetch](2026-08-14-worker-boot-timeout-races-the-wasm-fetch.md) | fixed (P3 recovery, P4 scheduling, P5 inactivity budget) | lpa-link browser_worker + lpa-studio-core preview_host |
| state-conflation | 2026-08-14 | [sibling-module-bus-tie-blanks-preview](2026-08-14-sibling-module-bus-tie-blanks-preview.md) | **open** | lpc-engine (bus resolution) + fw-browser preview runtime |
| newest-only-inflight-memory | 2026-08-13 | [stale-echo-reseeded-dive-session](2026-08-13-stale-echo-reseeded-dive-session.md) | fixed | lpa-studio-web editor_shell (mapping_session pipeline) |
| config-masked-defect | 2026-08-05 | [generated-palette-header-dies-on-naga](2026-08-05-generated-palette-header-dies-on-naga.md) | fixed | lps-frontend (parse.rs) + lpc-model shader_header_gen |
| config-masked-defect | 2026-08-21 | [hello-gate-assumes-fresh-boot](2026-08-21-hello-gate-assumes-fresh-boot.md) | **open** | lpa-link device_session (readiness) + fw server_loop heartbeat |
| config-masked-defect | 2026-08-29 | [lamp-views-latch-one-output](2026-08-29-lamp-views-latch-one-output.md) | fixed | lpa-studio-core lamp compositors (card feed, preview feed, module hero) |
| state-conflation | 2026-08-28 | [wire-load-skips-link-engine-state](2026-08-28-wire-load-skips-link-engine-state.md) | fixed | lpa-server (wire load handler) + lpc-engine (display-layout budget) |
| assumed-context | 2026-08-24 | [power-gate-black-scan-counts-alpha](2026-08-24-power-gate-black-scan-counts-alpha.md) | **open** | fw-esp32-common output/power_gate (is_all_black) |
| unenforced-test-precondition | 2026-09-08 | [usb-gates-pin-a-heartbeat-millisecond](2026-09-08-usb-gates-pin-a-heartbeat-millisecond.md) | **open** (the image half closed by [cold-target-dir-links-esp-hals-stock-rodata](2026-09-08-cold-target-dir-links-esp-hals-stock-rodata.md); the precondition stands) | lp-emu-esp32c6 `tests/usb_control.rs` G3-1/G3-1b: an exact `"uptime_ms":5000` pins a millisecond the firmware's loop only samples by luck |
| unenforced-test-precondition | 2026-08-05 | [cross-core-panic-races-the-isr-thread](2026-08-05-cross-core-panic-races-the-isr-thread.md) | fixed | lp-fw/lp-ws281x tests (cross_core) |
| reclaim-ordered-behind-its-own-rebuild | 2026-08-04 | [compile-window-drops-rebuilt-before-compile](2026-08-04-compile-window-drops-rebuilt-before-compile.md) | fixed | lpc-engine nodes (fixture + output pressure handlers) |
| assumed-context | 2026-08-02 | [provisioning-flashes-one-image-unchecked](2026-08-02-provisioning-flashes-one-image-unchecked.md) | fixed | lpa-link serial ESP32 providers + lpa-boards + justfile |
| opt-in-degradation | 2026-08-01 | [xt-builtins-image-strands-just-test](2026-08-01-xt-builtins-image-strands-just-test.md) | fixed | justfile (`ci-prereqs`/`test`) + build-builtins-xt.sh + lpvm-native tests |
| config-masked-defect | 2026-08-01 | [xtlpn-f32-loses-writes-to-value-parameters](2026-08-01-xtlpn-f32-loses-writes-to-value-parameters.md) | fixed | lpvm-native lowering (lower_f32.rs) |
| unsynchronized-shared-artifact | 2026-08-02 | [serial-line-interleaving](2026-08-02-serial-line-interleaving.md) | fixed (PR #884, wire proto 32: io_task is UART0's one writer after boot, fed whole link frames and whole telemetry lines; `[MEM]`/`[JIT]`/`[stack]` are log records; panic text follows a `0xFF` mark — emulator-validated, desk walk pending) | fw-esp32v3 UART0 (esp_println + log + transport) |
| precision-loss-at-a-seam | 2026-08-01 | [gamma-8bit-choke](2026-08-01-gamma-8bit-choke.md) | fixed | lpc-engine fixture node |
| misattributed-symptom | 2026-08-01 | [classic-rmt-open-fault](2026-08-01-classic-rmt-open-fault.md) | fixed | lpc-shared DisplayPipeline + fw-esp32-common provider |
| capacity-regression | 2026-08-01 | [classic-heap-regression-after-f32-merge](2026-08-01-classic-heap-regression-after-f32-merge.md) | fixed | lpc-engine resolver (payload caches) + fw-esp32v3 gate |
| metadata-parsed-but-never-enforced | 2026-08-02 | [browser-flash-never-checks-the-chip](2026-08-02-browser-flash-never-checks-the-chip.md) | fixed | lpa-link browser_esp32_flash.js (flashFirmware) |
| proxy-signal-outranks-the-real-outcome | 2026-08-02 | [erase-fails-a-successful-erase-on-flash-id-noise](2026-08-02-erase-fails-a-successful-erase-on-flash-id-noise.md) | fixed | lpa-link browser_esp32_flash.js (eraseDeviceFlash) |
| allocator-refuses-a-request-it-can-serve | 2026-08-02 | [classic-oom-retry-succeeds](2026-08-02-classic-oom-retry-succeeds.md) | **open** | fw-esp32v3 esp-alloc/LLFF + lps-glsl typeck |
| test-rig-lies-about-its-subject | 2026-08-01 | [xt-pipeline-rigs-declare-param-types-as-return-types](2026-08-01-xt-pipeline-rigs-declare-param-types-as-return-types.md) | fixed (f32 rig; Q32 rig outstanding) | lpvm-native tests + lpir::builder |
| model-conflation | 2026-08-01 | [xt-f32-builtins-exhaust-the-emulator-code-region](2026-08-01-xt-f32-builtins-exhaust-the-emulator-code-region.md) | fixed | lp-xt-emu (board/memory) + lps-builtins-xt-app + lpvm-native/rt_emu |
| upstream-toolchain-limitation | 2026-08-01 | [xtensa-backend-cannot-select-float-constant-pool](2026-08-01-xtensa-backend-cannot-select-float-constant-pool.md) | **open** (worked around) | lps-builtins + esp Rust toolchain |
| invented-encoding | 2026-07-31 | [zexth-encoding-steals-xori-128](2026-07-31-zexth-encoding-steals-xori-128.md) | fixed | lp-riscv-inst (encode/decode) + lp-riscv-emu (executor) |
| invented-encoding | 2026-07-31 | [elf-loader-riscv-reloc-numbering](2026-07-31-elf-loader-riscv-reloc-numbering.md) | **open** | lp-riscv-elf (relocations) |
| invented-encoding | 2026-09-09 | [zbs-funct6-hex-mistranscribed-in-the-disassembler](2026-09-09-zbs-funct6-hex-mistranscribed-in-the-disassembler.md) | fixed | lp-riscv-inst (decode/encode/inst) |
| partial-knowledge-loss | 2026-07-31 | [elf-loader-drops-relocation-addends](2026-07-31-elf-loader-drops-relocation-addends.md) | fixed | lp-riscv-elf (relocations) |
| incomplete-subset | 2026-07-31 | [mksadj-missing-from-fp-subset](2026-07-31-mksadj-missing-from-fp-subset.md) | fixed | lp-xt/lp-xt-inst (FP subset) |
| split-source-of-truth | 2026-07-30 | [jit-sret-return-count-zero](2026-07-30-jit-sret-return-count-zero.md) | fixed | lpvm-native/rt_jit (module.rs) |
| split-source-of-truth | 2026-08-07 | [wasm-f32-unorm-scale-convention](2026-08-07-wasm-f32-unorm-scale-convention.md) | fixed | lpvm-wasm/emit (F32 unorm lowering) |
| config-masked-defect | 2026-07-30 | [xtensa-call-argument-clobber](2026-07-30-xtensa-call-argument-clobber.md) | fixed | lpvm-native/regalloc (walk.rs) |
| config-masked-defect | 2026-07-30 | [xtensa-sret-pointer-clobber](2026-07-30-xtensa-sret-pointer-clobber.md) | fixed | lpvm-native/regalloc (pool.rs) |
| config-masked-defect | 2026-07-30 | [xtensa-stack-arg-staged-over](2026-07-30-xtensa-stack-arg-staged-over.md) | fixed | lpvm-native/regalloc (walk.rs) |
| config-masked-defect | 2026-07-30 | [xtensa-two-value-return-clobber](2026-07-30-xtensa-two-value-return-clobber.md) | fixed | lpvm-native/regalloc (walk.rs) |
| config-masked-defect | 2026-07-30 | [xtensa-integer-div-by-zero-trap](2026-07-30-xtensa-integer-div-by-zero-trap.md) | fixed | lpvm-native lowering (lower.rs) |
| config-masked-defect | 2026-07-31 | [opt-z-missed-rmt-drain-deadline](2026-07-31-opt-z-missed-rmt-drain-deadline.md) | fixed | workspace release profile / fw-esp32s3 |
| backend-contract-divergence | 2026-10-08 | [littlefs-rust-compacts-every-metadata-commit](2026-10-08-littlefs-rust-compacts-every-metadata-commit.md) | **open** | littlefs-rust 0.1.0 `lfs_dir_commitcrc` never writes the FCRC tag, so every commit compacts: one sector erase per file write (30 simulated days of panel writes, lp-nor-sim: 21,602 erases on one sector); fix in light-player/littlefs-rust, with its 256-tag fetch cap raised |
| backend-contract-divergence | 2026-10-08 | [littlefs-rust-block-cycles-overflows-under-overflow-checks](2026-10-08-littlefs-rust-block-cycles-overflows-under-overflow-checks.md) | **open** | littlefs-rust 0.1.0 `lfs_dir_alloc`: with `block_cycles > 0` an erased block's revision (`0xffff_ffff`) is aligned up with a plain `+` where C wraps, so a build with overflow checks on cannot format an erased partition; release builds (the firmware) wrap as C does. Found by F3's `cargo test`; pinned there |
| backend-contract-divergence | 2026-08-09 | [gpu-render-pass-floors-the-fragment-center](2026-08-09-gpu-render-pass-floors-the-fragment-center.md) | **open** | lp-gfx-wgpu assembly (generated fragment main) |
| backend-contract-divergence | 2026-07-30 | [q32-native-vs-wasmtime-last-bit](2026-07-30-q32-native-vs-wasmtime-last-bit.md) | **open** | lpvm-native / lpvm-wasm (Q32 execution) |
| backend-contract-divergence | 2026-07-17 | [deletedir-error-shape](2026-07-17-deletedir-error-shape.md) | fixed | lpa-server + lpa-client |
| backend-contract-divergence | 2026-07-22 | [littlefs-listdir-doubled](2026-07-22-littlefs-listdir-doubled.md) | fixed | fw-esp32/fs |
| backend-contract-divergence | 2026-07-27 | [created-package-unloadable](2026-07-27-created-package-unloadable.md) | fixed | lpa-studio-core/library |
| budget-exhaustion | 2026-07-28 | [esp32c6-app-partition-overflow](2026-07-28-esp32c6-app-partition-overflow.md) | **open** (mitigated −42 KB) | lp-fw/fw-esp32 (partitions) |
| unbounded-payload-on-bounded-transport | 2026-08-04 | [oversized-display-layout-wedges-project-read](2026-08-04-oversized-display-layout-wedges-project-read.md) | fixed | lpc-engine probe + lpc-shared transport |
| ungated-variant | 2026-07-28 | [fw-esp32-harnesses-rotted-uncompiled](2026-07-28-fw-esp32-harnesses-rotted-uncompiled.md) | fixed | lp-fw/fw-esp32c6 (src/tests/ + cfg gates) |
| ungated-variant | 2026-07-30 | [stacked-prs-get-no-ci](2026-07-30-stacked-prs-get-no-ci.md) | fixed | .github/workflows/pre-merge.yml (trigger) |
| lifecycle-ownership | 2026-07-16 | [browser-serial-endpoint-lost](2026-07-16-browser-serial-endpoint-lost.md) | fixed | lpa-link/registry |
| lifecycle-ownership | 2026-07-22 | [flash-session-map-deleted](2026-07-22-flash-session-map-deleted.md) | fixed | lpa-link/browser-serial |
| state-conflation | 2026-08-13 | [evicted-visible-slot-frozen](2026-08-13-evicted-visible-slot-frozen.md) | fixed | lpa-studio-core/preview_host |
| state-conflation | 2026-08-04 | [unbound-shader-uniform-warns](2026-08-04-unbound-shader-uniform-warns.md) | fixed | lpc-engine engine host + shader nodes |
| state-conflation | 2026-07-17 | [unreadable-masqueraded-as-empty](2026-07-17-unreadable-masqueraded-as-empty.md) | fixed | lpa-studio-core/roster |
| state-conflation | 2026-07-22 | [read-failure-vs-unreadable-content](2026-07-22-read-failure-vs-unreadable-content.md) | **open** | lpa-studio-core/roster |
| state-conflation | 2026-07-26 | [worker-poisoned-instance-reuse](2026-07-26-worker-poisoned-instance-reuse.md) | fixed | fw-browser + lpa-link/browser-worker |
| state-conflation | 2026-07-28 | [playlist-entry-selection](2026-07-28-playlist-entry-selection.md) | fixed | lpa-studio-core/project (node face derivation) |
| assumed-context | 2026-07-17 | [storage-slot-assumed](2026-07-17-storage-slot-assumed.md) | fixed | lpa-studio-core/places |
| assumed-context | 2026-07-23 | [deploy-dialog-ignores-running-project](2026-07-23-deploy-dialog-ignores-running-project.md) | fixed | lpa-studio-core/device |
| assumed-context | 2026-07-27 | [launch-json-pinned-port](2026-07-27-launch-json-pinned-port.md) | fixed | dev tooling (launch.json + dev-port.sh) |
| assumed-context | 2026-07-30 | [vacuity-guard-tripped-on-color](2026-07-30-vacuity-guard-tripped-on-color.md) | fixed | .github/workflows/pre-merge.yml (Xtensa gate) |
| assumed-context | 2026-08-19 | [popover-entrance-parks-without-frames](2026-08-19-popover-entrance-parks-without-frames.md) | fixed | lpa-studio-web/base/popover |
| assumed-context | 2026-08-24 | [select-value-lands-before-options-mount](2026-08-24-select-value-lands-before-options-mount.md) | fixed | lpa-studio-web (every rsx select) |
| partial-knowledge-loss | 2026-07-22 | [identity-lost-on-failed-read](2026-07-22-identity-lost-on-failed-read.md) | fixed | lpa-studio-core/places+studio |
| partial-knowledge-loss | 2026-07-23 | [reconnect-transient-twin-card](2026-07-23-reconnect-transient-twin-card.md) | fixed | lpa-studio-core/home + device |
| policy-leak | 2026-07-17 | [hardware-attach-opened-editor](2026-07-17-hardware-attach-opened-editor.md) | fixed | lpa-studio-core/studio |
| stand-in-divergence | 2026-07-23 | [popover-open-resizes-card](2026-07-23-popover-open-resizes-card.md) | fixed | lpa-studio-web/base/popover |
| stand-in-divergence | 2026-07-27 | [story-check-tolerance-ignores-amplitude](2026-07-27-story-check-tolerance-ignores-amplitude.md) | **open** | lpa-studio-web/scripts + CI |
| stand-in-divergence | 2026-10-03 | [an-emulated-power-button-board-powered-off-on-detach](2026-10-03-an-emulated-power-button-board-powered-off-on-detach.md) | fixed | lpa-studio-web `?emu=` shim: an undriven D0 read "switch off", so a power-button project's board deep-slept on `detach` with no wake; the banner now holds a D0 switch on (`pin 0 1`), re-sent after reboots |
| stand-in-divergence | 2026-10-06 | [the-virtual-lans-forward-kept-a-moved-boards-old-connection](2026-10-06-the-virtual-lans-forward-kept-a-moved-boards-old-connection.md) | fixed | lp-emu-esp-common `seam::net` port forward: after a board moved to a new lease, a forwarded connection to its old address kept asking ARP once a second and held smoltcp's stack-wide ARP rate limit, so no new connection reached the board (walk W9); connections now close when the lease moves, and every forward socket times out after 60 s with data outstanding |
| nondeterministic-capture | 2026-07-28 | [overview-composite-capture-races](2026-07-28-overview-composite-capture-races.md) | fixed | lpa-studio-web story capture (overview composites) |
| retired-surface-still-reachable | 2026-07-28 | [retired-device-pane-still-reachable](2026-07-28-retired-device-pane-still-reachable.md) | fixed | lpa-studio-core/home + studio_shell |
| stale-measurement | 2026-07-30 | [deploy-compiles-previous-upload](2026-07-30-deploy-compiles-previous-upload.md) | **fixed** (CLI-side; hardware confirmation pending P7) | lp-cli (upload observability) |
| stale-measurement | 2026-07-26 | [popover-outline-stale-on-content-resize](2026-07-26-popover-outline-stale-on-content-resize.md) | fixed | lpa-studio-web/base/popover |
| stale-measurement | 2026-07-27 | [code-editor-gutter-misaligned](2026-07-27-code-editor-gutter-misaligned.md) | fixed | lpa-studio-web/base/code_editor |
| stale-measurement | 2026-08-05 | [clock-face-baselines-oscillate](2026-08-05-clock-face-baselines-oscillate.md) | fixed | lpa-studio-web story capture (clock-face stories) |
| inline-emit-stack-imbalance | 2026-07-27 | [wasm-q32-fabs-stack-leak](2026-07-27-wasm-q32-fabs-stack-leak.md) | fixed | lpvm-wasm emit (+ lpvm-cranelift trunc) |
| untested-path | 2026-07-27 | [cranelift-q32-floor-ceil](2026-07-27-cranelift-q32-floor-ceil.md) | fixed | lpvm-cranelift q32_emit (rv32c) |
| untested-path | 2026-08-02 | [f32-shader-cannot-render-a-frame](2026-08-02-f32-shader-cannot-render-a-frame.md) | fixed | lpvm hot path (native JIT, wasmtime, browser) |
| silent-drop | 2026-07-28 | [flash-progress-never-reached-the-ui](2026-07-28-flash-progress-never-reached-the-ui.md) | fixed | lpa-studio-core (actor/controller) |
| silent-drop | 2026-07-31 | [loader-silently-drops-unparseable-nodes](2026-07-31-loader-silently-drops-unparseable-nodes.md) | fixed | lpc-engine loader + flush + virtual ws281x |
| silent-drop | 2026-08-03 | [dev-file-sync-drops-on-uart-rx-overflow](2026-08-03-dev-file-sync-drops-on-uart-rx-overflow.md) | fixed structurally (PR #884: lp-link's ARQ makes an RX FIFO overflow a counted resend; the FIFO can still overflow — emulator-validated, desk walk pending) | lp-cli/src/commands/dev (fs sync) + fw-esp32v3 UART0 RX |
| silent-drop | 2026-08-26 | [inbound-frames-longer-than-a-tick-lossy](2026-08-26-inbound-frames-longer-than-a-tick-lossy.md) | fixed structurally (PR #884: a long message is 256 B link frames, each resent if damaged; the loss mechanism is still unpinned, and the RX-ring "design answer" is now optional latency work — emulator-validated, desk walk pending) | fw-esp32v3 UART0 RX (post #448 executor isolation) |
| silent-drop | 2026-08-07 | [boot-compile-oom-crash-loop](2026-08-07-boot-compile-oom-crash-loop.md) | **open** | fw-esp32v3 boot-compile + lp-cli upload + lpfs partition |
| silent-drop | 2026-08-28 | [auto-publish-outcomes-invisible](2026-08-28-auto-publish-outcomes-invisible.md) | fixed (visibility + 5xx drop; legacy-library gap open) | lpa-studio-web cloud sync |
| timeout-scoped-to-sub-phase | 2026-08-07 | [upload-wait-timeout-unbounded-deploy](2026-08-07-upload-wait-timeout-unbounded-deploy.md) | **open** | lp-cli upload (deploy wait) |
| timeout-scoped-to-sub-phase | 2026-08-24 | [request-idle-budget-blind-to-dropped-responses](2026-08-24-request-idle-budget-blind-to-dropped-responses.md) | fixed | lpa-link device_client_io + client correlation |
| unbounded-restatement | 2026-07-28 | [tick-error-restated-every-frame](2026-07-28-tick-error-restated-every-frame.md) | fixed | lpa-server (advance_frame) |
| unsynchronized-shared-artifact | 2026-07-29 | [builtins-elf-uplift-race](2026-07-29-builtins-elf-uplift-race.md) | fixed | justfile `test` + lpvm-cranelift/build.rs |
| missing-coverage | 2026-07-29 | [uniform-struct-array-runtime-index](2026-07-29-uniform-struct-array-runtime-index.md) | fixed | examples/effects/meteor + lps-frontend lowering |
| arena-retained-transient | 2026-08-29 | [shader-jit-compile-transient-starves-classic-heap](2026-08-29-shader-jit-compile-transient-starves-classic-heap.md) | fixed (silicon bracket 2026-09-06: compile retains 2,160 B, zook green at 19 fps) | lps-glsl HIR build transient vs the classic's arena; probes `xt_compile_peak_memory`, `example_shader_compile_peak_memory` |
| arena-retained-transient | 2026-09-01 | [hir-place-clones-exhaust-c6-heap-at-compute-compile](2026-09-01-hir-place-clones-exhaust-c6-heap-at-compute-compile.md) | fixed (recurrence 2026-09-02 on every other node kind — fixed, PR #497; module-wide interning open) | lps-glsl hir/typeck + hir/place + lower/place; lpc-engine shader nodes ([mem] bracket) |
| deadline-margin-by-accident | 2026-09-02 | [c6-ws281x-first-three-leds-then-stale](2026-09-02-c6-ws281x-first-three-leds-then-stale.md) | fixed | fw-esp32c6 output/rmt + lp-ws281x refill path placement |
| untested-path | 2026-09-02 | [studio-flasher-cannot-recover-a-boot-looping-c6](2026-09-02-studio-flasher-cannot-recover-a-boot-looping-c6.md) | **open** | lpa-studio-web device card flash flow (esptool-js ladder) |
| lifecycle-ownership | 2026-09-02 | [same-gpio-rebind-disconnects-the-pad](2026-09-02-same-gpio-rebind-disconnects-the-pad.md) | fixed | fw-esp32c6 + fw-esp32s3 output/rmt `bind_channel` (esp-hal `with_pin` guard order) |
| untested-path | 2026-09-06 | [emu-transport-drops-unprefixed-client-lines](2026-09-06-emu-transport-drops-unprefixed-client-lines.md) | fixed | lpa-client transport_serial/emulator (async `M!` framing) + lp-cli `emu` host spec |
| shared-namespace-collision | 2026-09-08 | [a-stray-hello-answered-a-request-that-never-asked](2026-09-08-a-stray-hello-answered-a-request-that-never-asked.md) | fixed | lpa-client protocol_session/client/tokio_client/project_read_stream (correlation) |
| lifecycle-ownership | 2026-09-08 | [serial-close-leaks-the-port-on-a-wedged-device](2026-09-08-serial-close-leaks-the-port-on-a-wedged-device.md) | fixed | lpa-client stream/serialport_stream + transport_serial framing thread/close; lpa-link host_serial_esp32 provider + DeviceSession::release_link |
| assumed-context | 2026-09-11 | [a-reset-replays-the-boards-lifetime](2026-09-11-a-reset-replays-the-boards-lifetime.md) | fixed by #737 (`4940c2b53`); **classic (`lp-emu-esp32v3`) has the same shape, still open** | lp-emu/esp/lp-emu-esp32c6 machine.rs (`run_until`, `reboot`) |
| partial-knowledge-loss | 2026-09-11 | [forget-leaves-the-emu-flash-image-in-opfs](2026-09-11-forget-leaves-the-emu-flash-image-in-opfs.md) | fixed by #715 via #728 (`0560344fa`) | lpa-studio-web public/lpa-link/emulator_worker.js (`deleteFlash`) |
| unexplained-transient-stall | 2026-09-11 | [the-tab-walks-upload-step-flaked-once-in-three](2026-09-11-the-tab-walks-upload-step-flaked-once-in-three.md) | **open** (undiagnosed) | scripts/emu/walk-no-board.mjs — the tab-backed upload step |
| bound-in-a-foreign-unit | 2026-09-13 | [the-dilation-window-drains-one-shift-at-a-time](2026-09-13-the-dilation-window-drains-one-shift-at-a-time.md) | fixed | lpa-studio-web `public/lpa-link/emulator_worker.js` (the tab backing's pacing loop) |
| lifecycle-ownership | 2026-09-13 | [a-stabilization-timer-outlived-the-popover](2026-09-13-a-stabilization-timer-outlived-the-popover.md) | fixed | lpa-studio-web `base/popover.rs`: a `forget()`-ed `setTimeout` (plus the fonts-ready future and an already-queued observer rAF) measured into signals the popover's scope had dropped — twelve panics under a green walk |
| assumed-context | 2026-10-07 | [three-firmware-chips-painted-over-each-other-in-a-narrow-card](2026-10-07-three-firmware-chips-painted-over-each-other-in-a-narrow-card.md) | fixed (9b3cad436) | lpa-studio-web `device_roster_card.rs`: the firmware row was a fixed-height nowrap flex row sized for two chips; "Other version…" beside another install verb made three, painted over each other in a narrow card (found by `walk-ota-emu --steps install-older`). The row wraps when an install verb sits beside Update or Reinstall |
| state-conflation | 2026-10-08 | [reset-was-disabled-on-a-board-on-wifi](2026-10-08-reset-was-disabled-on-a-board-on-wifi.md) | fixed (this change) | lpa-studio-core `device_offers` × lpa-devices `ResetBoard`: Reset read `is_over_bluetooth()`, which is `firmware_blocked` and so true on every network link, and drew "Reset needs USB" on a Wi‑Fi board; the model only had the line reset. Over Bluetooth, the LAN and the relay, Reset is now the wire's `Reboot` (enabled for the author tier), and the card comes back on the redialled link with no click |
| state-conflation | 2026-10-07 | [a-relay-card-said-bluetooth-and-usb-connected](2026-10-08-a-relay-card-said-bluetooth-and-usb-connected.md) | fixed (network transport PR C) | lpa-studio-web `device_roster_card` × lpa-studio-core `lan_link_view`: a board through the relay took its link kind from `is_over_bluetooth()` (= `firmware_blocked`, true on every network link) and said "No live picture over Bluetooth", "USB connected" and "Connecting over Bluetooth…". Core's network line now covers `relay:` and carries the kind ("Wi‑Fi via lightplayer.app"); the second conflation of that predicate in a day |
| config-masked-defect | 2026-10-08 | [a-studio-update-burst-past-the-send-ring-was-dropped](2026-10-08-a-studio-update-burst-past-the-send-ring-was-dropped.md) | fixed (OTA Wi‑Fi PR B) | lpa-link `link_port_service.rs`: channel-3 messages the 24 KiB send ring refused with `Full` were dropped as errors; USB and Bluetooth (4 ahead) never filled it, the LAN (8 ahead of raw 4 KiB restore chunks) did, and a Wi‑Fi restore stalled at 1 % (found by `walk-ota-emu --lan`). They now wait in an outbox, as lp-cli's host does |

## Predecessor: `docs/bugs/`

Two ad-hoc pre-registry writeups live in `docs/bugs/` (2026-03 JIT
filetest segfault, cranelift rv32 ld instruction). They stay where they
are as historical record; new entries belong here.
