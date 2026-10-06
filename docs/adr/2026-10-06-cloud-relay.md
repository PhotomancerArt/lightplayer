# ADR: The cloud relay — boards on Wi-Fi reachable through lightplayer.app

- **Status:** Proposed (accepted at PR A's ship call)
- **Date:** 2026-10-06
- **Deciders:** Photomancer (Yona; the Wi-Fi roadmap director)
- **Supersedes:** None
- **Superseded by:** None
- **Plan:** lp2025/2026-10-06-0815-wifi-relay (Wi-Fi roadmap M7)

## Context

A C6 on the house Wi-Fi is reachable on its LAN (M6: a secure lp-link inside
a WebSocket at `ws://<board>/link`). From anywhere else it is not: home
routers do not take incoming connections, and a phone away from home is not
on the LAN. The Wi-Fi roadmap's answer (its D2, D9) is a **relay** on
lightplayer.app that the board dials out to and that browsers reach, with the
network-link security ADR (`2026-10-01-network-link-security.md`) already
deciding that every session through it is Noise NNpsk0 end to end and the
relay is untrusted.

What constrains the shape:

- **The C6 cannot afford TLS.** Measured 2026-10-01: +79 KB flash and ~29 KB
  heap per connection, on a heap whose read gate a held LAN session already
  leaves only ~1.7 KB of margin.
- **fly.io runs one machine** (SQLite on its volume) and deploys
  `immediate`: every connection drops on every deploy.
- **Fielded boards outlive cloud deploys.** The device wire's no-compat rule
  (Studio, lp-cli and firmware ship together) cannot hold for a protocol a
  lamp speaks to a service that redeploys daily.
- **"Anyone" means anyone nearby.** A fresh board is open at Author
  (`FRESH_OPEN`) to whoever reaches it over Bluetooth or the LAN. A relay
  that let any browser through would make that "anyone on the internet".

## Decision

### 1. Two legs, one pipe

| Leg | URL | Sockets | Carries |
|---|---|---|---|
| Device | `ws://lightplayer.app/relay/device` | one per board | `lpc-relay` frames: hello, challenge, proof, registered/refused, then `Open`/`Frame`/`Close` with a route id, `LanChanged` |
| Browser | `wss://lightplayer.app/relay/board/<id>` | one per session | bare lp-link frames, one per binary message |

The hub joins a browser socket to a route on its board and passes frames
byte-identical, in both directions, reading none of them. Because the
browser leg carries bare lp-link frames, Studio's `browser-websocket` link
port and lp-cli's `lan:` socket code run above it unchanged. Keepalive is the
WebSocket's own ping, every 25 s on each leg; a leg silent for 60 s is closed
by either end. Frames are at most 2 KiB (an lp-link frame on the LAN preset is
≤ 1,088 B, plus a 3-byte route header). The hub holds at most four routes per
board; a board that holds fewer (the C6 holds one) answers the rest `Busy`.

**The device leg is plain HTTP, deliberately.** It is the one path on
lightplayer.app that is. fly's `force_https` is all-or-nothing, so it is off,
and `lp-cloud-server`'s `https_redirect` middleware answers every other
plain-HTTP request (fly's `X-Forwarded-Proto: http`) with the same `301` to
https fly sent before (checked against the live service: same status, same
`Location`, no HSTS). A route-walk test over every route in the router, and a
post-deploy smoke in `deploy-cloud.yml`, hold it. Nothing is lost in the
clear that matters: the board authenticates in-band, and the sessions inside
are sealed. TLS can be added later on chips that can afford it, as a wrapper
on the board's `ByteStream` (D3), without changing what runs over it.

### 2. `lpc-relay`: the device leg's own protocol, version-and-refuse

A new `no_std` + `alloc`, sans-IO crate, `lp-core/lpc-relay`, shared by the
hub, the firmware, lp-cli's host board and the tests: the framing (a one-byte
tag and fixed little-endian fields, length-checked, never serde; goldens in
`tests/relay_frame_golden.rs`), the hello, the proof, the close codes, and the
board's relay client state machine (when to dial, backoff, the route table).

It carries **`RELAY_PROTO_VERSION`** (1), the first field after the hello's
tag so the hub reads it before anything else, and the hub accepts exactly the
versions it lists in `SUPPORTED_RELAY_PROTO_VERSIONS`; any other is a named
refusal (`VersionTooOld` / `VersionTooNew`) the board reports. This is the
cloud API's policy, not the wire's: a board in the field cannot be upgraded
in lockstep, so the hub keeps an old version listed while boards speak it.
A board refused for its version asks again after an hour.

### 3. The board proves which accounts it holds keys for

A board holds copies of its holders' keys (`/.lp/access.json`); Studio puts
the signed-in account's key on it over USB. The cloud minted that key and
keeps it. So registration is a challenge:

```text
relay_auth_key(K)          = HMAC-SHA256(K, "lp-relay auth/1")
proof(account, nonce, mac) = HMAC-SHA256(relay_auth_key(K), nonce ‖ mac)
```

`K` is the entry's key exactly as the board stores it
(`PBKDF2(key_secret, key_salt, 1)`, pinned against Studio's installer by a
test). The hello names each account entry by salt (≤ 8); the hub sends 32
fresh bytes from the OS CSPRNG; the board answers one proof per salt; the hub
looks each salt up (`MetaStore::account_by_key_salt`, the **current** salt
only, so a key the account has reset proves nothing — the board says so and
Studio refreshes it over USB) and checks it in constant time. No secret
crosses the wire. The label separates the proof key from the link PSK
(`"lp-link psk/1"`) and from the HMAC login, so none can stand in for
another; the MAC is bound in, so a proof cannot be replayed for another board.

**What it proves:** "I hold account A's key." **Not** "I am board X": a MAC is
public, so any board holding A's key could claim X's. That lets one account's
own boards confuse each other at the hub and nothing more — every session is
still sealed and keyed end to end, so the wrong board fails the handshake
rather than leaking. Per-device keys (the easy-access ADR's "Revisit") would
close it.

The store lookup is the one store call on the device path: once per
registration, through `with_service`. Long-lived sockets never hold the store
lock (amendment to `2026-08-06-cloud-service-architecture.md`).

### 4. Presence lives in memory

One machine, so the hub's table of online boards (`RelayHub`, sans-IO, behind
its own lock) is in memory. A deploy drops everyone: shutdown closes every leg
`1001 going away` first, and a board told "going away" waits 2–15 s
(jittered) instead of its failure backoff (1 s doubling to 60 s, ±50 %), so
the reconnect storm spreads over the window. No table, no migration for
presence. The store gains one index (migration 0006, on
`account_access.key_salt`; not `UNIQUE`, so it cannot fail on existing data).

`ListBoards` (cloud API v5) answers the signed-in account's online boards:
id, name, wire version, LAN address, since when, and **`sameNetwork`** — the
browser's and the board's public addresses (fly's `Fly-Client-IP`) match, so
M8 can try the LAN only when it can work and a phone away from home never
sees a pointless Local Network prompt. Guests get an empty list.

### 5. Who may reach a board through the relay — the interim rule (Yona, 2026-10-06)

**This rule is interim.** Relay access is meant to be governed by
cloud-side access settings for each board — a board registry, owner
sharing, share links — which the cloud does not have yet (planning notes:
`lp2025/2026-10-06-1500-cloud-board-access/notes.md`). So the decision lives
in one file, `lp-cloud-server/src/relay/route_admission.rs`, behind one
small interface (`RouteAdmissionPolicy`: the session, the board id and the
board's presence in, member / visitor / a close code out), and nowhere else:
the hub only routes, and the board only enforces its own tiers. The cloud
check replaces or wraps the interim policy there.

The interim rule:

1. **A signed-in browser session is required** — an account or a guest. It
   costs a real user nothing (Studio always has one) and gives the limits
   below something to hold on to. It could be relaxed later.
2. **An account that has plugged the board in by USB** (the board proved its
   key) reaches it and gets that key's tier.
3. **Anyone else** may open a session to an online board by its id, and gets
   in with the **board's own Play/Author password**, inside the sealed
   session, exactly as a LAN or Bluetooth client does. The relay never sees
   the password. Finding a board you do not own takes a share link from its
   owner's Studio (M8).
4. **The board's "Anyone" (open) setting never applies over the relay.** A
   relay link is `LinkTrust::Relayed` on the board: keyed like a LAN link,
   but its tier is only what its handshake's key grants. The anonymous key
   completes a handshake (so a visitor can fetch the login offers and then
   re-handshake with the password-derived key) but holds nothing, whatever
   the board's open setting says. This is the second lock, and it is the
   board's, so it holds even against a hub that lies.

### 6. Threat model

- **The relay is untrusted** (network-link ADR §6). It learns which board a
  session goes to, the session's key id, sizes and timing; never a payload,
  a tier or a key. It can drop or delay. Because it mints account keys it
  could sit in the middle of an account-key session (accepted, D5); it
  cannot for a password session.
- **Board ids are MACs, not secrets.** An Espressif prefix plus 24 bits is
  enumerable in principle. Every session open that is not a member reaching
  its own board — a visitor, or any try at an id that is not online — takes
  a token from a per-address bucket (20 at once, then one per 30 s; close
  code `4420 slow-down`). One address can therefore probe about 2,900 ids a
  day: sweeping one vendor prefix (2^24 ids) would take it about 16 years,
  and an attacker with a thousand addresses about a week. What a sweep wins
  is a list of online boards to guess passwords against, which the next
  point bounds. An unguessable relay id (a random id the board would have to
  persist) would close it; not now.
- **Password guessing through the relay** is bounded twice: by the same
  per-address bucket (each visitor session is a token), and by the board's
  own login backoff, which is device-wide and survives the link closing — a
  wrong key on a relayed link goes through the same `key_wrong` path as a
  LAN link, and every lookup is refused while the board is backing off
  (pinned in `lpa-server/tests/access_gate.rs`). A typed password is still
  guessable offline from a recorded handshake (network-link ADR, D4); the
  relay records nothing, but an attacker who is the relay could.
- **Cross-site WebSocket.** The session cookie is `SameSite=Lax`, so a
  third-party page's socket to `/relay/board/<id>` carries no cookie and is
  refused `4401`; were one to get through, it would reach only what an
  anonymous visitor does — nothing until a password.
- **A refused upgrade is invisible to a browser,** so every browser-leg
  refusal is a close code after the upgrade (`4401 sign-in-required`,
  `4404 board-offline`, `4410 board-gone`, `4420 slow-down`, `4429 busy`;
  `lpc_relay::RelayCloseCode`, with plain words for each).
- **Deploys** drop every session (above); lp-link's own reset brings a
  session back on the client's next connect.

### 7. Host stand-ins

`lp-cli serve --relay <origin>` puts lp-cli's host board on the relay (the
board's own `RelayClient`, max one session like the C6), and
`relay:<id>[@<origin>]` is a client (session from `LP_CLOUD_SESSION`, never
argv), so M8 builds against a board-shaped peer before the firmware lands.
`lp-cli/tests/relay_link.rs` runs both against an in-process
`lp-cloud-server`.

### Device side (PR B)

To be written by P11: the shared network slot and same-key takeover (D2), DNS
on the C6, the relay client in the core, buffers and the measured cost.

## Consequences

- lightplayer.app serves one plain-HTTP path, and the redirect that keeps
  everything else on https is now ours to keep: the route-walk test fails on
  a new route without a case, and every deploy smokes it.
- Cloud API v5 (`ListBoards`); a tab on v4 is refused by name until reload.
- The relay's protocol is the first LightPlayer protocol with a
  compatibility promise. Every change to `lpc-relay`'s bytes bumps
  `RELAY_PROTO_VERSION`, and the hub keeps old versions listed while boards
  speak them.
- Every deploy drops every board for 2–15 s.
- A board reachable through the relay is reachable by anyone who learns its
  id and its password. That is the point (helping Sean set up his board), and
  why "Anyone" never applies there.

## Alternatives Considered

- **TLS on the device leg.** Too big for the C6 (above). A wrapper on
  `ByteStream` keeps it possible for chips that can afford it.
- **A second fly service or IP for plain HTTP.** Two services to deploy and
  monitor for one path; one service with an app redirect is less.
- **A relay over the device wire (no own version).** Breaks the first time a
  deploy changes a frame while lamps in the field still speak the old one.
- **Presence in the store.** A table written on every connect and dropped on
  every deploy, for a "last seen" no one has asked for yet.
- **Members only through the relay** (the plan's first answer to Q1).
  Rejected by Yona: a friend's board could only be set up by plugging it into
  your own Studio first.
- **Applying the board's "Anyone" setting through the relay.** It means
  "anyone nearby"; over the internet a board id is not a secret.
- **No session required for visitors.** Workable, and possible later; a
  session costs a real user nothing and gives the limits a handle.

## Follow-ups

- PR B (P6–P11): the board side, the wire field (`NetworkStatus.relay`), the
  emulator's uplink, measurement, and this ADR's device half.
- M8: Studio's relay provider, the board list, the share link
  (`lightplayer.app/b/<id>`), the automatic LAN upgrade.
- Cloud-side access settings for each board (registry, owner sharing,
  share links) replacing the interim admission rule
  (`lp2025/2026-10-06-1500-cloud-board-access/notes.md`).
- An unguessable relay id, if enumeration ever matters more than the
  per-address limit and the board's backoff allow.
- Per-device keys (closes board impersonation within an account, and salt
  linkability).
- The relay from beta channels (cross-origin, no cookie): a token, later.
- Relay metrics (fly has no APM by choice).
