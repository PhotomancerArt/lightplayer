# Studio's network links: the LAN and the relay, one provider, on for everyone

- Status: accepted
- Date: 2026-10-08
- Plan: `lp2025/2026-10-06-0815-studio-network-transport` (PR A #1020, the
  LAN with no flag; PR B #1031, the `relay:` endpoint behind `?relay=1`;
  PR C, this change: the relay on for everyone). It is milestone M8 of the
  Wi‑Fi control roadmap (`lp2025/2026-10-01-1832-wifi-control`).
- Related:
  - `2026-10-07-c6-wifi-link.md` (the board's LAN link, `lan:<ip>`)
  - `2026-10-06-cloud-relay.md` (the relay: `/relay/board/<mac>`, the
    board's one network slot, the same-key takeover)
  - `2026-10-01-network-link-security.md` (the secure channel; §8's M8 row
    is amended below)
  - `2026-06-18-link-provider-id-convention.md` (one provider id, amended
    below)
  - `2026-10-01-agentic-control-offers-in-core.md` (every connect is a core
    offer)

## Context

A C6 on Wi‑Fi serves its secure lp-link on the house network
(`ws://<board>/link`), and, when it holds an account key, through
lightplayer.app's relay. M8 is the browser half: Studio reaches those
boards. The plan's shape was "relay first (every browser, iPhone
included), an automatic move onto the LAN in Chromium when browser and
board share a network, and basic, functional ways to find and connect".

Three PRs landed it:

- **PR A** made the LAN a product link with no flag. Studio remembers each
  board's Wi‑Fi address, and an unplugged board's tile offers "Connect over
  Wi‑Fi". The add slot also takes an address.
- **PR B** added the `relay:<mac>` endpoint behind `?relay=1`.
- **PR C** turns the relay on for everyone. Yona called it on 2026-10-08:
  "it seems to me we should turn on the relay now". The OTA work was
  waiting on it, because Studio updates boards through the relay.

The plan's G1 walk was to run on production behind `?relay=1` before PR C.
It did not run as written. What ran instead, on 2026-10-08, was Yona's
house walk: a C6 (LC6) on main's firmware, joined to a test access point,
reached through the relay on lightplayer.app with `?relay=<mac>`. **The
relay passed on silicon through production.** That walk also found the
silent full-access-file case (see 6). An update was also walked through
the real relay the same day (OTA M8). Chrome's Local Network prompt and
whether Chrome resolves `.local` (M6's N12) were **not** measured. Both
belong to the automatic LAN move, which this ADR holds back.

## Decision

1. **One provider, two endpoint kinds.** `browser-websocket` serves
   `lan:<ws url>` (a board's own socket) and `relay:<mac>` (the relay's
   browser leg on the page's own origin, `wss://<host>/relay/board/<mac>`,
   `ws://` on a plain-http dev origin). Everything above the socket is
   shared: one session per URL per page, one binary message per lp-link
   frame, the secure link (`LinkConfig::ws()`), and the same wire
   conversation. Both are "network" endpoints (`EndpointKey::is_network`),
   so firmware verbs stay USB-only on both. Over-the-air updates ride both
   (OTA M8, its own ADRs). The registry stores the transport label "Wi‑Fi"
   for both. The endpoint is not stored.

2. **The relay is on for everyone, with no flag and no off switch.** The
   relay half is installed in every browser with a WebSocket, iPhone
   included. `?relay=<mac>[,<mac>…]` stays only as a dev shortcut that dials
   the named boards at load, like `?lan=`. `?relay=1` (the old flag) asks
   for nothing. `?relay=0` is ignored, and the console says the relay is
   always on.

3. **The least way in: one offer, no list.** A remembered board (one that
   said its MAC over USB, Bluetooth or the LAN) with no live link gets
   `devices/<board>/connect-relay`, "Connect through lightplayer.app",
   beside `connect-wifi`. It is shown only while someone is signed in,
   because the account's key is what opens a board through the relay. It
   is built and pressed in core (`relay_connect_offer.rs`,
   `RelayConnectOp`), and core tests press it by path. Nothing asks
   lightplayer.app first whether the board is online. The press finds out,
   and the answer is said on the tile in plain words:
   - "The board isn't online." (the hub's 4404)
   - "Busy with another connection — try again" (4429)
   - "Sign in to Studio and plug this board in once to reach it through
     lightplayer.app." (no held key opens it)
   - "Couldn't reach lightplayer.app. Are you online?" (the socket never
     opened)
   - other relay codes in `lpc_relay`'s own words.

   A failed connect is not redialled. The next press asks again.

4. **Held keys only through the relay (ND7).** A relay link presents the
   account's key, then this browser's. It never presents the anonymous key:
   the board's "Anyone" setting never applies over the internet (M7 D1).
   It never presents a typed or remembered password either. The access
   layer never logs in over a relay link; it only reads the tier the held
   key was granted. When no held key opens the board, the session is given
   up with the words above. The key a board holds is filed by its MAC, so
   the LAN and the relay share it, and so does the memory of a wrong key.

5. **The card says which link.** Its words are "USB", "Bluetooth", "Wi‑Fi",
   or "Wi‑Fi via lightplayer.app" (`UiLinkKind`). Core's network line
   (`UiLanLink`) covers the relay and carries the link's kind, and the card
   reads its link kind from that line before anything else. So a relay
   card never says "No live picture over Bluetooth" or "USB connected"
   (`docs/defects/2026-10-08-a-relay-card-said-bluetooth-and-usb-connected.md`).
   Like a LAN card, it wears no Connections group. The login line names the
   link it is on. The update row's words are OTA's and unchanged.

6. **Say so when the account's key cannot go on.** A board reaches the
   relay only while it holds an account key. That key goes on when the
   board is plugged in over USB while someone is signed in. On a full
   access file, Studio already drops the oldest other browser's key to make
   room. That has been true since the two-passwords change, so a board full
   of browser keys does take the account key. When nothing may be dropped
   (a file full of passwords and account keys), the card now says so under
   its Connections group, with the reason, not only inside the Access
   panel: "Your account's key couldn't be added, so this board can't be
   reached through lightplayer.app. …". **How a full file makes room (the
   cap, what may be evicted) is not decided here.** That ruling is Yona's,
   in auto-queue ticket `2026-10-07-full-access-file-blocks-relay`.

7. **Remembered addresses are a browser convenience, not saved data.** A
   board's Wi‑Fi address lives in this browser's `localStorage`
   (`lp.devices.wifi-addresses.v1`, keyed by MAC, try/catch on every read
   and write). It is never stored in `registry.json` or the cloud, and no
   persisted format changes.

8. **Held for the domain-model and device-UX pass (E18): P06, P07, P08.**
   These are not built:
   - **"Your boards"**: a list of the account's online boards from
     `ListBoards` that this browser does not hold.
   - **The sweep** that would merge those boards into the roster.
   - **The automatic move onto the LAN**: `sameNetwork` with a LAN
     address, in Chromium, after the Local Network prompt, and back to
     the relay on loss.

   Yona's boards-and-projects model work may reshape what "your boards" is,
   and the move needs the prompt measured on a desk. So today:
   - a board must be met once in this browser before it can be reached
     through the relay;
   - a board reached through the relay stays on the relay even when it is
     on the same network;
   - Safari never tries the LAN on its own (it tries only an address a
     person typed or a remembered one they press).

## Consequences

- Any signed-in Studio, iPhone Safari included, reaches a board it has
  met, from anywhere, with one press. The OTA path follows with no flag of
  its own.
- With nobody signed in, the relay half exists but offers nothing. Its
  discovery answers what is present, never an error
  (`relay_transport.rs`, the plan's R5).
- A browser that never met the board has no way to it through the relay
  yet (the held list). `?relay=<mac>` is the dev way, and the hand-over
  names the product one.
- The emulated walk `just walk-wifi-emu studio-relay` proves the offer, the
  dial, the card's words and the hub's 4404 end to end. It runs the
  shipped C6 image (`lp-emu:esp32c6:t1+net=lan` through a local
  lp-cloud-server standing in for lightplayer.app). It does not prove the
  production edge, Safari, or the Local Network prompt.

## Alternatives considered

- **Keep `?relay=1` until P06–P08 land.** Rejected (Yona, 2026-10-08): the
  relay passed on silicon through production, and OTA needs it on.
- **Offer the connect signed out too, and let the press say "sign in".**
  Rejected: signed out, nothing in this browser opens a board through the
  relay, so the button could only fail.
- **Ask `ListBoards` before offering the connect, to hide offline boards.**
  Rejected for now: that is the held sweep (P06). The press answers
  "isn't online" in one round trip.
- **Evict account keys, or move the 16-entry cap, so the key always fits.**
  Not decided here (6): it changes the persisted access file's behaviour.

## Amendment (2026-10-20): your boards, from the account's list

"Your boards" (P06, held above) is built by the roadmap
`lp2025/2026-10-19-2300-your-boards-follow-you`: M1 had Studio read the
account's online boards (`ListBoards`) and join them onto the roster by MAC,
a board never met here included; M2 replaced `ListBoards` with
`ListAccountBoards` (cloud API v7), the account's stored board list, offline
boards included
([2026-10-20-the-account-board-list](2026-10-20-the-account-board-list.md)).
A listed board needs no meeting in this browser to be reached through the
relay. The automatic move onto the LAN (P07) remains the roadmap's M3.

## Amendment (2026-10-21): best link in one click, and the move onto the LAN

Plan `lp2025/2026-10-20-0845-best-link` (the roadmap's M3). Item 8's held
list is now built: "your boards" (amendment above), the roads Connect
takes, and the automatic move. Nothing persisted, no wire, cloud API or
firmware byte changed.

**The road order** (`lpa-studio-core/src/app/devices/best_link.rs`, pure).
For a board no link of this tab reaches, Connect tries, in order: Wi‑Fi,
then lightplayer.app, then its cable (a click). Wi‑Fi is tried when there is
an address, the page may reach the LAN, and the account's list does not say
the board is online on another network. The address is the one the list
gives while it says the board shares this computer's network
(`sameNetwork` with `lan`), else the one this browser remembers. The list's
address is used in memory only and never written to the address book. A
Wi‑Fi try that fails goes on to lightplayer.app in the same press, with one
log line; when both fail, the card shows lightplayer.app's words.
"Connect over Wi‑Fi" and "Connect through lightplayer.app" pressed in a
row's details are that road alone. A board this browser never met has no
Connect of its own: its `connect-wifi` is its Connect and carries the same
fall-through (`then_relay`, ruled by the director 2026-10-20).

**What the page can reach** is a platform fact the web reports once
(`LanReach`): an http page reaches the LAN directly in any browser; an
https page does in Chromium after its Local Network prompt
(`navigator.userAgentData` present), and never elsewhere (mixed content).
Only the automatic choices obey `Blocked`; a press of "Connect over Wi‑Fi"
still tries.

**The move** (`devices/lan_move.rs`, `studio_controller/lan_move_flow.rs`).
A board reached through lightplayer.app moves onto the LAN by itself, once
per page load, when it is idle (no update, push, flash, erase or removal;
no open, take-over or hold pending), its link holds a tier, and the list
says it shares this network at an address. In order:

- its LAN link presents the keys in the relay's order (the account's, then
  this browser's) for as long as the board is on Wi‑Fi: the board hands its
  one network slot only to the key that holds it, and a change of the
  link's best key would rekey the link, which the board closes (the walk
  found both);
- the relay session is parked (`park`/`unpark` on the WebSocket provider):
  when the board closes its route for the LAN connection, it is neither
  redialled nor said as a drop;
- once the board's own link is the LAN's (within 8 s), the editor, if it is
  on the board, is rebound onto it in one step, never held, and the relay
  session is let go. The relay link's departure no longer takes the board
  with it: `lpa-devices` drops the close or departure of a link a device
  is not on while it is on another (`StaleLinkEvent`;
  [the defect](../defects/2026-10-21-a-board-moved-onto-wifi-went-offline-when-its-relay-link-left.md));
- any failure stays on lightplayer.app with one log line and no second try
  this page load.

**The way back.** A moved board whose Wi‑Fi link goes is held by the editor
as any dropped link is. If Wi‑Fi is not back within 5 s (core's clock), the
LAN session is let go and the board is reached through lightplayer.app
again, once. Not after a close with code 1000 (another connection with the
same key took the slot: the card says someone else is connected, and the
LAN session stops redialling), and not while the list no longer names the
board online.

**Findings** from planning (notes F1–F4): F1, the cross-browser slot
ping-pong, stays open — its fix needs a firmware "taken" close reason and a
relay-protocol version (the defect
[2026-10-09](../defects/2026-10-09-two-clients-with-one-key-take-a-boards-network-slot-from-each-other.md));
the move is once per page load, never goes back after a 1000, and parks
the relay while it moves, so it does not make the loop worse. F2 (the key
order), F3 (the redial race) and F4 (never move during work) are the rules
above.

**A false `sameNetwork`** (two networks behind one public address) costs
one quiet failed try, and on an https page in Chromium a pointless Local
Network prompt. Accepted.

**What is walked, and what is not.** `just walk-wifi-emu studio-relay`
(T1–T5, the move refused) and `studio-relay-move` (T6 the move with the
editor open, T7 the way back and the 1000, T8 the best link) run the shipped
C6 image (`lp-emu:esp32c6:t1+net=lan`, lp-emu 9348e7191) through a local
lp-cloud-server, with a page-side shim standing in for the LAN's
reachability. A loopback http page never sees Chrome's Local Network
prompt, so nothing there says how Mac or Android Chrome behave. A third
lane, `studio-relay-account` (M4), walks the account's list: a board with
no Wi‑Fi reported over USB, both boards offline on a never-met profile, and
Remove from your account. For the desk
sitting (Q1), still blank:

- a WebSocket alone raises the prompt (Mac Chrome): ___
- a WebSocket alone raises the prompt (Android Chrome, Pixel 8a): ___
- `.local` resolves: ___
- a refusal looks like: ___

## Follow-ups

- P06–P08, after the domain-model and device-UX pass: your boards from
  `ListBoards`, the automatic move onto the LAN, and the end-to-end walk
  with the Local Network prompt and `.local` (N12) recorded on a desk.
  (2026-10-21: the first two and the emulated walks are done, by the
  amendments above and `lp2025/2026-10-19-2300-your-boards-follow-you`;
  the desk recording is that roadmap's desk sitting, still to run.)
- The full-access-file ruling (auto-queue
  `2026-10-07-full-access-file-blocks-relay`).
- Typed passwords and sharing over the relay (a guest reaching a board
  they do not own). The relay plan's notes record Yona's direction. M8
  ships ND7's held-keys-only.
- Hand the card a `UiLinkKind` for every device instead of deriving it
  (the lesson of the two `is_over_bluetooth` defects).

## Amendment to `2026-10-01-network-link-security.md` (§8, the M8 row)

The client key policy, as built:
- **On the LAN**, a link presents the keys this browser holds, then keys
  typed for this board (by its MAC), then the anonymous key, last.
  An anonymous link holds only what the board is open to. The card says
  what it holds ("Needs a device password", "Open — play, no password"),
  so a locked board is never silently treated as unlocked.
- **Through the relay**, a link presents held keys only: never the
  anonymous key and never a typed password. A walk that runs out is
  `Exhausted` and is given up in words. There is no anonymous fallback
  over the relay.

## Amendment to `2026-06-18-link-provider-id-convention.md`

`browser-websocket` is one provider id with two endpoint kinds, `lan:` and
`relay:`. The endpoint prefix, not the provider id, says which. The
convention's "one id per capability difference" holds: the two kinds share
discovery (none), permissions (the page's origin) and the wire. Their
differences (the key policy, the socket's origin) live in the endpoint
kind.
