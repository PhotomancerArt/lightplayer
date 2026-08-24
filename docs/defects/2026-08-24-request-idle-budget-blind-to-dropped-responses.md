---
status: open
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

**Fix direction** — bound the REQUEST, not the gap: a per-request total
deadline in the correlation layer (arrival of unrelated frames must not
extend it), surfaced as the same "did not respond" error. Independently:
a hung connect must not wedge the page — the connect flow needs an
overall watchdog and a user-visible cancel (the actor-queue coupling is
its own UX defect). The true paydown is firmware-side: responses that
survive engine load (see the debt entry's exit criteria).

**Bench workaround** — idle the device before connecting
(`stopAllProjects` over the wire): with tick=0 no responses drop and
every pull completes.

**Regression coverage** — none yet: the fake device answers every
request. A fake failure mode "drop responses but keep heartbeating"
would have caught this exactly; it belongs next to the existing
premature-input machinery.

**Lesson** — sibling of `2026-08-07-upload-wait-timeout-unbounded-deploy`
(same class): a timeout placed on the easily-measured sub-phase silently
fails to bound the phase the user experiences. "Liveness" evidence must
be scoped to the thing being waited FOR — an unrelated heartbeat is not
progress on this request.
