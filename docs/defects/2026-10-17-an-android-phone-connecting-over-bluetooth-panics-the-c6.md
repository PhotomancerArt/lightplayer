---
status: fixed
found: 2026-10-17      # hardware-walk: the first phone walk (just walk-ble-phone), Pixel 8a Android 14 Chrome 122 → fixture-c6, lightplayer.app
fixed: 2026-10-17      # PR #1180 (claude/android-ble-panic): fe7f6ab84 (bt-hci), 281452893 (trouble-host), eea667e73 (Studio's words)
area: fw-esp32c6 ble (trouble-host RxRunner over LpHciTransport, esp-radio 0.18 fork) × Android's Bluetooth stack × lpa-studio-core device_effects (the failure is only a console warning)
class: untested-path   # every Bluetooth walk before this one was Mac Chrome or Bluefy; no Android central had ever connected
related:
  - 2026-09-25-a-knob-jump-over-bluetooth-kills-the-c6-ble-host.md   # the same receive path, a chained ACL packet cut short
  - ../adr/2026-09-24-ble-transport.md
  - ../../scripts/phone/README.md
  - ../../third_party/bt-hci/README-LP.md
  - ../../third_party/trouble-host/README-LP.md
---
# An Android phone connecting over Bluetooth panics the C6, every time

**Symptom** — Studio in a Pixel 8a's Chrome (Android 14, Chrome 122,
lightplayer.app) presses Bluetooth and picks the board in the phone's own
chooser. Nothing else happens: no card, no error on the page. The page's
console has one warning,

    device grant request failed: GATT Server is disconnected. Cannot retrieve services.

Each attempt panics the board and resets it. Reproduced on every attempt
(five of five) on `fixture-c6`, on two firmwares: an old proto-33 image
(commit `30ed7c05e`) and the current release, 2026.10.17-4
(`dd8534867`, proto 43, split image).

**What each end said** (one attempt, 2026-10-17 03:01, the board's USB
console read with `scripts/emu/tty-capture.py`, the phone's `adb logcat`):

- Phone: the LE link opens (`onClientConnectionState status=0`), Chrome asks
  for an ATT MTU of **517** (`configureMTU`), and Android starts primary
  service discovery (`GATTC_Discover … disc_type=1`). Nothing comes back:
  `gatt_rsp_timeout: retry discovery primary service` 5 s later, then the
  link drops with `HCI_ERR_CONNECTION_TOUT` / `GATT_CONN_TIMEOUT`, and
  Chrome reports status 8.
- Board, in between:

      ====================== PANIC ======================
      panicked at <redacted>:0:0:
      range end index 2 out of range for slice of length 0
      frames: 0x42064b44 0x4204dd8e 0x4206b32c 0x4206b2de 0x4206b296 0x420925d4 0x4206185e
              0x42038172 0x420554b4 0x4206ee6a 0x420334a8 0x420333e0 0x42069b12 0x420304ea 0x42030456
      [RECOVERY] crash committed to the RTC ledger
      [RECOVERY] resetting

**Where** — partly decoded. The release carries no ELF, and a local
`just fw-esp32c6-split` at the same commit gave a core 272 B shorter than the
release's (version stamp), so most frames land in unrelated functions. The
coherent middle of the chain is `fw_esp32c6::ble::ble_task` runner_task →
`trouble_host::host::RxRunner<ExternalController<LpHciTransport, 20>, …>`
(+0x860): the panic is in the host's receive path, the code the 2026-09-25
chained-ACL defect fixed one bug in. A slice `[..2]` on an empty buffer fits
an L2CAP or ATT header read on a zero-length payload. Mac Chrome and Bluefy
(MTU ~185) never sent whatever this is; Android's first moves are a
517-byte MTU request and, likely, a data length update to 251-byte packets.

**Studio's half** — the failed connection reaches the person as nothing at
all: the grant request's failure is a console warning, and the Connect a
board section looks as it did before the press.

**Root cause** — decoded exactly on 2026-10-17 against the ELF of a split
build flashed for the purpose (`target/fw-split/shipped/p2.elf`, `9c1ad0ac9`):

    core::slice::index::slice_index_fail
    ← <Range<usize> as SliceIndex<[u8]>>::index_mut
    ← bt_hci::controller::ControllerState<20>::complete
    ← trouble_host::host::RxRunner<ExternalController<LpHciTransport, 20>, …>
    ← fw_esp32c6::ble::ble_task::runner_task

and the board's console, read through its USB link (`lp-cli link capture`),
said just before it:

    [ble] link1: central asks interval 7500..7500 us latency 0 timeout 5000 ms
    trouble_host::connection: Connection parameters request procedure failed

An Android central asks for connection parameters over the **link layer**
(the Connection Parameters Request procedure), which the controller hands the
host as an HCI *LE Remote Connection Parameter Request* event. Mac Chrome and
Bluefy never did. Two upstream bugs met on that path:

1. **trouble-host 0.6** answered it with an *LE Connection Update* first and
   the *Reply* only after that succeeded. While the remote procedure waits on
   the host, the controller refuses the update, so every Android request
   ended in a *Negative Reply*.
2. **bt-hci 0.8.1** (0.9.0 too) declares both the Reply (`0x2020`) and the
   Negative Reply (`0x2021`) async, with no return, but the Core spec gives
   both a Command Complete carrying Status and Connection_Handle. Its
   `ExternalController` hands an async command an empty return buffer, and
   `ControllerState::complete` copied the 2 handle bytes into it:
   `event[..2]` on a slice of length 0.

The earlier guesses (a zero-length ACL payload, the 251-byte data length)
were wrong. Nothing on the ACL path was involved.

**Fix** — PR #1180.

- `third_party/bt-hci` (a 0.8.1 fork): a Command Complete carrying more
  return bytes than its command declares is copied up to what fits, and
  warned about (`[bt-hci] command 0x2020 completed with 2 return bytes, room
  for 0; extra ignored`). Its status still reaches the caller. No answer from
  the controller can slice past the buffer any more.
- `third_party/trouble-host` (a 0.6.0 fork): a link-layer request is answered
  with the Reply alone. Also, the request's maximum interval was read from
  `interval_min`.
- Studio: Connect a board now says, in red under its squares, when a USB or
  Bluetooth connect fails after the pick (`UiChooserFailure`), until the next
  press. A dropped Bluetooth connection reads "The board ended the Bluetooth
  connection before it answered. Press Bluetooth to try again, or connect it
  by USB."

**Regression coverage**

- Host: `lp-fw/fw-esp32-common/tests/ble_controller_answers.rs` drives both
  commands through `ExternalController` against the controller's exact answer
  bytes, plus a refused Reply and a Command Status answer. On upstream bt-hci
  three of its four tests panicked with the board's own message.
- Core: `studio_device_e2e_tests/chooser_failure_tests.rs` presses
  `devices/connect-ble` and `devices/connect-usb` by path.
- Silicon, Pixel 8a (Android 14, Chrome 155) at desk distance, this branch's
  firmware: no panic on fixture-c6, and the requests are granted (7.5 ms, then
  the board's own 15 ms, then Android's 48.75 ms once discovery is done), with
  the link open at ATT MTU 247. `just walk-ble-phone --serve-release`
  (this branch's Studio) passed all nine steps on CX2 `c6-button` (this
  branch with `fs-tree`, the store that board's files are in): add, identify,
  Connect on the card and a control turned, Done, the card's picture moving
  over Bluetooth, the editor (the control held), Play and a knob, and a power
  cut under Play ridden out (Reconnecting after 6.7 s, back 9.6 s after the
  cut, the knob turning again). fixture-c6 stops at `identify` for another
  reason: its access is `open: nobody` (only its owner's browsers, account and
  password unlock it), and the phone is signed out and holds no key, so it
  sees the board as locked, which is correct.
