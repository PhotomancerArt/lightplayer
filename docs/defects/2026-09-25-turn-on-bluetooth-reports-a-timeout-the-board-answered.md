---
status: open
found: 2026-09-25      # how: hardware-walk (M7 S1 laptop desk walk, Run M: desk XIAO C6 over USB, Studio at main 23b7db5b3 served locally in Brave, the bench serial grant)
area: lpa-studio-core access_controller::write_store × devices/shared_link_client_io (one ConversationInbox per link) × device_frame_feed
class: state-conflation
related:
  - docs/adr/2026-09-23-ble-access-model.md
  - lp2025/2026-09-23-1428-ble-remote-control (spike-results.md, Run M; m7-data/runM/)
---
# "Turn on Bluetooth" reports a timeout for a write the board answered

**Symptom** — on a USB-connected XIAO C6 whose card shows its live picture,
the Bluetooth panel's **Turn on Bluetooth** (device password typed, Edit)
fails every time with

    the piece did not take it: transport error: Transport error: device did not respond within 5.0s

4 of 4 tries, including one on a fresh boot with only this board's card on
the page. The write had in fact landed: after a hardware Reset the board
printed `[ble] enabled by the device store — starting` and later accepted the
password over Bluetooth. The panel went on saying "Not set from this browser.
Bluetooth is off unless someone turned it on elsewhere." Studio did not reset
the board (DD23), so nothing tells the user a Reset is needed.

A wire tap in the page (the Web Serial writer and reader, 2026-09-25 09:47:52 UTC)
shows the request and a prompt answer:

    out  M!{"id":1073741824,"msg":{"filesystem":{"write":{"path":"/.lp/access.json",…
    in   M!{"id":1073741824,"msg":{"filesystem":{"write":{"path":"/.lp/access.json","error":null}}}}   (+192 ms)

In between and after, the card's frame feed was sending `projectRead` pulls
(ids 1073742328, 1073742329, …) about three times a second.

**Root cause** (from reading the code, not a debugger) — a shared-link
conversation gets its replies from the link's **one** `ConversationInbox`.
The access write (`write_store` → `io.into_client()`) and the card's frame
feed are both conversations on that link and both drain the same queue. The
feed polls it all the time, so it usually pops the write's reply first, and
its protocol session drops an id it did not ask for ("a straggler … is a quiet
discard"). The write's client then waits out `RESPONSE_BUDGET` (5 s). Both
clients also start their ids at `APP_CONVERSATION_ID_BASE`, so the ids are
not even unique to one conversation. The lab script
(`spikes/ble-lab/scripts/provision-access.py`) writes the same file on the
same port and gets its reply at once, because it is the only reader.

**Fix** — none yet. The inbox has to route by conversation, not per link, or
the feed has to stand down while another conversation runs.

**Regression coverage** — none: the access tests run against `FakeBoard`
with no frame feed beside them, and the emulated walk (`walk-ble-emu`) never
turns Bluetooth on from the panel.

**Lesson** — a mailbox shared by two readers is a race, and the reader that
polls more often wins. The shared-link io's own doc says a stray id is
dropped quietly, and that is safe only while one conversation owns the inbox.
