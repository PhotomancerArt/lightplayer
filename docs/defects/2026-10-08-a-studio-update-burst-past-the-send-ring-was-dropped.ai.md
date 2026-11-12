---
status: fixed
found: 2026-10-08     # e2e (walk-ota-emu --lan --steps engine-less)
fixed: PR B of lp2025/2026-10-06-2249-ota-wifi-updates
area: lpa-link `device_link/link_port_service.rs` (every browser port's channel-3 send)
class: config-masked-defect
related:
  - lp2025/2026-10-06-2249-ota-wifi-updates
---
# A Studio update burst past the link's send ring was dropped

**Symptom** — over Wi‑Fi, an engine-less board's restore stopped at 1 %
("Restoring firmware… 1 %") and never moved. The board's LAN link stayed up
(its frame counters crept on keepalives), and the board waited for chunks
that never came. Studio's record of the session showed three
`wi-fi update write failed: the link would not take the update message: Full`
errors right after the restore started.

**Root cause** — Studio's update host keeps several chunks ahead
(`ServeConfig::ahead`; 8 on the LAN). A restore from the engine cache sends
raw 4 KiB chunks. Eight of them (~33 KB) do not fit the link's send ring
(`LinkConfig::send_budget`, 24 KiB in every preset), so `send_update` answered
`Full` for the last ones. `LinkPortService::send_update` turned `Full` into an
error and dropped the message; the driver had already counted it as sent.
`lp-cli`'s host has the outbox Studio lacked (`capture_session.rs`: "the rest
wait"), which is why the same restore passed over `lan:` in PR A's scenarios.
USB and Bluetooth keep 4 ahead (~16.5 KB of raw chunks), which fits the ring;
an update's own pieces are compressed (~2.4 KB a chunk), which fits even at 8.
So the drop was there on every browser port, masked by the numbers.

**Fix** — `LinkPortService` holds channel-3 messages the ring refuses with
`Full` in an outbox, in order, and moves them into the link on every
transmit as acknowledgements free the ring; a session's outbox goes with the
session. Other refusals are still errors.

**Regression coverage** — `link_port_service::tests::a_burst_past_the_send_ring_waits_and_arrives_in_order`
(ten 4 KiB messages against the 24 KiB ring arrive once each, in order);
`just walk-ota-emu --lan --steps engine-less`.

**Lesson** — a send that can answer "not now" needs a caller that waits. An
error per message looks honest, but the sender above it (a driver that keeps
N ahead) had no way to resend, so "not now" meant "never". When a host keeps
work ahead of a bounded queue, the queue's refusal must be backpressure, not
an error — and a preset change that raises the work ahead (8 instead of 4)
is the test that finds out.
