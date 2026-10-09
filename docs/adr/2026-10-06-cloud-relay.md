# ADR: The cloud relay — boards on Wi-Fi reachable through lightplayer.app

- **Status:** Accepted at PR A's ship call (#999, merged 2026-10-07; sections 1–7). The device half ("Device side", PR B, #1019) is proposed until that PR ships, and nothing in it has run on silicon.
- **Date:** 2026-10-06
- **Deciders:** Photomancer (Yona; the Wi-Fi roadmap director)
- **Supersedes:** None
- **Superseded by:** None
- **Plan:** lp2025/2026-10-06-0815-wifi-relay (Wi-Fi roadmap M7)
- **Status note, 2026-10-08:** PR B (#1019) merged as `27b5b4b1f`, so the
  device half is in the shipped core. Its desk sitting (plan P10) has run
  since the "Device side" section below was written: on silicon (a C6, the
  test access point, a LAN-local relay) all ten of its rows passed, and the
  LAN round trip missed the M6 target and was reported. The internet path
  from silicon (NAT, the real proxy, real DNS) is still unwalked; it is the
  Wi-Fi roadmap's closeout walk. The decisions in this ADR are unchanged,
  and the "no desk sitting has happened" line in "Device side" is the state
  at the time it was written.
- **Related (Wi-Fi control roadmap, M7):** `2026-10-01-network-link-security`
  (the link every relayed session runs; the relay is untrusted),
  `2026-10-07-c6-wifi-link` (the LAN link this shares the one network slot
  with), `2026-10-04-device-wifi-settings` (the Cloud relay switch and the
  relay states the board reports), `2026-10-05-emulator-seams` section 11
  (the uplink that lets an emulated C6 dial a relay),
  `2026-10-06-radio-frame-rate-budget`,
  `2026-10-02-c6-repartition-and-layout-migration` (the flash the client's
  +29,840 B is spent from)

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
are sealed. (Since relay protocol 2 a board's picture and its project's name
also cross this leg in the clear, by decision: see the 2026-10-09
amendment.) TLS can be added later on chips that can afford it, as a wrapper
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
A board refused for its version asks again after an hour. (A protocol 2
board asks again after 5 minutes when it is refused `VersionTooNew`: see the
2026-10-09 amendment.)

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
`1001 going away` first, and a board told "going away" waits 2–12 s
(jittered — 12, not the plan's 15, so that with its registration's round
trips every board is back within 15 s) instead of its failure backoff (1 s doubling to 60 s, ±50 %), so
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

### Device side (PR B, #1019)

The C6 dials lightplayer.app by itself and holds its device leg. Every figure
below is emulated unless it says otherwise: **no desk sitting has happened**
(plan P10), so nothing here is validated on silicon.

**One network slot, shared (D2, RD9).** The C6 holds one secure network
session (about 14 KB with its link), and a relay route is a session too. So
the LAN's slot became **the network slot** (`NETWORK_LINK_SLOTS = 1`,
`fw-esp32-common/src/radio_link/radio_link_port.rs`): the LAN endpoint and the
relay driver take turns at it, and each link records the edge serving it.
A connection that finds the slot held does not open a second session. It
parks its first frame (the initiator's SYN, which carries Noise's msg1 and
names its key id in the clear, at most 96 B, in a buffer made with the slot)
and the link mux decides (`radio_link/parked_handshake.rs`):

- **another key id**, an anonymous one, or a holder whose session is not up:
  *busy*, at once, with no key lookup and nothing charged to the login
  backoff (a LAN newcomer is closed 1013; a relay route is closed `Busy`);
- **the holder's key id**: the server looks the key up through the same
  `lpc-access` path every handshake takes and checks msg1 against it. Only a
  msg1 that verifies takes the slot, so a stranger who knows a key id (they
  travel in the clear) can never knock a session down. The holder is closed
  and the newcomer opens with the parked frame first. A wrong key is a failed
  guess, charged to the login backoff like any other.

This runs in both directions: a `lan:` client with the key a relay session
holds takes the slot (what M8's automatic LAN upgrade rides), and a relay
route with the LAN session's key takes it back. The relay route a LAN client
displaces is closed `Normal`, with no reason of its own: a `TakenOver` close
reason would be a `RELAY_PROTO_VERSION` bump for a word the browser does not
need, and the director decided against it (DD199). A parked frame waits at
most 5 s for its verdict.

**What dials, and when (RD8).** Only when the station has an address, Cloud
relay is on, and the board holds at least one `SecretKind::Account` entry.
`RelayClient::may_dial` states it once, for the driver and the C6 task. The
backoff is `lpc-relay`'s: 1 s doubling to 60 s with ±50 % jitter; 2–12 s
after a hub that closed `1001 going away` (a deploy); an hour after a version
refusal; 10 s each to resolve, connect and register. A leg silent for 60 s is
closed by the driver, and the socket's own timeout is 75 s. Account entries
come from the access store once at boot and again on every `AccessAdd`,
`AccessRemove` and `AccessSetSwitches` (a new `AccessChanged` hook on
`LpServer`; a raw write of `/.lp/access.json` fires nothing, so the relay
picks those keys up at the next boot). The hello carries the board's MAC, its
name (`/.lp/device.json`, else `lp-xxxx`), the wire version and its LAN
address (`<ip>:80`); a new address sends `LanChanged`.

**DNS on the device.** embassy-net's `dns` feature is on (DHCP's DNS servers
feed it), `SOCKET_SLOTS` went 6 → 8 (the relay's TCP socket and the DNS
socket) and `net_address`'s watcher table grew by one. Name resolution costs
**+4,304 B** of core flash, measured with the feature switched on alone.

**The task is in the core.** `fw-esp32c6/src/net/relay_task.rs` runs on
`lp-net` beside the station, the LAN endpoint and mDNS, started from
`core_boot`, so an engine-less core reaches the relay too (OTA M8's rule). The
loop itself (`fw-esp32-common/src/net/relay/`: the driver and the leg, with
the WebSocket's client half in `net/ws/`) is shared with the host harness and
takes no `esp-*` crate; the C6 supplies embassy-net's DNS and TCP, the
hardware RNG and the station's address. The split verifier places the relay
client in the core (0 core nodes in the engine region). **Core-only
registration has not been exercised**: `test-emu-c6-split-boot`'s S8 boots
core-only with the link up and the task running, but no uplink is wired to
that cell.

**Memory: allocated only while the board may dial (RD12 reversed, round 2).**
P8 first allocated the relay's buffers at boot with the LAN's. With
`projects/test/basic` loaded and a network session open that left the largest
free block at 13,448 B (relay session) / 13,384 B (LAN session); a probe with
Cloud relay on but no account key gave the same 13,448 B, and with Cloud relay
off 19,556 B. Yona ruled (2026-10-07), in order and stopping when the gate
case passed:

- **(a)** `run_relay_leg` allocates the leg's TCP and WebSocket buffers
  (5,830 B) the first time the driver asks to connect, keeps them across
  reconnects, and gives them back whenever the board may not dial. The
  allocation is fallible (`try_zeroed_bytes`): no room is a failed dial that
  the backoff retries, never a reset. A board with Cloud relay on and no
  account key holds nothing.
- **(b)** Each path's outgoing frame exists only while that path serves the
  session: the LAN's 1,088 B after its upgrade, the relay's 1,091 B while a
  route holds the slot. Neither path's whole set can go: the LAN listener
  must stay to take a same-key takeover and to answer anyone else busy, and
  the relay leg must stay registered so a second key through the relay is
  told busy. An image with no LAN endpoint at all (the most "a relay session
  frees the LAN" could give, and it breaks the takeover) put the relay-session
  row at only 16,600 B.
- **(c)** was not applied. The C6's read gate is **40 KiB free / 8 KiB
  block** (`READ_GATE`, from the 2026-10-07 read-frame budget: a read on a
  fragmented heap shrinks its frames to half the largest block), not the
  16,384 B the plan quoted from the LAN's earlier gate. Against the gate the
  firmware runs, every row below passes in every run (DD208).

**Measured, emulated** (`lp-emu:esp32c6:t1+net=lan@81816d2f4`, the board's
heartbeat, 2 runs; free bytes never move run to run):

| State | Free | Largest block |
|---|---:|---:|
| joined, no account key | 174,568 B | 95,340 B |
| registered, no session | 168,668 B | 89,500–89,508 B |
| `projects/test/basic` loaded, relay session open | 57,848 B | 20,368 B (2/2) |
| `projects/test/basic` loaded, LAN session open | 57,784 B | 20,368 B (2/2) |

Registering costs about **5.9 KB** of free heap (P9 measured 7.0 KB before
(a); the plan's line was 8 KB). **The gate case's largest block is bimodal
run to run**, because host timing moves the order of the emulated board's
allocations: at the previous head (`af4c35ded`, 6 runs) the relay-session row
read 20,432 B in 4 of 5 runs and 13,440 B in 1, and the LAN-session row
20,424–20,440 B in 5 of 6 and 13,384 B in 1. Which step did the work: (b), in
most runs and not all; (a) alone moved nothing there, because the relay dials
in that case. A scratch probe of (a) itself (a network saved, a LAN session
open, not committed) read the same free bytes with Cloud relay on and no key
as with it off, on every image, but the largest block differed on the last
one (15,880 / 15,880 / 19,604 B on, against 26,508 B ×3 off): same bytes, a
different layout, reported as measured and not tuned. At boot, with nothing
saved, the C6 heap record moved from 103,000 to **105,580 B used / 195,956 B
free / 116,840 B largest** (the network slots' parked-handshake buffers and
per-edge signals, two more socket slots and the DNS socket; CI's clean
figures). **`lp-net`'s stack high water with DNS and the handshake was not
measured** (it needs a `net_thread_stack_diag` build).

**Flash.** The relay client alone is **+29,840 B** of core (P8's relay task
over its own DNS), above the plan's 24 KB line (A3). The slot, challenge, mux
and wire field cost +5,200 B of core and +4,484 B of engine. At this PR's last
firmware change before the last merge of main the core sat 64 B under its
32 KiB page; the merge of main's #1005 (core-only Bluetooth updates, +6,768 B
with the core-only arms this branch needed to match) crossed it (DD209), so
the page is main's growth rather than this PR's.
CI's build of `a303512e4` reads **88,308 B gated headroom**: above CI's 64 KB
line, below the plan's 128 KB bar, accepted by Yona on 2026-10-07 as "OK but
tight" (a local build at `81816d2f4` read 89,146 B). The spend is in
`2026-07-28-esp32c6-flash-budget.md`'s ledger. A line the board prints on
every relay state change was written and **taken out**: 688 B of core pushed
the core 224 B over a page and cost the update headroom 32 KiB at once. So
the board's console says its relay state only in the heartbeat's
`[relay] state=… routes=… rx=… tx=…` line (and a walk reads the board's status
answers instead); there is no line when the state changes.

**Where the desk image dials (RD14).** The product image dials
`lightplayer.app:80`, always. `LP_RELAY_HOST=<host>[:port]` at build time
(`fw-esp32c6/build.rs`) makes a **desk image** that dials that instead and
says so on the console at boot (`[INIT] desk image: the relay is …`). It
exists so the desk sitting can use a local `lp-cloud-server` and a dev account
without anyone's real session. Nothing in the release workflows sets it.

**Status.** `NetworkStatus.relay` carries `off`, `noAccount`,
`waitingForInternet`, `connecting`, `connected` or `refused { reason }`
(`unknownAccount`, `updateFirmware`, `busy`): one wire bump, to 40. The words
are core's (`wifi_words::relay`) and `lp-cli wifi status` reads the same
ones. See the device-wifi-settings ADR's amendment.

**The emulator reaches it** through an uplink on the virtual LAN (the
emulator-seams ADR, section 11's amendment), so an emulated C6 resolves and
dials `lightplayer.app` like a real one, with no change to the firmware.

**Not proven here, and by whom.** The desk sitting (P10) owns what the
emulator cannot answer: the real radio, a home router's NAT, real DNS, the
internet path to the service, whether the leg survives 30 minutes idle
through fly's proxy and a NAT (R3), the 15 s reconnect after a deploy (a
desk-only measurement, DD200: the emulated cell prints the time and does not
assert it, and its spread was 7.2–47.4 s of wall time on a loaded host,
undiagnosed), lp-link's timeouts against a real relay's round trip (R2; none
was changed), and the frame-rate budget with the relay joined (the radio
budget ADR's ceilings). One compatibility promise carries over to the board:
a fielded core's `RELAY_PROTO_VERSION` has to stay listed in the hub for as
long as boards run it, which matters most for cores that will update over the
relay (OTA M8).

## Amendment (2026-10-08): a fielded core's relay version is never refused (OTA M8, W7)

Plan `lp2025/2026-10-06-2249-ota-wifi-updates`, PR C: boards now take
firmware updates through the relay (channel 3 on a relayed link,
`docs/adr/2026-10-06-ota-update-protocol.md`'s amendment of this date), and
the core is what dials the relay, in core-only too. A house board with no
cable and no Studio on its network has no other way to be updated, so:

- **The device leg of every fielded core is a never-break surface.** Once a
  core that updates through the relay is in a release, the hub keeps that
  core's `RELAY_PROTO_VERSION` in `SUPPORTED_RELAY_PROTO_VERSIONS` for as
  long as such cores may exist, and keeps answering its hello, proof,
  routes and frames as they are today. Dropping a version needs a decision
  of its own (and a path for the boards on it), never a deploy.
- `lpc-relay/tests/relay_frame_golden.rs` is already never edited to make a
  change pass; with this it pins what fielded cores speak, beside lp-link's
  and channel 3's goldens.
- A new relay feature is a new version **added** to the list; the old one
  stays.

No version moves in PR C: nothing about the relay's bytes changed.

## Amendment (2026-10-09): relay protocol 2 — pictures through the cloud

Plan `lp2025/2026-10-08-2050-pictures-through-the-cloud` (the
boards-and-projects roadmap's M8), PR #1066. A board on the relay now sends
its LED colours, and the project it runs, to lightplayer.app. The hub keeps
the latest picture beside presence, so anyone in the board's accounts can
see what it shows without taking its one network slot. This is **relay
protocol 2**, added beside protocol 1. Nothing earlier in this ADR changes
except the two sentences that now point here.

### Protocol 2 beside protocol 1

`SUPPORTED_RELAY_PROTO_VERSIONS` is `[1, 2]` (a test fails if 1 ever leaves
it), and `RELAY_PROTO_VERSION`, what a new board speaks, is 2. Protocol 1's
nine tags (`0x01`–`0x09`) are unchanged, and `RelayHello::new` is still the
protocol 1 hello (protocol 1's goldens build their hellos with it).

| Tag | Frame | Direction | Fields after the tag |
|---|---|---|---|
| `0x01` | Hello at `relay_proto` 2 | board → hub | protocol 1's fields, then `firmware`: `len u8` (≤ 40), ASCII (`RelayHello::with_firmware`) |
| `0x0a` | `Project` | board → hub | `0` = no project; or `1`, `name` (`len u8` ≤ 32, UTF-8), `uid_tag` (`0` / `1` + 16 B), `content_tag` (`0` / `1` + 16 B) |
| `0x0b` | `Picture` | board → hub | `n u8` (≤ 16 outputs), `n × lamps u32`, `count u16`, `count × [r g b]` |
| `0x0c` | `PictureRate` | hub → board | `idle_s u16`, `watched_ms u16`, `watched_for_s u16` |

Protocol 2's bytes, and the tag derivations' vectors, are pinned in
`lpc-relay/tests/relay_frame_golden_v2.rs`. The board links only its
direction's half of the codec (`RelayFrame::decode_from_hub`,
`encode_to_hub`: the same bytes, pinned against the full codec by a test).
That cut is worth 3.9 KB of C6 core (3,888 B, measured on the split image):
it took this change's core growth from about +7.2 KB to +3.3 KB.

### What a picture means

Lamps per output, in the project's tree order; with `T` their sum, sample
`i` is lamp `⌊i·T/count⌋` of the outputs concatenated. `count` is 0 exactly
when `T` is 0, otherwise `1 ≤ count ≤ T`. Each sample is R, G, B as the
sRGB8 display codes Studio's card draws: a `U16` sample goes through
`linear16_to_srgb8`, and a `U8` one is widened ×257 first, so the codes mean
one thing whatever the output publishes. The colour order is undone by the
`RgbPixels` span covering the lamp; a lamp no span covers is read in wire
order. The samples are the published, finished output (after the engine's
finalize). A board sends at most 256 samples (`DEFAULT_PICTURE_SAMPLES`);
the hub takes any count that fits a frame. `RelayPicture`'s doc is the
normative text; the engine's sampler (`Engine::append_output_picture`,
`&self`, reading the published buffers in place) follows it rule by rule,
each with a test.

### The project, by keyed tags

A project's uid and its package hash are read capabilities: a link-viewable
project opens by its uid, and blobs and trees are served by hash
(`2026-08-08-project-url-identity-and-sharing.md`, `blob_route.rs`). The
device leg is plain HTTP. So a board sends its project's **name** in the
clear and the uid only as a tag:

```text
tag_key     = HMAC-SHA256(K, "lp-relay project/1")
uid_tag     = HMAC-SHA256(tag_key, "uid\0" ‖ uid)[..16]
content_tag = HMAC-SHA256(tag_key, "content\0" ‖ package_hash)[..16]
```

`K` is the stored entry key of the first account the hub verified (the
lowest bit of `Registered.accounts_ok`, the hub's `BoardAccounts.users[0]`).
The cloud holds `K`, so it can match a tag to a project of that account;
anyone on the path learns nothing they could open. A board reports its
project after every `Registered` and whenever it changes. `content_tag` is
defined and pinned, and no board sends it yet. The firmware's version
rides the hello. That answers the vision's "which project, which version"
with no further protocol bump.

### What crosses the device leg in the clear, and why (Yona, 2026-10-08)

Pictures and project names cross the plain-HTTP device leg **unencrypted**.
Anyone on the path (an open Wi‑Fi, the ISP) can see what a board's lamps
show, sampled, and when, and the name of the project it plays — as they can
already see the board's own name in its hello. Sealing them to the account
key would cost core flash the C6 barely has and a more complex format. The
capabilities never cross: no uid, no package hash, no key, only the tags
above. A later protocol can seal pictures and names, or the leg can gain
TLS on chips that afford it (section 1's path).

### The cadence: the hub decides, the board clamps

The hub sends `PictureRate` right after `Registered` (protocol 2 only), and
a board sends a picture at once on every rate it gets. The hub's defaults:
**idle**, one every 60 s; **watched**, one every 500 ms, for a lease of 15 s
that a member's read renews (the board is sent a new rate only when less
than half the lease is left). The board clamps what it is told — at least
250 ms between pictures, idle 10–3600 s (or 0, none), a watch of at most
300 s — and falls back to idle by itself when the watch runs out, so a
closed tab costs nothing. At most one picture is in flight. Two knobs
(`lp-cloud-server`'s `config.rs`): `LP_CLOUD_RELAY_PICTURE_IDLE_S` (60;
0 = none) and `LP_CLOUD_RELAY_PICTURE_WATCHED_MS` (500; 0 = never fast). A
home page that watches every board keeps every visible board at two
pictures a second while it is open.

### The cache: memory only, members only

The hub keeps each board's last picture and project beside presence
(`picture_cache.rs`), and keeps it — marked offline — after the board
leaves, until the next deploy. A deploy loses the cache; online boards
refill it within seconds, because each sends a picture as soon as the new
hub asks. At most 4,096 boards (the oldest offline entry goes first); a
picture less than 0.2 s after the board's last is dropped (a guard against a
broken board, never a close). Readers are the accounts the board proved:
guests, visitors and other accounts read nothing and cannot make a board
fast. Persisting the last picture is new persisted cloud data and belongs
with the account's board list (M7).

### Cloud API v6

`BoardPictures { boards: [{ id, seq? }], watch }` →
`BoardPictureList { pictures: [{ id, online, seq, at, outputs, colors? }] }`
(colours left out when the caller's `seq` is current; at most 16 boards a
call; `watch` renews the lease). `BoardPresence` (in `ListBoards`) gains
`relayProto`, `firmware` and `project` (the name). A tab on v5 is refused by
name until it reloads (the API's version-and-refuse).

### The per-version rule

**The hub never sends a protocol 2 frame to a protocol 1 leg.** A protocol 1
client treats any frame it does not expect as a protocol error and drops
its leg, so one stray frame would put every fielded board into a reconnect
loop. Every send to a board goes through one function (`to_board`) that
compares `frame_protocol` with the board's: a frame above it is dropped and
logged, and a debug build fails outright. Hub tests assert it on every
action path; a protocol 1 board that sends a picture is closed.

### Never-break, now for two versions

Protocol 1's bytes stay pinned by `tests/relay_frame_golden.rs`, which this
change did not touch (`git diff --exit-code origin/main --` on it is
empty). Protocol 2's are pinned by `tests/relay_frame_golden_v2.rs` and
become never-break from the first release whose core speaks them. From that
day, **rolling the cloud back past the protocol 2 hub is a never-break
violation**: a board updated to protocol 2 would be refused `VersionTooNew`
by the old hub.

### The deploy window and the retry

A merge marks its firmware release Latest a few minutes before the hub that
accepts its protocol deploys (the deploy waits for the release). A board
updated in that window is refused `VersionTooNew`: over the relay its trial
core cannot confirm and it returns to the old core by itself; over USB or
the LAN it waits. So a protocol 2 board asks again after **5 minutes** when
refused `VersionTooNew` (the hub is behind and will catch up), and keeps the
hour for `VersionTooOld`. The post-deploy smoke in `deploy-cloud.yml` sends
the live hub a protocol 1 hello and a protocol 2 hello (no accounts) and
expects a `Challenge` for each.

### What it costs the C6

CI's own builds of the split image (`just fw-esp32c6-size-check`'s
figures):

| | Core | Engine | Gated headroom | Core → next 32 KiB page |
|---|---:|---:|---:|---:|
| `main` at `f5039fb93` (run 37905412888) | 1,426,368 B | 1,845,046 B | 88,266 B | 15,424 B |
| PR #1066's merge commit `2a42c672f` (run 37917190206) | 1,429,648 B (+3,280) | 1,848,476 B (+3,430) | 84,836 B | 12,144 B |

Local builds read the same growth (core +3,280 B, engine +3,412 B). The
plan's targets (core and engine ≤ 4 KB each, no page crossed) hold; the
engine still starts at `0x178000`. No new log line is in the core.

Heap, emulated (`lp-emu:esp32c6:t1+net=lan@d1efe5028`, the board's own
heartbeat): with `projects/test/basic` loaded, the relay registered, the
board **watched** and a relay session open, **63,232 B free, 17,644 B
largest block**, over the C6 read gate (40 KiB / 8 KiB); being watched costs
68 B of free heap and nothing of the largest block. A registered board
keeps one 836 B picture buffer (`MAX_BOARD_PICTURE_FRAME`) while its leg is
up, released when the leg ends, and none while it may not dial. The boot
heap ratchet sits inside `main`'s own spread (CI: 105,716 B used on the
PR's merge commit; 105,708 and 105,740 B in one run on `main`). Every row
is in the walk record.

### What is emulated only

Every claim here was walked on the emulator, recorded in
`docs/reports/2026-10-09-relay-pictures-emulator-walk.md`: the relay walk
(13/13, in the board's own words), **the protocol 1 lane** — CI's image of
the last protocol 1 commit (`f5039fb93`) at the new hub, registered, routed,
listed at `relayProto` 1, one leg across a minute of being watched, and its
received bytes unchanged throughout — and **the crossing walk**, that core
updated to this build through the relay, which then sent pictures. The
frame-rate cost on silicon (the radio budget's idle row) is queued for a
desk sitting and does not block; the radio budget ADR is not amended, as no
silicon number lands here.

## Consequences

- lightplayer.app serves one plain-HTTP path, and the redirect that keeps
  everything else on https is now ours to keep: the route-walk test fails on
  a new route without a case, and every deploy smokes it.
- Cloud API v5 (`ListBoards`); a tab on v4 is refused by name until reload.
- The relay's protocol is the first LightPlayer protocol with a
  compatibility promise. Every change to `lpc-relay`'s bytes bumps
  `RELAY_PROTO_VERSION`, and the hub keeps old versions listed while boards
  speak them.
- Every deploy drops every board for 2–12 s, and every session on it.
- On a C6 the LAN and the relay share one session: while one person is on
  Wi-Fi, a second (by either path) is told "busy" unless it proves the same
  key, in which case it takes over.
- The C6 pays for it in the core: +29,840 B of flash for the client and
  +4,304 B for DNS, about 2.6 KB more heap at boot and about 5.9 KB more only
  while it may dial. The gated headroom is 88,308 B (CI's build),
  under the plan's 128 KB bar and over CI's 64 KB line; the next core
  growth has to be weighed against it.
- A board reachable through the relay is reachable by anyone who learns its
  id and its password. That is the point (helping Sean set up his board), and
  why "Anyone" never applies there.
- (2026-10-09, relay protocol 2) Cloud API v6 (`BoardPictures`; a tab on v5
  is refused by name until reload). A board's pictures and its project's
  name cross the device leg in the clear; its uid and package hash never
  do. The hub holds every board's last picture in memory, lost at a deploy.
  The hub now speaks two protocols, and must never send protocol 1 a frame
  it does not know.

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

- PR B (#1019): the board side, the wire field (`NetworkStatus.relay`), the
  emulator's uplink and this ADR's device half are written; the desk sitting
  (P10) is what moves them from emulated to measured.
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
- (Relay protocol 2) Draw a board's cloud picture on its card in Studio (M7,
  or the director's small follow-on); persist the last picture with M7's
  board list; fill `content_tag` when something needs "which version of the
  project"; seal pictures and names (a later protocol) or TLS on chips that
  afford it; a `LabelChanged` frame, since the hub's board name goes stale
  until the board registers again.
