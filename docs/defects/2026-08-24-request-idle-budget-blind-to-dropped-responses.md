---
status: fixed          # total per-request deadline in the correlation layer
fixed: this change
found: 2026-08-24      # how: hardware-walk (dig2go editor gate; hung connect + wedged page)
area: lpa-link device_client_io (request_idle) + client request correlation
class: timeout-scoped-to-sub-phase
related:
  - 2026-08-21-hello-gate-assumes-fresh-boot.md
  - ../debt/shared-uart-io-task-starvation.md
  - 2026-08-07-upload-wait-timeout-unbounded-deploy.md
---
# The request-idle budget cannot see a dropped response on a heartbeating wire

**Symptom** — Connecting the Studio to the bench dig2go while a project
played wedged the connect at "Connecting…" FOREVER — no timeout, no
verdict — and, because UI gestures queue behind the in-flight connect on
the actor command queue, the entire page went dead (no cancel, no other
device actions, dead terminal tab). Reproduced repeatedly on
auto-reconnect at page load, 2026-08-24.

**Root cause** — `DeviceClientIo::receive` bounds ONE `recv_frame` in the
`request_idle` budget ("quiet gap per app-protocol response frame") and
its comment asserts "every request gets a response frame, so a quiet gap
this long means the wire died mid-request." Two facts break the
assumption together:

1. The firmware drops response frames under engine load (`[io_task] UART
   TX timed out`, `responses=0` — the shared-UART starvation debt), so a
   request's response can simply never exist.
2. The server heartbeats every 5 s, and each heartbeat is a frame — so
   every `receive()` call returns well inside the budget, the caller's
   correlation loop re-calls for its response id, and a fresh budget
   starts. The composite wait is unbounded: the timeout is scoped to the
   frame gap sub-phase, while the phase that needed bounding is the whole
   request.

The hello-gate fix exposed this: before it, a connect to a running
server died at the gate before issuing any request; now it reaches
Ready, issues the first pull against a playing (response-dropping)
device, and waits forever, kept "alive" by heartbeats.

**Fix** (this change) — bound the REQUEST, not the gap:
`lpa_client::RequestDeadline`, an optional total per-request deadline on
`LpClient` racing the whole send + correlation loop against ONE timer
built at request start — unrelated frames never extend it. On expiry the
request id is abandoned (late frames classify `StaleAbandoned`) and the
caller sees the same "device did not respond" transport error the idle
backstop uses, so every caller settles to the same retryable failure.
The budget is `DeviceDeadlines::request_total` (20 s default);
`DeviceSession::request_deadline()` builds it from the session's
injected timers and `StudioServerClient::from_device_session` installs
it. `request_idle` stays as the frame-gap backstop for a dead wire (it
also bounds each receive inside streamed reads, which keep their own
quiet-gap `ProgressDeadline` and deliberately skip the total deadline).
Still open elsewhere: a user-visible cancel for a slow connect (the
actor-queue coupling, owned by the planned device-management rewrite),
and the firmware-side paydown — responses that survive engine load (see
the debt entry's exit criteria).

**Bench workaround** — idle the device before connecting
(`stopAllProjects` over the wire): with tick=0 no responses drop and
every pull completes.

**Regression coverage** — the fake device now scripts exactly this
failure mode (`FakeLightPlayerState::drop_responses` +
`heartbeat_interval`; the fake's host `LpServer` never heartbeats on its
own, so the cadence is synthesized), covered at three layers:
`lpa-client` `request_deadline_fires_through_endless_heartbeats` /
`late_response_after_deadline_classifies_stale_not_uncorrelated`
(mechanism + abandon contract), `lpa-link`
`dropped_responses_on_a_heartbeating_wire_hit_the_total_deadline`
(end-to-end against the fake, plus same-session recovery after heal),
and `lpa-studio-core`
`dropped_responses_during_connect_settle_to_a_bounded_failure` (the
real wedge: connect settles to a retryable failure and the actor keeps
serving commands).

**Lesson** — sibling of `2026-08-07-upload-wait-timeout-unbounded-deploy`
(same class): a timeout placed on the easily-measured sub-phase silently
fails to bound the phase the user experiences. "Liveness" evidence must
be scoped to the thing being waited FOR — an unrelated heartbeat is not
progress on this request.
