---
status: fixed
found: 2026-09-25      # ci — Validate (x64) on #814 and #816, about 30 minutes apart
fixed: this change
area: lp-app/lpa-studio-core/src/app/studio/studio_device_e2e_tests.rs (`an_effect_that_outlives_its_activity_gives_the_wire_back_and_the_pump_resumes`) × lp-app/lpa-link/src/providers/fake_device/fake_device_core.rs
class: unenforced-test-precondition
related: [2026-09-08-a-stray-hello-answered-a-request-that-never-asked.md, lp-app/lpa-devices/src/activity/identify.rs]
---
# A late answer to identify's hello landed in the window the test needed silent

**Symptom** — `Validate (x64)` failed on two PRs that do not touch the crate,
#814 (run 36090520066) and #816 (run 36088722632), about 30 minutes apart. Both
failed in the same assertion, at `studio_device_e2e_tests.rs:4284`:

```
assertion `left == right` failed: the fold saw the port cycle its recovery performed:
  DeviceView { … status: Ready, state_label: "Ready", freshness_label: Some("last heard 1 s ago"), … }
  left: Ready
 right: NotResponding
```

Both passed on re-run, and the test passed 8/8 locally when run alone.

**Reproduction** — 96 copies of the test binary run concurrently, filtered to
this one test (the load stands in for a busy runner). It failed
**11/96, 6/96**, with the same assertion each time. After the fix: **0/96,
0/128, 0/128**.

**Root cause** — The test assumes that nothing the board says after the
eviction's port cycle can be a hello. It never checks that assumption.

1. `identified()` opens the port. `IdentifyActivity` sends its hello
   request, **id 1**. The fake board's server has already queued its
   unsolicited boot hello, **id 0**. Identify settles on that one, so the
   answer to id 1 is still pending.
2. The fake's `LpServer` runs on a **real thread** and answers in real
   time. The bench's clock is fake: 5 ms per step, about 1 ms of real time
   per step.
3. The hung push borrows the wire, and the link pump stops reading. The
   fake moves server frames onto the byte wire only when someone reads or
   writes, so an answer the server produces after this point stays in the
   server's channel.
4. The cancel grace evicts, and recovery does close then open.
   `FakeDeviceCore::reopen` clears the byte wire (`out`). It cannot clear the
   server's channel. The fresh window starts.
5. The pump reads again and delivers the server's late `id=1` hello into the
   fresh window. The fold sets `Classification::LightPlayer`, and the card
   reads **Ready**.

A trace from a failing run shows the order: `pump frame id=0 hello`, then
`client->server id=1`, then `reopen`, then `pump frame id=1 hello`. On an
idle machine the server answers id 1 within a few steps, while the pump is
still reading. The answer is heard in the *old* window, and the test passes.

This is the mechanism behind
[a-stray-hello-answered-a-request-that-never-asked](2026-09-08-a-stray-hello-answered-a-request-that-never-asked.md),
one layer further down. There, the straggler was already on the byte wire
and a borrowing client mistook it for its own answer. Here, it was still
inside the server and outlived a reopen.

**Not a product bug.** The fold reads a frame's *body*, not its id (see
`CLEAR_FAULTS_REQUEST_ID` in `lpa-devices/src/device.rs`). A late hello from
a board that was not reset in between is true and current, and it was heard
over the pump the test is checking. So Ready is honest. Real firmware can do
the same thing: a USB CDC board still holding an answer when the host
reopens sends it afterwards. The test was wrong to treat "a hello in the
fresh window" as proof of a stale verdict. That is only true if no answer
could still arrive.

**Fix** — The precondition is now set up explicitly, with no time budget
involved. `FakeEsp32Device::unanswered_requests()` counts the correlation ids
the server has received but has not yet put on the byte wire. The count is
incremented in `forward_to_server`, decremented in `pump_server_frames`, and
cleared on reset. Before starting the push, the test runs until that count is
zero. After that, every answer is either already read in the old window or
still in `out`, where the reopen clears it. Neither can reach the fresh window,
so the NotResponding assertion is deterministic again. No timeout was widened
and no clock was touched.

**Lesson** — A reopen clears only what is already on the wire. Anything the
fake board, or a real one, has not sent yet will arrive after the reopen. If a
test asserts that a fresh window is silent, it must first confirm that every
outstanding question has been answered. A fake with a real-thread server next
to a fake bench clock is a case of wall-clock dependence, even when every
timer in the test is injected.
