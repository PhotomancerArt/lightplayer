---
status: fixed
found: 2026-10-06      # how: e2e (PR C's emulated Wi-Fi walk; intermittent)
fixed: this change
area: lpa-studio-core `NetworkController` (`NetworkCommand::Scan`)
class: newest-only-inflight-memory
related:
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR B, #989; PR C's walk)
---
# A scan asked while the board's status read is out is dropped

**Symptom** — the Wi-Fi popover's Nearby list sometimes stayed empty on a
board whose radio had heard networks. The connect page asks for one scan
when it opens, and nothing asked again.

**Root cause** — `NetworkCommand::Scan` returned early when the
controller's own status read (or a change) was in flight, or when it had
no status yet. It recorded nothing, so when the read's answer landed there
was no memory that a scan had been asked. The connect page opens just as
a board's first read goes out, so the two often met.

**Fix** — the scan is owed (`scan_owed`) when it cannot go now, and the
read's (or change's) answer sends it. A refusal for want of author drops
it, since the scan would be refused too.

**Regression coverage** —
`studio_device_e2e_tests::wifi_device_tests::a_scan_asked_while_the_status_read_is_out_goes_after_it`,
against a fake board with a scripted Wi-Fi station
(`FakeDeviceScript::with_wifi_station`). It fails without the fix.

**Lesson** — an early `return` on "busy" in a request handler is a dropped
request unless something records it. The class is the same as remembering
only the newest in-flight operation: what was asked while something else
was out has to survive until that answer.
