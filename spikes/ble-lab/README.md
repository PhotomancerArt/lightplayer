# ble-lab — remote-controlled Web Bluetooth

`spikes/serial-lab`'s shape, for BLE: the human opens one page and clicks
**Join** once; after that the agent drives the link over HTTP. Built
2026-09-23 for the BLE spike in the `ble-remote-control` vision, against
`fw-esp32c6`'s `test_ble` harness (`just fwtest-ble-esp32c6`).

Web Bluetooth needs a user gesture only for the chooser. Reconnecting to the
same `BluetoothDevice` needs none, so the page reconnects by itself after a
drop and records every connect and disconnect with its uptime. That timeline
is the evidence for link stability.

The server also owns the board's USB console through
`scripts/emu/tty-capture.py`, which never asserts DTR/RTS (the reset sequence
on Espressif native USB). Every `/cmd` response carries the console lines
that arrived while it ran, so one call shows both sides.

```bash
python3 -u spikes/ble-lab/server.py          # prints its port (dev-port.sh ble-lab; BLE_LAB_PORT overrides)
P=36666                                      # whatever it printed
curl -s -X POST localhost:$P/serial -d '{"dev":"/dev/cu.usbmodem113301"}'   # start the console reader
curl -s -X DELETE localhost:$P/serial        # release the port before flashing
```

Human: open `http://localhost:$P/` in Brave (with
`brave://flags/#brave-web-bluetooth-api` enabled) or Chrome, click **Join**,
pick `LP-BLE-…`, and leave the tab in front.

```bash
cmd() { curl -s -X POST localhost:$P/cmd -d "$1"; echo; }
cmd '{"op":"status"}'
cmd '{"op":"idle","ms":30000,"timeoutMs":40000}'   # hold the link, report drops
cmd '{"op":"echo"}'                                # rtt
cmd '{"op":"burst","n":200,"timeoutMs":30000}'     # board -> page throughput
cmd '{"op":"writes","n":200,"timeoutMs":30000}'    # page -> board (upload direction), board-counted
cmd '{"op":"send","text":"hello"}'
cmd '{"op":"disconnect","reconnect":true}'         # drop and let the page reconnect
cmd '{"op":"auto","on":false}'                     # stop auto-reconnect
cmd '{"op":"eval","js":"return lab.S.events"}'     # escape hatch
curl -s localhost:$P/serial?n=40                   # the board's console tail
curl -s localhost:$P/log?n=40                      # page telemetry + server lifecycle
```

## Wire mode: the product image (BLE M4)

The product firmware carries the wire itself over the same NUS service, and
advertises as `LP-<project>` (or `LP-<last 4 MAC hex>`), so **Join** now
accepts any `LP-…` name or any board advertising NUS. It starts BLE only when
its device store says so, so provision the board over USB first:

```bash
curl -s -X DELETE localhost:$P/serial                       # release the console
python3 spikes/ble-lab/scripts/provision-access.py --dev <port by MAC> --password desk-lab
curl -s -X POST localhost:$P/serial -d '{"dev":"<port>"}'   # console back (it rebooted)
# … later, to put it back the way it was:
python3 spikes/ble-lab/scripts/provision-access.py --dev <port> --disable
```

Then, after Join:

```bash
cmd '{"op":"wire","on":true}'                                # packets → the line joiner
cmd '{"op":"req","msg":"hello"}'                             # any wire request; reply frames back
cmd '{"op":"req","msg":"listLoadedProjects"}'                # → notPermitted before login (locked board)
cmd '{"op":"login","password":"desk-lab"}'                   # PBKDF2 + HMAC in WebCrypto → loginResult
cmd '{"op":"unsolicited","n":10}'                            # hellos/heartbeats the board sent on its own
```

`scripts/lab.py` has the same as functions (`wire`, `req`, `login`,
`unsolicited`, `knob_rtt`). WebCrypto needs a secure context: `localhost`, or
the Tailscale HTTPS origin a phone uses. The password here is for the lab
only.

For ESP-NOW loss beside the product server, build the desk meter:
`--features esp32c6,server,desk_espnow_meter` (the product image with M2's
`[COEX]` counter in place of the ESP-NOW driver; never shipped). Run it on
both boards and read the `[COEX]` lines from the console as in M2.

## A phone (Bluefy on iOS): HTTPS over Tailscale

Web Bluetooth needs a secure context, and `localhost` doesn't reach a phone.
Expose the lab on the tailnet, on a port other than 443: the perf lab
(`scripts/emu/lab/`) already owns 443 on the desk.

```bash
tailscale serve --bg --https=8443 http://127.0.0.1:$P
# phone: https://<desk>.<tailnet>.ts.net:8443/  → Join → LP-BLE-…
tailscale serve --https=8443 off              # when done: --bg is a setting, not a process
```

## Coexistence: BLE beside Wi-Fi/ESP-NOW (`test_ble_coex`)

`test_ble_coex` is `test_ble` plus Wi-Fi/ESP-NOW brought up the way the
product's radio driver does it (`esp_radio::wifi::new`, channel 11,
broadcast), with `esp-radio/coex` on. Each board broadcasts a
sequence-numbered frame at a fixed rate and prints a `[COEX]` line every 2 s,
with both directions' counters (the peer reports its view inside its frames).
Loss over a window is Δ`rx_last_seq` − Δ`rx` (peer → this board) and
Δ`peer_last_seq` − Δ`peer_rx` (this board → peer). A sequence up to 1,000
below the high-water mark counts as `rx_dup`, not as a reboot. The images used
in M2's sitting (`08c6167b3`) predate that fix: there, a duplicate zeroed `rx`,
so board → peer is computed per 2 s interval, skipping any interval where a
counter fell. `rssi_avg`/`peer_rssi` from those images are unsigned bytes
(237 = −19 dBm). Build-time knobs:
`LP_COEX_BLE=0` (no BLE: the control, and the peer board),
`LP_COEX_RF_SWITCH=0` (leave the XIAO's RF switch undriven), `LP_COEX_HZ`
(default 50).

```bash
cd lp-fw/fw-esp32c6
LP_COEX_BLE=0 cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 \
    --no-default-features --features esp32c6,test_ble_coex
espflash flash --chip esp32c6 --partition-table partitions.csv --flash-size 4mb \
    --after hard-reset --port <port by MAC> ../../target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6
```

`spikes/ble-lab/scripts/nus-probe.py` does the same checks with `bleak` from a
terminal. It can't run from an agent shell on macOS: TCC aborts a process
whose responsible app doesn't declare `NSBluetoothAlwaysUsageDescription`
(exit 134, no prompt). Run it from Terminal.app if you want the host-stack
numbers.
