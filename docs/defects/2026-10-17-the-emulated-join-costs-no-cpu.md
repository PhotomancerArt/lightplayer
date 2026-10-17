---
status: open
found: 2026-10-17      # hardware-walk: the saved-network first-light sitting (CX2)
area: lp-emu-esp32c6 network seam (net=lan) × fw-esp32-common SeamStation
class: fidelity
related:
  - 2026-10-17-a-saved-network-held-the-c6s-first-light-back-900-ms
  - docs/adr/2026-10-05-emulator-seams.md
---
# The emulated Wi-Fi join costs no CPU; on silicon it stops the main thread ~0.9 s

**Symptom.** With a network saved, a silicon C6 lost ~900 ms of first light
to the join (CX2: 409 → 1,320 ms), while the emulated C6 at t4 with the
network seam (`net=lan`, `emu_first_light.rs` with `LP_FIRST_LIGHT_NET=1`,
the same two images) moved 15 ms (374 → 389 ms) and joined by 450 ms.

**Root cause.** Under the network seam the station is `SeamStation`: the
join is answered by the virtual LAN, and esp-radio's `connect` never runs.
On silicon that call runs the blob's WPA2 key derivation (PBKDF2-HMAC-SHA-1,
4,096 rounds, in software) in the Wi-Fi task, above every thread of ours,
for ~0.9 s. The seam ADR's trust caveat already says it does not model the
driver's own timing; this is the largest instance measured so far, and it
is CPU, not airtime.

**To do.** Have the seam's station bill a join with a password the blob's
measured derivation time on a thread above the main one (as `led=fast`
bills the time it skips), so an emulated boot with a network saved shows
the stall where silicon does. Until then, an emulated first-light or frame
figure with a network saved is not evidence about the join's cost.
