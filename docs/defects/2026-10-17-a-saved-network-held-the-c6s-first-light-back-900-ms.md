---
status: fixed
found: 2026-10-17      # hardware-walk: the C6 cutover sitting (CX1, CX2), plan lp2025/2026-10-16-0848-tree-store-adoption
fixed: this change
area: fw-esp32c6 net/station_task.rs (the boot's first join) × esp-radio's Wi-Fi task
class: foreign-task-starvation
related:
  - 2026-10-17-the-emulated-join-costs-no-cpu   # why the emulator never showed it
  - docs/reports/2026-10-16-c6-time-to-first-light.md
  - docs/adr/2026-10-06-radio-frame-rate-budget.md
---
# A saved Wi-Fi network held the C6's first light back ~900 ms

**Symptom.** A board with a network saved and Wi-Fi on reached its first
lit frame ~900 ms later than one without: CX1 378 → 1,266 ms and CX2 403 →
1,291 ms on the cutover build, 613 → 1,550 ms on the littlefs release. Every
part of the `[perf] first light` line was the same except `lit`, which grew
from ~180 to ~1,060 ms, and the shader compile inside it said
`elapsed=955ms` instead of ~105.

On a local build of main (`90c65514d`), CX2 (XIAO C6, its power-button
project), ten power-on boots each: **no network 409 ms** (407–414); **network
saved 891 ms median, bimodal**: six boots at 1,318–1,323 ms, four at
454–459 ms. The fast four lit first and then served one frame of 860–870 ms;
the slow six lit after it. Steady state once joined was unaffected (38–39
fps, p99 ≤ 33 ms).

**Root cause.** The station task started the boot's first join as soon as
the network thread ran, while the engine was loading the project. A WPA2
join with a password derives the pairwise master key from it:
PBKDF2-HMAC-SHA-1, 4,096 rounds, done in software by the Wi-Fi blob's
supplicant (`libwpa_supplicant.a`'s `sha1-pbkdf2.c`/`sha1-internal.c`,
reached from `wpa_set_passphrase`) inside esp-radio's Wi-Fi task. That task
runs above every thread of ours, so for ~0.9 s the main thread did not run
at all. Whichever of the project load, the compile or the first frames was
under way took the whole stall; association reported `in ~960 ms`.

Proved on CX2 by saving the same network with its **64-hex raw key** instead
of its password (the network file accepts one; the supplicant then skips
the derivation): association in 125–185 ms, first light 478 ms (463–484),
and the long frame gone (max 245–262 ms, the compile's own frame).

**Fix.** On a board that will join (Wi-Fi on and a network saved, read
before the network thread starts), the station task waits for this boot's
first light to be decided (`first_light::decided`: a lit frame, or 64 unlit
ones) before anything else, capped at 2 s from reset so an engine-less core
still joins. Everything else about the join is unchanged.

CX2, the same build pair, ten power-on boots each: **network saved 415 ms**
(413–422), **no network 412 ms** (411–413). Association still takes ~960 ms
and starts at ~424 ms; the address came at a median 2.09 s from reset (DHCP
retries once a second); the one long frame (~0.9 s) now always falls just
after the light. Heap after 70 s joined: 136,028 B free, largest block
65,528 B (main: 136,144 / 65,536).

**Regression coverage.** None that runs in CI: the emulated network seam
answers the join without running the blob, so an emulated board shows the
new order (its first light no longer moves with a saved network; the
address comes ~65 ms later) but never showed the cost
(`2026-10-17-the-emulated-join-costs-no-cpu`). The silicon harness is
`lp-cli/tests/board_first_light.rs` (`wifi`, `forget`, `wifioff` cases,
`LP_FIRST_LIGHT_BOARD=cx2`); the emulated one is `emu_first_light.rs` with
`LP_FIRST_LIGHT_NET=1`.

**Lesson.** A vendor task above all of ours is a CPU budget we do not see:
its bursts land on whatever we happened to be doing, and the profile of our
own code shows nothing. Look for the blob's work (key derivation, SAE,
calibration) before tuning the path that got slow. What is left of this
one is the ~0.9 s frame after the light: deriving the key ourselves (once
per network, in slices, handing the blob the raw key) would remove it and
join ~0.8 s sooner, but a raw key cannot do WPA3-SAE, so it needs a
fallback to the password and a WPA2/WPA3 transition-mode access point to
prove it on.
