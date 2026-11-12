---
status: fixed
found: 2026-10-06      # how: hardware-walk (G1 desk numbers, fixture-c6, m6-split 128aea9ac)
fixed: this change (silicon re-check owed)
area: fw-esp32c6 `net::station_task` (the DHCP client's config) × smoltcp's `RetryConfig` × `station_policy::ADDRESS_TIMEOUT_MS`
class: fixed-budget-over-variable-work
related:
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR B, #989; plan MD9)
---
# The first Wi-Fi join after boot gets no address

**Symptom** — on both reboots of the desk walk, the C6's first association
(~900 ms) got no DHCP address: the heartbeat said `connecting … frames in
267 out 1`, then `failed`. The rejoin ~20 s later associated in ~40 ms and
had an address in ~0.95 s. Boot to address was ~22 s against a 2 s target.

**Root cause** — the station starts DHCP the moment it associates. The
first DISCOVER often leaves before the access point has finished the WPA
4-way handshake, and is dropped (`out 1`: the one frame the board sent).
smoltcp's DHCP client sends the next DISCOVER after its default
`discover_timeout`, 10 s, which is exactly the join policy's
`ADDRESS_TIMEOUT_MS`. So the policy gave up on the attempt just as the
retry was due, and backed off before rejoining. The plan's "DHCP starts on
link-up" fix removed the 10–12 s from starting DHCP early; this is the same
10 s, reached a different way.

**Fix** — `station_task::dhcp_config`: DISCOVER and the first REQUEST
retry every second. A lost first DISCOVER now costs about a second, well
inside the address timeout, and the first join gets its address.

**Regression coverage** — none on the host: neither the emulator's virtual
LAN nor the host harness drops a DISCOVER yet. The silicon re-check
(fixture-c6, boot to address, two reboots) is owed through `main`.

**Lesson** — two timeouts in different crates that happened to be equal
made one of them meaningless. A retry interval has to be well under the
deadline that waits for it, and the defaults of a library sit under ours
whether or not we read them.
