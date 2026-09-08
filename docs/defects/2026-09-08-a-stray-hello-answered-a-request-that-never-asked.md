---
status: fixed
found: 2026-09-08      # ci — Validate (x64) on main (5aa8dae55), after the P3 merge
fixed: this change
area: lp-app/lpa-client/src/protocol_session.rs (response_disposition)
class: shared-namespace-collision
related: [2026-08-24-request-idle-budget-blind-to-dropped-responses.md, lp-app/lpa-devices/src/activity/identify.rs, lp-app/lpa-studio-core/src/app/devices/shared_link_client_io.rs, lp2025/2026-09-07-0118-studio-emulated-boards]
---
# A hello the device model asked for became a push's answer, because they numbered their requests from the same 1

**Symptom** — Intermittent, on the serial bench, in whichever e2e row
happened to push right after an identify. Three different tests wore it in
one day (`a_refused_removal_leaves_the_running_face_and_says_why` locally
during P1, `the_empty_face_pushes_an_example_and_the_card_ends_up_running`
on CI run 34184122936 during P3, and
`the_feed_never_pulls_while_the_lens_holds_the_wire` on main), always with
the same card:

```
unexpected response for project.list_loaded: Hello(ServerHello { proto: 20,
  build: BuildFacts { … package: "fw-esp32c6", commit: "fake-firmware" … },
  … device_uid: Some("dev000000daqf6dvvt2") })
```

The device terminal reads: `Identifying` → ROM banner → the two `[INIT]`
lines → `heartbeat · idle` → `hello · proto 20` → outcome
`fw-esp32c6 fake-firmware` → `Sending the project` → `Asking the board what
it is running` → that failure. A board that had just identified perfectly
refused its first push question.

Reproduced at **12 failures in 40 runs** of
`the_feed_never_pulls_while_the_lens_holds_the_wire` on main.

**Root cause** — Two independent id spaces write to one wire, and the
client correlated on the id alone.

`IdentifyActivity` mints request ids from **its own counter starting at 1**
(`lpa-devices/src/activity/identify.rs:61`, `:91`), and
`byte_stream.rs:209` puts that id on the wire verbatim — the model's ids
are the wire's ids. Identify re-asks on an interval until it settles, and
it settles on whatever hello arrives first. The fake device (like real
firmware) announces itself **unsolicited, at id 0**, when its server loop
starts serving, so the identify routinely settles on the boot hello while
the answer to its own `Hello` **ask id 1** is still on the wire.

The push effect then borrows the port. The pump stops reading, but the
bytes already queued do not disappear — both readers drain the same FIFO.
The effect builds a plain `LpClient::new(io)`, whose `ProtocolSession`
**also starts at 1** (`protocol_session.rs:35`), and its first request is
`project.list_loaded` at id 1 (`device_push.rs:57`). The straggler hello
arrives bearing id 1, `response_disposition` saw `response.id ==
expected_id` and returned `Matched`, and `correlate_request`
(`client.rs:223`) handed a `Hello` body to a caller that had asked what
projects were loaded.

`ProtocolSession::starting_at` already existed for exactly this hazard —
the editor lens uses it, and the shared-link conversation mints ids at
`APP_CONVERSATION_ID_BASE` — but the borrow path never adopted it, and an
id base is a mitigation rather than a rule: it makes collisions unlikely,
not impossible. The rule that was missing is that **an id match is not
proof of an answer**. Nothing on hardware prevents this either: a board
that reboots mid-conversation (a flash, a brown-out) re-announces itself,
and a firmware answering a hello ask as an effect takes the port produces
the same frame.

**Did the P3 merge widen it?** Not by touching the mechanism. Between
`fff0f5540` and `5aa8dae55`, P3 changed nothing in `lpa-client`,
`lpa-devices`, `device_effects.rs`, `byte_stream.rs` or the fake device —
its `Opened`-before-boot-hello reordering is on the browser-worker/sim
path, which this failure does not use. What P3 did add is 397 lines of new
rows to `studio_device_e2e_tests.rs`. Those rows share a test binary with
this one and hold real wall-clock waits, so they lengthen the window in
which the fold's pump loses the drain race to the borrow. The race was
present before P3 (it was seen during P1); P3 made it easier to lose.

**Fix** — `ProtocolSession::response_disposition` now takes a `PendingAsk`
saying what the request in flight asked for, and rules out the bodies the
server sends on its **own initiative** — `Hello`, `Heartbeat`, `Log`
(exactly the set `ClientEvent::from_unsolicited_message` carries) — before
consulting the id at all. The one exception is a hello a client ASKED for:
`Hello` answers `ClientRequest::Hello` and nothing else. Such a frame
classifies as the new `ResponseDisposition::ServerOriginated`, which every
consumer (`client.rs`, `tokio_client.rs`, `project_read_stream.rs`) treats
like `Unsolicited`: surface it as a `ClientEvent` — the identity in it is
still true — and keep waiting for the real answer, which on this wire is
already the next frame.

**Regression coverage** —
`client::tests::a_stray_hello_under_the_pending_id_is_not_the_reply` (the
symptom, in one script: a hello at the pending id, then the real
`ListLoadedProjects` behind it — `project_list_loaded` must return the
projects and report the hello as an event),
`client::tests::a_hello_request_is_still_answered_by_a_hello` (the
exception),
`protocol_session::tests::a_hello_never_answers_a_request_that_did_not_ask_for_one`
and `::only_a_hello_request_expects_a_hello`. Both of the first two were
verified red against the pre-fix classifier. The e2e loop that failed
12/40 before runs 0/40 after.

**Lesson** — A correlation id is only unique inside the id space that
minted it, and a device's wire carries several of them: the device model's
own small counters, an effect that borrowed the port, the editor lens.
Giving each space a disjoint base (`starting_at`) makes collisions rare,
which is worse than it sounds — it converts a structural bug into a
scheduling-dependent one that surfaces on a different test each time and
reads as flake. The durable rule is the shape check: ask whether a frame
*could* answer the request before asking whether its number matches.
Frames the peer emits on its own initiative are the ones that can arrive
at any instant, so they are the ones that must never be eligible, and they
are already enumerated in the codebase — the side-channel event set. A
message that is a side-channel event is, by the same token, never a reply.
