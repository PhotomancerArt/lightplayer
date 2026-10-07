# Studio reaches Wi‑Fi boards with no flag: the emulated walk

Network-transport plan, PR A (#1020), phase P04. `just walk-wifi-emu
studio-lan` (`scripts/emu/walk-wifi-emu-studio-lan.mjs`), run 2026-10-07 on
the PR branch after merging `origin/main` (#993's virtual LAN), lane at
`d80d56ce2`. Configuration **`lp-emu:esp32c6:t1+net=lan`**, lp-emu
**`7c5192118`**. Emulated, not hardware-validated; nothing here is a timing
claim.

## Setup

Two emulated ESP32-C6 boards, `c6-a` (`02:4c:50:00:00:00`) and `c6-b`
(`02:4c:50:00:00:01`), running the packaged firmware ROM-up on one virtual
LAN (`lan=home`, the `lan` lane's fixture). Real Studio, headless Chrome, on
the release bundle, with **no `?lan=`**. Each board's USB link is held by
`lp-cli link capture` from S2 on; every step waits for the board's console.

## The stand-in (ruling DD193)

An emulated board's Wi‑Fi address is its virtual-LAN lease
(`192.168.4.100`), which the host cannot dial; `emu serve` reaches each
board through a loopback forward (`127.0.0.1:<port>`). So the lane first
proves Studio remembered the board's **own** lease (S1 reads it back from
`lp.devices.wifi-addresses.v1` under the board's MAC), then rewrites that one
entry's `ip` to the board's forward and presses "Connect over Wi‑Fi": it
dials the host forward in the lease's place. S3 types each board's forward
where a person would type its IP. On a desk the remembered address is
dialled as it is. No emulator change was made for this.

## What each step showed

| step | Studio | the board's words |
|---|---|---|
| S1 both boards join; Studio over the USB shim meets c6-a | c6-a Ready over USB; the book holds `{"ip":"192.168.4.100","host":"lp-0000.local",…}` under `024c50000000`, nothing for c6-b | `lp-cli wifi status` over each USB door: connected, c6-a `192.168.4.100`, c6-b `192.168.4.101` |
| S2 no cable (no `?emu=`): c6-a's remembered tile → "Connect over Wi‑Fi" | the same device comes back as a card, "Wi‑Fi · 127.0.0.1:<fwd>", Ready, c6-a's MAC, no remembered line left; a project pushed from that card | `[lan] link link1 from 192.168.4.1:49152: secure session opening (1024 B frames)`; `Project loaded: studio`; heartbeat frames 5493 → 6125 |
| S3 c6-b, never seen, by address; a second browser types c6-a's | c6-b's card Ready with its MAC; the second page says "Busy with another connection — try again" | c6-b: `secure session opening`; c6-a: `[lan] every LAN link is in use: a new one was told to try again later`; c6-a's first link not closed |
| S4 an address nothing answers at (`127.0.0.1:9`) | "Couldn't reach the board at 127.0.0.1:9. Is it on this network?" | (no board; the page's sentence is the claim) |

Result: `✓ the studio-lan walk finished S1–S4 (lp-emu:esp32c6:t1+net=lan, lp-emu 7c5192118).`
Screenshots and consoles: `target/walk-wifi-emu/studio-lan/` (not committed).

## The `lan` lane on the same tree

`just walk-wifi-emu lan --skip W10`: W1–W9 ✓. PR A changed two things in it:
W2's card line is now "Wi‑Fi · " (U+2011, the card's word), and W4 reads
lp-cli's busy words since #999 ("busy with another connection — try again
later"). It also gained `--skip`, because **W10 (`link rtt` with a 180 s idle
window) runs past a session's 10-minute command cap; W10 never ran to
completion locally for this PR.** W9's renumber moved c6-a from
`192.168.4.100` to `192.168.4.103` and Studio's link came back through the
same forward.

## `just walk-no-board`

6/6 (`--serve-release`): flash, connect, identify, upload, detach,
re-attach, with the LAN transport now installed on every page.

## Not covered

Chrome's Local Network prompt (a loopback page never sees it), real `.local`
resolution, and silicon timing — G1's. The emulated LAN has no uplink, so
`lp-cli lan list` was not run against these boards (the door's own
`/lans/<name>/browse` is the `lan` lane's W5).
