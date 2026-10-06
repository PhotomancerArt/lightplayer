---
status: fixed
found: 2026-10-06      # how: e2e (PR C's emulated Wi-Fi walk, W6 on c6-b and W9 on c6-a)
fixed: this change
area: lpa-studio-core `device_effects::port_is_gone` × lpa-link `BrowserWebsocketLink` (a LAN link's loss)
class: lifecycle-ownership
related:
  - docs/defects/2026-10-06-a-bluetooth-reconnect-reads-the-old-links-loss.md
  - lp2025/2026-10-05-1903-wifi-link-c6 (PR B, #989)
---
# A redialled LAN link leaves the card "Attached — not listening"

**Symptom** — after a board's LAN socket closed and Studio's page
redialled it, the card never came back to "Ready". It read
`Attached — not listening · quiet` while the board held a live session
from that same page.

**Root cause** — a dropped socket reaches the link as
`wi-fi link lost: …`, and `BrowserWebsocketLink` turns it into `Error`
then `Closed`. The effects pump treats an error as a departure only when
`port_is_gone` recognises it. It knew Web Serial's notices and Bluetooth's
`bluetooth link lost`, but not the LAN's. So the link was closed but kept
attached. The page's session redials at once and is present at every
sweep, so the departure sweep never saw it leave. The redialled session
found its endpoint still attached, and nothing opened it again. (The
model in `lpa-devices` was right: a closed, attached link is
"not listening".)

**Fix** — `port_is_gone` recognises `wi-fi link lost`. The pump then
detaches the link, and the next presence edge attaches the redialled
session, which opens, says hello and is "Ready".

**Regression coverage** —
`studio_device_e2e_tests::lan_drop_tests::a_lan_link_that_drops_and_redials_comes_back_ready`:
a LAN board's link drops the way `BrowserWebsocketLink` does while its
session stays present, and the card comes back ready on the new link.
Without the fix it times out on exactly the walk's card
(`Attached — not listening`, `quiet`).

**Lesson** — a transport's death notice is matched by text in one place,
and every new transport has to add its own there. Bluetooth had to, and so
did the LAN. A typed "link lost" event from the link would have made the
LAN's loss a compile-time question, not a string to remember.
