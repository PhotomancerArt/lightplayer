# The Wi‑Fi walk on two emulated S3s

**Date** 2026-10-22 · **Plan** `lp2025/2026-10-20-2335-s3-radio-and-ota`
(M5, the second half) · **PR** #1273 · **Command** `just walk-wifi-emu lan
--s3` · **Script** `scripts/emu/walk-wifi-emu-lan.mjs` · **CI cell**
`lp-cli/tests/emu_s3_lan_pair.rs` (`just test-emu-esp32s3-wifi`) · **C6
twin** `docs/reports/2026-10-06-wifi-emulator-walk.md`

> **STATUS: THE LAN WORKS; THE WALK DOES NOT PASS WHOLE.**
> - **Passed:** joining, Studio's secure LAN links, DNS‑SD, a reset onto a
>   new lease, and, with a small project, Studio editing a board over the
>   LAN.
> - **Failed on the image's heap:** with Peach (1D), the walk's usual
>   project, the S3 `wifi` image runs out of heap and resets
>   (docs/defects/2026-10-22-the-s3-wifi-image-runs-out-of-heap-with-a-project-loaded.md).
> - **Failed on Studio's side:** two steps fail on the walk's own Wi‑Fi panel
>   selector, which C6 runs hit before this PR.
>
> Tree `b5737b828` (firmware `fw-esp32s3 --features wifi` built at the tree
> before it, `427f91ab4` + this PR's firmware commit; the Studio release
> bundle and lp-cli at `b5737b828`). The label is
> `lp-emu:esp32s3:t1+net=lan`. Desk load average 60–115 throughout.

## What ran

Two boards, `s3-a` and `s3-b` (`emu serve --chip esp32s3`, MACs
`02:4c:50:00:00:00` and `:01`, ROM-up from the merged `wifi` image), on one
served LAN (`lan=home`), Studio headless over `?lan=`. The served firmware
store was an empty directory: the walk flashes nothing.

### Run 1 — every step, Peach (1D)

| step | | what the boards said |
|---|---|---|
| W1 join over the USB door | ✓ | `s3-a` 192.168.4.100 (`lp-0000.local`), `s3-b` 192.168.4.101 (`lp-0001.local`) |
| W2 Studio over `?lan=` | ✓ | each: `[lan] link link1 … secure session opening (1024 B frames)` |
| W3 push Peach, edit | ✗ | `Project loaded: studio` with 43,532 B free, then `memory allocation of 68 bytes failed` and a reset: the defect |
| W4 `s3-b` beside `s3-a` | ✗ | knock-on: `s3-a` had reset in W3 and held no project |
| W5 DNS‑SD browse | ✓ | `lp-0000` and `lp-0001`, each with its own MAC |
| W6 wrong password | ✗ | the walk's wait for the Wi‑Fi panel timed out. The panel is open in the shot and shows the board connected, so this is the walk's selector |
| W7 not in range | ✗ | the same selector |
| W8 `lp-cli upload button-sign lan:` | ✗ | deploy acked, then `read refused: board memory busy (free 34924 B, largest block 26276 B; needs 40960 B free and a 16384 B block)`: the defect |
| W10 `link rtt lan:` | ✗ | up at the edit tier, idle 106.6 fps, then no reply to a request in 30 s |
| W9 renumber + reset | ✓ | 192.168.4.100 → .103, station connected, Studio's link back through the same forward with traffic |

### Run 2 — W1–W3 and W5, Fiber Headband (28 KB, the smallest gallery project)

`WALK_PROJECT="Fiber Headband" just walk-wifi-emu lan --s3 --skip
W4,W6,W7,W8,W9,W10`

| step | | what the boards said |
|---|---|---|
| W1 | ✓ | the same leases and names |
| W2 | ✓ | both secure sessions |
| W3 | ✓ | loaded with 73,984 B free; frames 3,477 → 5,754; the knob edited 1 → 8 |
| W5 | ✓ | both instances |

## What it shows

- **The emulated LAN works on the S3.** The `wifi` image engages the network
  seam (`[wifi] network: the emulator's network seam`), and the station, the
  IP stack, DHCP, mDNS/DNS‑SD, the secure lp-link over the WebSocket and the
  server all run on Xtensa as they do on the C6. Two boards share the LAN and
  find each other, and Studio edits one over it (run 2's W3).
- **The image's heap is the open problem**, not the network. Peach takes
  57 KB on load and leaves the board below both the LAN read floor and,
  apparently, the `Internal` region esp-radio needs. That is the defect,
  and it is lane R's (the S3 `wifi` image).
- **For the silicon join defect**
  (docs/defects/2026-10-22-the-s3-wifi-image-never-authenticates.md):
  the seam stands in for `EspStation`, and everything else joins and
  serves: the station task's join policy, the IP stack, the LAN endpoint
  and the `lp-net` thread on its S3 facts. So the silicon failure lies at
  or below `EspStation`: the station configuration it hands esp-radio, the
  blob, or the radio. The note is on that defect.

## What it does not show

The radio itself: no signal, airtime, coexistence with BLE and ESP-NOW, or
the driver's own heap and timing on silicon (the seams ADR's §11 caveat).
No number here is a silicon number. Emulated time is never a gate.
