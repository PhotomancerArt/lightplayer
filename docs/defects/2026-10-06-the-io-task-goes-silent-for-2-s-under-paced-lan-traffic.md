---
status: open
found: 2026-10-06      # how: experiment (PR #993, measuring LAN resends under a host-pacing patch)
area: fw-esp32c6 recovery/watchdog × the USB link's io task × LAN traffic (cause not yet named); lp-emu-esp-common seam/net (the pacing that exposed it)
class: unexplained-stall
related:
  - docs/defects/2026-10-06-an-emulated-boards-clock-outran-its-lan-host.md
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR C, #993), data/pr993-lan-host-pace.patch and data/pr993-io-stall/
---
# The io task went silent for over 2 s while an emulated board served paced LAN traffic

**Symptom** — measuring lp-link resends over an emulated C6's LAN forward
(`lp-emu:esp32c6:t1+net=lan`, tree `f3feec073` plus a host-pacing patch for
the virtual LAN), the board's own console logged, with nothing loaded and only
`lp-cli link rtt lan:` and a USB console capture attached:

```text
[INFO] fw_esp32_common::server_loop: [perf] frame=11994 fps=825 elapsed=5000ms recv=0ms tick=0ms send=0ms total=0ms responses=0
[ERROR] fw_esp32c6::recovery::watchdog: [RECOVERY] io task silent > 2000 ms; withholding watchdog feed
[INFO] fw_esp32_common::server_loop: [perf] frame=13736 fps=348 elapsed=5000ms ...
[ERROR] fw_esp32c6::recovery::watchdog: [RECOVERY] io task silent > 2000 ms; withholding watchdog feed
[INFO] fw_esp32_common::serial::packed_link: packed link: replies are JSON again
[WARN] fw_esp32_common::usb_link::usb_link_transport: [usb_link] host link reset (); now session 1
```

One `link rtt` request in the same run took about 3 s. The frame rate halved
across the silence and the USB host link reset after it.

**When it shows** — in 3 runs of the round-trip experiment under pacing
variants (`pr993-arq-rtt-dbg`, `-dbg2`, `-final`). The variant that made up a
host sleep's overshoot (up to 10 ms of catch-up, so the board ran at 1.00× of
host time) showed it 3 of 3. It did not show in the unpaced run (`-orig`) or
in `-final2`, and the no-catch-up patch showed it in neither of its two upload
runs. So it appears under one traffic timing and not others. Nothing shows it
on silicon yet, and nothing proves it emulator-only: a latent stall under a
traffic timing a real host could also produce is a candidate for silicon.

**Not yet known** — whether the io task is starved (a lock held across the
LAN's `lp-net` work: the radio-link port's priority-1 lock is shared by
`lp-io` and `lp-net`), blocked (a USB IN endpoint that does not drain while
the board's clock is held to the host's), or whether the watchdog's 2 s is
being measured across a pacing wait the guest did not see.

**Repro** — the patch `data/pr993-lan-host-pace.patch` in the plan directory
(the no-catch-up version as kept; the catch-up variant drops the pace's
per-reading wait floor so overshoot is repaid, up to 10 ms). The experiment
script, its fixture and the three consoles that show the stall are in
`data/pr993-io-stall/` (`pr993-arq-experiment-rtt.sh`: join over USB, WebSocket
pings through the forward, `lp-cli link rtt lan: --count 60`, then a 6 s USB
console capture).

**Next** — re-run the experiment with `lp-io`'s stack and the port lock
instrumented (who holds the lock across the silent window), with and without
the pacing; and run the same traffic shape against a desk board when one is
next on the bench.
