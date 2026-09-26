---
status: fixed
found: 2026-09-25      # how: hardware-walk (M7 S1 laptop desk walk, Run M: desk XIAO C6 + Mac Chrome 153 over CDP, deployed Studio 5d40f95fb)
fixed: 2026-09-25      # PR #831 (claude/ble-knob-burst-fix): d5dfb532d (cause), 0d2a594f3 (recovery), 1119e9098 (desk check)
area: esp-radio 0.18 NPL ACL receive path (C6) × fw-esp32c6 ble (host-runner restart, advertising) × Studio Play panel writes over `ble:`
class: untested-path
related:
  - docs/adr/2026-09-24-ble-transport.md
  - third_party/esp-radio/README-LP.md
  - lp2025/2026-09-23-1428-ble-remote-control (spike-results.md, Run M; m7-data/runM/, m7-data/knob-fix/)
---
# A knob jumped to its end over Bluetooth kills the C6's BLE host, and it never advertises again

**Symptom** — in Studio's Play mode over Bluetooth (unlocked at edit), sending
the Scale knob to its maximum (a keyboard `End` on the knob, 1 → 4) dropped
the link within a second or two. The page said
`transport error: Transport error: bluetooth link lost: the board or the radio
ended the connection` and `the board under the editor went away; the editor is
closed`, and went back to `/devices`. The board's console (USB, non-resetting
reader) printed, right after the write and before any disconnect line:

    [WARN] esp_radio::ble::controller: [hci] error parsing packet:
    [ERROR] fw_esp32c6::ble::ble_task: [ble] host runner error — restarting it
    [INFO] trouble_host::host: [host] initialized
    [INFO] trouble_host::host: [host] Device Address A2:F2:62:87:B4:8D

and then nothing more from `[ble]`: no `disconnected` line for the link, and
no `advertising as …` line. The render loop and USB heartbeats carried on. A
Bluetooth chooser then found **no device for 40 s**. Only a reboot (over USB)
brought advertising back. Reproduced 2 of 2 in Run M with `End`; a one-step
change (1 → 1.04) and a brightness change went through.

Reproduced again on the desk board on 2026-09-25 (deployed Studio, Mac Chrome
over CDP, `Home` on the knob: 4 → 0.25), with the host's read path
instrumented.

**Root cause** — two faults, the second turning the first into a dead remote.

1. **esp-radio 0.18 drops the tail of a chained ACL packet.** The C6's NPL
   controller hands each received ACL packet to the host as an `os_mbuf`, and
   `ble_hs_rx_data` copied only the first mbuf's `om_len` bytes. The captured
   packet the host could not parse:

       020020c200be0004001209004d217b226964223a34323934393637333331…

   — ACL on handle 0, header length `0x00c2` = 194 bytes; L2CAP 190 bytes on
   the ATT channel; an ATT Write Request (`12`) to handle 9 carrying Studio's
   `M!{"id":4294967331,…"value":{"f32":0.25},"ttl_ms":nul` — 191 bytes in all,
   so 186 of the header's 194: the line's last 8 bytes (`l}}}}}}\n`) never
   arrived. A sweep of single writes of every length 150–244 B (fork build,
   logging each join) showed exactly the ACL packets of **193–198 bytes**
   arriving as two mbufs — ATT values of **182–187 bytes**. Studio writes the
   whole line in one write, and its knob write is 186 B when the value prints
   short (`4.0`, `0.25`); `1.04` is an f32 whose JSON is longer, which is why
   the one-step change survived. It was never "a burst": one write of the
   wrong length is enough, burst or not.
2. **A host-runner restart left every link dead but open.** trouble-host 0.6
   restarts a failed runner by running its bring-up again, which begins with
   an HCI `Reset`. The controller drops every connection on a reset without a
   `Disconnection Complete`, so the host kept the old connection: its task
   never ended and never freed its slot, the advertiser sat forever in an
   `accept` the reset had cancelled, and — seen on the desk with only the
   first half of the fix — the next connection got handle 0 again and its
   events went to the dead one. The "different address" in Run M's log was
   a misreading: `Device Address A2:F2:62:87:B4:8D` is the controller's
   public address, which the host prints at every bring-up including boot;
   the random static address it advertises is set again by the restart.

**Fix** — PR #831.

1. `third_party/esp-radio`: esp-radio 0.18.0 vendored and patched in; the ACL
   receive callback walks `om_next` and copies every segment. The parse
   warning now also prints the packet's length and header (the `{:?}` alone
   printed nothing in our `-Zfmt-debug=none` builds — why Run M's line was
   empty). Upstream candidate.
2. `fw-esp32c6::ble::hci_transport` + `fw-esp32-common::radio_link::hci_connection_ledger`:
   the host's transport records the connections the controller reports
   opened and closed; when the host writes a `Reset` it hands the host a
   `Disconnection Complete` (reason 0x16) for each one still open, before
   anything else the controller sends. `ble_task`'s advertiser drops what it
   was doing on a host restart and advertises again.

**Regression coverage**

- Host: `hci_connection_ledger` unit tests (a reset closes every link the
  controller opened; closed links, failed connections, other events and
  truncated ones are ignored; the synthesised event's bytes). The chained
  mbuf is not reachable host-side (it lives in the controller blob's
  callback), and neither the chunker, the mux nor the notify queue was
  involved.
- Desk: `spikes/ble-lab/scripts/m4-desk-check.py --only-knob-burst`: ten
  Studio-shaped knob jumps (1 → 4, 10-digit ids) with a preview poll in
  flight, padded through the 182–187 B window; one write of every length
  178–244 B; no `error parsing packet` / `host runner error` on the console;
  and `--force-restart` on a `desk_ble_fault` image (an ATT write carrying
  `LP-DESK-FORCE-BLE-HOST-RESTART` fails the host's read). On an image with
  the recovery but without the esp-radio fork the check fails at the first
  182-byte write, and the board recovers by itself.
- Silicon, desk XIAO `A0:F2:62:87:B4:8C`, final image `077941dab`: deployed
  Studio Play, Reset Scale then `End` (a 186-B `"f32":4.0` write) **10 of 10**
  with the link held (and 10 more earlier, 20 of 20, one connection
  throughout); the desk check above passed on the same image. The forced
  restart came back **5 of 5** on the `desk_ble_fault` build of the same
  code: the old link closed in the host (`controller reset: telling the host
  connection 0 is gone`), advertising resumed, the page was connected again
  6.2–7.5 s after the marker (most of it the central noticing the dead link)
  and was answered on the new link; heap free at the restart 109,248–109,300 B
  on the four drill runs that logged it.

**Lesson** — a recovery path that has never run is a guess; this one had never
run and did the wrong thing twice (no disconnect, then events routed to a dead
connection). And a parse error with an empty reason is a question nobody can
answer: the first instrumented reproduction named the cause in one line.

## Run M evidence, as first filed (M7 laptop walk, before the fix)

Kept from the entry as it was filed on the walk, as the record of what was
seen before the cause was known.

- **2 of 2 with `End`**, at 09:52 and 09:57 UTC (02:52 and 02:57 PDT) on 2026-09-25, each from a
  fresh boot and a fresh unlock.
- In the same session, on the same link, a one-step knob change (`ArrowUp`,
  1 → 1.04, read back from the board) and a brightness change (0.2 → 0.8)
  went through with no error. The fix above explains why: `1.04`'s f32 JSON
  is longer than `4.0`'s, so that write fell outside the 182–187 B window.
- Before the drop the board advertised on the random static address
  `9F:F2:64:5D:AB:00`; the restarted host printed `A2:F2:62:87:B4:8D`. The
  walk read this as the restart not reusing the original setup. It was a
  misreading (Root cause 2): that is the controller's public address,
  printed at every bring-up.
- The `[hci] error parsing packet:` line's reason was empty in that build,
  and `?ble=emu` could not have caught it: the emulated path goes through
  neither the controller nor `trouble-host`.
