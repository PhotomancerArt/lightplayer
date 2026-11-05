---
status: open
found: 2026-10-29      # how: hardware walk (FS3, the S3 Wi‑Fi default sitting, a 20 dBm control image)
area: lp-fw/fw-esp32-radio/src/net/esp_station.rs (`outcome_of`) × lp-fw/fw-esp32-common/src/net/station_policy.rs (the refused-password skip)
class: state-conflation   # "the AP never finished the handshake" and "the AP refused the key" are one outcome
related:
  - docs/defects/2026-10-22-the-s3-wifi-image-never-authenticates.md
  - docs/reports/2026-10-29-s3-wifi-on-by-default.md
---
# A handshake timeout on a weak link is taken for a wrong password

**Symptom** — FS3 (XIAO ESP32-S3 Plus) on a 20 dBm control image, where its
frames reach `lp-bench-ap` badly. One join attempt ended:

```text
[wifi] attempt ended: 4-way handshake timeout (the password was refused) bssid heard rssi -28
```

The network's `last` became `wrongPassword` and the station stopped trying it:
eight more Wi‑Fi off → on cycles over the same link made no attempt at all
(`tries=0`), though the password is right (the same board joins 10 of 10 with
it at 15 dBm). A reboot cleared it, since the refused set is held in RAM.

**Cause** — `outcome_of` maps `FourWayHandshakeTimeout` and `HandshakeTimeout`
to `ConnectOutcome::AuthFailed`, because a wrong WPA2 key usually ends that way
(the AP silently drops message 2's bad MIC). The station policy then skips a
network whose password was refused until the password changes. A link that
loses frames during the handshake ends with the same reason, so one bad radio
moment parks a correct network until the next boot.

**Why it matters** — a lamp at the edge of its access point's range, or a
board whose transmitter misbehaves as FS3's does at 20 dBm, goes offline for
good after one timed-out handshake and says its password is wrong. The C6
shares both functions.

**Expected fix** — tell the two apart before skipping: treat a handshake
timeout as wrong-password only when it repeats (say, twice in a row with the
AP heard at a usable signal), or keep retrying it on the backoff's slow
schedule while reporting "the password may be wrong". Reasons the AP sends
outright (`AuthenticationFailed`, `MicFailure`) can stay immediate.
