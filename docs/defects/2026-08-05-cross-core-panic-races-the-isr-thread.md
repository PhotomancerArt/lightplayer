---
status: fixed
found: 2026-08-05      # how: ci (reached `just test`, aborted the run)
fixed: this change
area: lp-fw/lp-ws281x tests (cross_core)
class: unenforced-test-precondition
related:
  - docs/adr/2026-08-04-rmt-isr-on-app-core.md
  - PR #346
---
# The cross-core unwind test raced the ISR thread it was testing against

**Symptom** — `lp-ws281x`'s
`cross_core::panicking_send_blocking_aborts_before_the_borrow_ends` failed
nondeterministically on macOS — 3 of 5 consecutive runs at first report, 2 of
30 in a clean measured loop:

```
thread '...' panicked at lp-fw/lp-ws281x/tests/cross_core.rs:183:9:
the spin must have panicked
```

It runs under bare `cargo test` (`mock` is a default feature, the crate is in
`default-members`), so it is inside `just test` via `test-rust-core`
(justfile:1487) and inside CI's `Validate (x64)`. A failure aborts the whole
`cargo test` invocation with exit 101, so **every test after it was skipped** —
one flaky concurrency test silently reduced the coverage of the entire run.

CI does not hide it. Run 30986885160's `Validate (x64)` log shows the test
compiling and running on the Linux runner (`test
panicking_send_blocking_aborts_before_the_borrow_ends ... ok`). It has simply
won the race there so far; the exposure is latent, not absent.

**Root cause** — the test drove `send_blocking` with a poll closure that
panicked on its **second** invocation, and nothing made a second invocation
happen. `send_blocking` checks `is_complete` before each `spin()`, and the
adversarial ISR thread — free-running, advancing three words and dispatching
`on_interrupt` in a tight loop — could stream the whole 6-px frame (145 words)
to completion while the main thread was descheduled. The first poll's own
`thread::yield_now()` is what handed it the CPU to do so. Instrumenting the
failure showed the mechanism exactly: `polls=1`, `send_result=Ok(())`,
`frames` incremented — the frame ran to `tx_end` under the main thread's feet,
the closure never reached its panic, and `catch_unwind` returned `Ok`.

The assertion was therefore checking the scheduler, not the driver. Worse, the
same gap voided the test in the other direction: nothing required the abort to
land while a handler pass was live, so a round in which the ISR had barely
started exercised a far weaker property than the doc comment claims — and
passed.

**Fix** — make the panic unconditional on a poll that is guaranteed to happen,
by construction rather than by timing. The ISR thread runs on a **pass budget**
(`spawn_metered_isr_thread`): it services an interrupt only while an
`AtomicIsize` is positive, spending one per pass. The test zeroes the budget
across `start_frame`, so with nothing advancing the transmitter the channel is
provably incomplete at `send_blocking`'s first `is_complete` check and the
first poll is reached. That poll grants a bounded number of passes, waits
(bounded, varying per round) for them to be spent so refills are actually
streaming, then reopens the budget and panics unconditionally — so the guard's
abort still races a live handler, which is the property the test exists for.
Only pass *entry* is metered, so an in-flight pass always drains and `abort`'s
`isr_seq` handshake cannot deadlock behind an exhausted budget. The frame grew
to 60 px for the same reason the headline test uses that size.

Metering **progress rather than time** is the load-bearing choice, and it was
not the first attempt. The first fix gated the ISR with a plain on/off flag and
asserted afterwards that no frame had completed (`stats(0).frames == 0`). That
passed 40/40 in isolation and then failed 2 in 20 as soon as all three tests in
the file ran together: the grant was a stretch of wall-clock, and one long
descheduling of the main thread let the ISR cover the whole 1441-word frame
inside it. Bounding words instead makes "the frame is still in flight" a
theorem — the ISR advanced at most `grant * 3` words however the threads were
scheduled — so it holds under any load. **The first fix reproduced the very
class it was fixing**, which is the strongest evidence available that the class
is worth naming.

Two assertions were tightened rather than loosened. The `is_err` check became a
`match` that reports the `Ok` payload, distinguishing "completed under us"
(`Ok(())`) from "never started" (`Err(Busy)`). And each round now checks that
the frame was still in flight when the unwind fired. That check is *recorded*
inside the spin and asserted outside it: an `assert!` in the closure would
unwind straight into the test's own `catch_unwind` and be read as the simulated
panic, hiding exactly what it was added to catch.

**Regression coverage** — the test is its own coverage; the change is what
makes it deterministic. Validated four ways: 0 failures in 25 consecutive runs
of the whole file (the configuration that broke the first attempt); 0 failures
in 48 runs under 6-way process contention; clean under the canonical Miri
invocation (`just ws281x-miri`, 55.4 s); and the **negative control still
fires** — with `AbortGuard` removed from `send_blocking`, Miri reports a data
race between the ISR thread's refill read and `drop(frame)` at
`cross_core.rs:292`. The test still catches the use-after-free it was written
for.

**Lesson** — a concurrency test's *precondition* deserves the same rigor as its
assertion. This one needed "at least two polls occur," and expressed it by
counting invocations of a closure whose invocation count is decided by the
scheduler. The failure mode is two-sided, and that is what makes the class
worth naming: the schedule that breaks the precondition either fails the
assertion spuriously (what we saw) or satisfies it vacuously (what we could not
see) — and the second is the expensive one, because it is silent.

The move is to stop hoping and start *holding*: give the adversary a throttle
the test owns, close it to establish the precondition by construction, open it
to create the race, and check an invariant that would be false if the race
never happened. The sharp edge is in the units. Throttling the adversary **by
time** — let it run, then reason about how far it got — is the same defect
wearing a control knob, because how far it got is still the scheduler's
decision; the first attempt here made exactly that mistake and flaked under
load. Throttle by **quantity of work**, and the bound survives any
interleaving. If the reasoning contains the words "long enough" or "should have
by now," it is a timing assumption in disguise.

A corollary about self-hiding checks: a test that wraps its subject in
`catch_unwind` cannot state its own invariants with `assert!` inside that
wrapper. The failure gets swallowed by the very mechanism under test and
reported as success. Record inside, assert outside.

The blast radius is a second lesson. `cargo test` aborts the run at the first
failing binary, so a flaky test does not cost one test — it costs every test
ordered after it, and the loss is invisible in a red run that everyone reads as
"the known flake."
