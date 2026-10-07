---
status: fixed
found: 2026-10-06      # how: e2e (PR C's emulated Wi-Fi walk, a Studio page reloaded after another LAN client let go)
fixed: this change
area: fw-esp32-common `LinkMuxTransport::expire_unauthenticated` × the C6's one LAN slot × an open board's tier
class: assumed-context
related:
  - docs/defects/2026-10-06-a-lan-link-strands-the-heap-below-the-load-floor.md (one LAN slot)
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR B, #989)
---
# A LAN socket that never handshakes holds the C6's one LAN slot

**Symptom** — a Studio page reloaded right after another LAN client let
go sat at "Connect a board". The board logged `secure session opening`,
then `frames in 0 out 34 · handshakes 0`, the out count climbing to 534:
a link the board kept offering SYNs on, from a peer that never answered.
Seen 2 of 2 without the emulator's pacing, 0 of 2 with it.

**Root cause** — the mux closes a radio link that has not logged in
within `LOGIN_DEADLINE_MS`, but skips any link that already holds a tier.
On an open board (new boards open at Author) every link holds a tier from
its first frame, before its lp-link session is up, so the deadline never
applied. A socket that opened and went quiet (likely the torn-down page's
own reconnect, which TCP kept alive) then held the slot, and with one LAN
slot every new client was told "try again later".

**Fix** — a link whose lp-link session has never come `Up` is closed at
the deadline whatever tier it would hold ("its session never came up").
Bluetooth's slots get the same rule. lp-cli also retries a busy board for
about 2 s, so back-to-back commands do not fail on the slot's release.

**Regression coverage** — `link_mux_transport::tests::a_link_whose_session_never_comes_up_is_closed_even_on_an_open_board`;
`lp-cli/tests/lan_link.rs::a_link_past_the_boards_slots_is_told_to_try_again_later`
(the retry count). Not reproduced end to end: the stuck page was seen only
without pacing, and this fix is inferred from the board's counters.

**Lesson** — "holds a tier" was standing in for "is a live client". On an
open board the two are different, and a deadline meant to clear dead links
has to ask about the link, not the tier.
