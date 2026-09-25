---
status: fixed
found: 2026-09-25      # how: hardware-walk (M7 S1 laptop desk walk, Run M: desk XIAO C6 over USB, Studio at main 23b7db5b3 served locally in Brave, the bench serial grant)
fixed: 2026-09-25      # PR #824 (easy-access Studio): 0f259f403 (per-conversation id slices), c189b635b (regression tests)
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

**Fix** — PR #824 (the easy-access Studio PR). Every conversation on a
shared link now claims its own slice of the app id range
(`CONVERSATION_ID_STRIDE` ids) when its io is made; its client mints ids only
there, and `receive` takes only the replies in its slice
(`lpa-studio-core/src/app/devices/shared_link_client_io.rs`, "Several
conversations on one link"). Another conversation's reply stays in the inbox
for its owner instead of being read and discarded as a stranger's. The same
PR replaced the panel's whole-file write with the board-merged
`AccessSetSwitches` / `AccessAdd` requests
(`docs/adr/2026-09-24-easy-bluetooth-access.md`), and Studio now restarts the
device itself after a Bluetooth toggle over USB, so the "needs a Reset nobody
mentions" half is gone too.

**Regression coverage** — `shared_link_client_io::tests::a_write_reply_is_not_consumed_by_a_concurrent_frame_feed`
(two conversations on one link, the feed polling; the write gets its reply)
and `shared_link_client_io::tests::dropping_a_conversation_leaves_another_conversations_reply`.
Not re-walked on silicon as its own step; the G4 walk (2026-09-25) turned
access on over USB on the easy-access build without seeing it.

**Lesson** — a mailbox shared by two readers is a race, and the reader that
polls more often wins. The shared-link io's own doc says a stray id is
dropped quietly, and that is safe only while one conversation owns the inbox.
