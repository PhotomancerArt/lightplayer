---
status: fixed
found: 2026-10-06      # how: emulated walk (`just walk-wifi-emu lan`, step W9), PR #993
fixed: this change
area: lp-emu/esp/lp-emu-esp-common seam/net (lan_port_forward.rs)
class: stand-in-divergence
related:
  - docs/reports/2026-10-06-wifi-emulator-walk.md
---
# The virtual LAN's port forward kept a moved board's old connection, and starved every new one

**Symptom** — in the emulated Wi-Fi walk's W9 (the gateway hands the board
a different address on its next lease, then the board is reset), c6-a
rejoined at `192.168.4.103` and announced `lp-0000.local` there, but
afterwards nothing reached it through its port forward: no `[lan]` line on
its console, Studio never reconnected, and `lp-cli link rtt lan:<forward>`
never finished (900 s). A one-board repro with an idle held socket did not
show it.

**Root cause** — the emulator's, not the firmware's. The gateway's TCP
stack is smoltcp 0.13.1, which rate-limits ARP to **one request a second for
the whole stack** (one `silent_until` in its neighbour cache), not per
address, and a failed send does not move a socket's retransmit timer. Studio's
established connection through the forward still had unacknowledged data
when the board reset. The forward kept that connection open to the old
address (`.100`): its sockets had no timeout, and nothing tied a connection
to the lease it was opened under. Once the gateway's neighbour entry for
`.100` expired (60 s), that dead socket asked ARP for `.100` every second and
held the stack-wide rate limit for good, so every new connection's ARP for
`.103` was refused. Studio's own socket never closed, so it never redialled.
A real router's NAT has the same shape of problem, but no real Wi-Fi board
is reached through one stack-wide rate limit; the stand-in diverged.

**Fix** — `lan_port_forward.rs`: each forwarded connection records the
address it was opened to; when the board's bound lease no longer equals it,
the forward closes the host side and drops the socket before it can send
(no RST, no ARP), counted in `ForwardCounters::moved`. Every forward socket
also gets `CONNECTION_TIMEOUT` (60 s, applied by smoltcp only while data is
outstanding), which bounds the other way into the same starvation: a board
that leaves the network while keeping its lease, whose dead connection
otherwise held off **another** board's forward.

**Regression tests** — `a_board_that_moved_address_is_reached_again_and_its_old_connections_close`
and `a_vanished_boards_connection_times_out_and_frees_another_boards_forward`
(lp-emu-esp-common, runner-driven, guest time). Real image: the held socket
closed 0.7 s after the reset and `wifi status` answered at the new address
through the same forward; the walk's W9 board-side gates pass.

**Lesson** — a medium's host edge holds state keyed to a guest fact (an
address) that the guest can change; tie the state to the fact, and bound
every wait on a peer that may be gone.
