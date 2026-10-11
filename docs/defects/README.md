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
that fixes a qualifying bug adds its entry. The new file is the whole
registry change: the index is built from entries' frontmatter (see
[Index](#index)), so there is no shared table to edit, and two PRs that
each file a defect never touch the same line. Closing a defect is an
edit to that entry's `status` and `fixed`, nothing else. `status: open`
entries are legal and expected for found-not-yet-fixed defects —
hardware-walk and live-debugging findings get a home immediately, before
anyone decides when to fix them.

## Entry template

The file name is `YYYY-MM-DD-slug.md`, and the frontmatter's `status`,
`found`, `area` and `class`, with the `# title` line, are what the index
shows. `just lint-defects` fails an entry missing any of them.

```markdown
---
status: fixed          # exactly open | fixed | wontfix; detail goes in a comment like this one
found: YYYY-MM-DD      # how: hardware-walk | live-debugging | ci | e2e | report
fixed: <commit>        # absent while open. NOTE: an entry cannot cite
                       # its OWN commit (the hash doesn't exist yet, and
                       # amending changes it) — write `fixed: this change`
                       # at commit time and fill the real hash in the NEXT
                       # commit that touches the registry.
area: <crate/module>
class: <one from the vocabulary>   # one kebab-case word
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
- **`marker-search-over-random-bytes`** — a test proves something about a
  stream's structure by searching its bytes for a marker (a framing prefix,
  a magic number), but the stream carries random or checksummed bytes, so
  the marker turns up by chance at a rate set by the stream's length. The
  fix shape is to parse the framing and assert on what the parser reports.

## Index

The index is built from each entry's frontmatter when you read it, and
it is never written down. Every defect PR used to add a row to a table
here, so any two of them open at once conflicted, and a conflicted PR
gets no CI at all
([`docs/debt/hand-written-defects-index.md`](../debt/hand-written-defects-index.md)).

```bash
just defects-index                    # every entry, newest first
just defects-index --by-class         # classes by count, then each one's entries
just defects-index --open             # open entries only (combines with the others)
just defects-index --class fidelity   # one class
```

`just lint-defects` checks that every entry carries what the index
reads. It runs in `just check-lint`, and in CI's "Defect registry" job,
which runs on a docs-only PR too. It also fails on a table row added back
here: a branch cut before the table went away re-adds its row when it
merges main, and the fix is to delete that row. A class missing from the
vocabulary above is not a failure; `--by-class` marks it with †.

Read it grouped by class, because a class that keeps recurring is the
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

## Predecessor: `docs/bugs/`

Two ad-hoc pre-registry writeups live in `docs/bugs/` (2026-03 JIT
filetest segfault, cranelift rv32 ld instruction). They stay where they
are as historical record; new entries belong here.
