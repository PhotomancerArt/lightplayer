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

## Follow-ups

- P06–P08, after the domain-model and device-UX pass: your boards from
  `ListBoards`, the automatic move onto the LAN, and the end-to-end walk
  with the Local Network prompt and `.local` (N12) recorded on a desk.
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
