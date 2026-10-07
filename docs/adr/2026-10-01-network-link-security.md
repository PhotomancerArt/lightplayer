# ADR: Network link security — Noise NNpsk0 inside lp-link, the key match as the login

- **Status:** Accepted (2026-10-01; G0 by Yona: "yes, that seems fine").
  Implemented as lp-link's `secure` feature (PR #894); **no product link
  turns it on yet** — the first is M6's LAN WebSocket.
- **Date:** 2026-10-01
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Planning:** roadmap `lp2025/2026-10-01-1832-wifi-control` (M4), plan
  `lp2025/2026-10-01-1843-secure-link`; decisions D2–D6 and D9 and their
  measurements in `lp2025/2026-10-01-0300-wifi-control-experiments`
  (`decisions.md`, `report.md`).

## Context

Wi-Fi control (D9) gives a device two network links: **LAN direct** (a
WebSocket server on the device, found by mDNS) and a **relay** on
lightplayer.app that the device dials when the internet is reachable. Both
are untrusted: anyone on the LAN can connect, and the relay is a pipe we run
but do not want to be able to read.

TLS does not fit the shipping chip. The 2026-10-01 bench night measured, on
the C6: a TLS 1.3 client at **+79 KB flash** (unverified certificates) and
**~29 KB heap per connection** (a 16 KB record buffer is forced), against
~186 KB of headroom that Wi-Fi + WebSocket + mDNS already take ~82 KB of;
and a TLS-terminating relay could read everything anyway. Sealing each
message with ChaCha20-Poly1305 measured **+6.7 KB flash** and 3.4 ms per 2 KB
seal+open on silicon.

Access is already modelled for an untrusted link
(`2026-09-23-ble-access-model.md`, `2026-09-24-easy-bluetooth-access.md`):
shared secrets (`K`, a 16-byte salt per holder), tiers (play, edit), an HMAC
challenge-response login, a per-device backoff. That ADR said "WiFi reuses
the model"; this one says how, once the link is encrypted.

## Decision

### 1. Noise NNpsk0, end to end, inside lp-link

Every untrusted network link runs **`Noise_NNpsk0_25519_ChaChaPoly_SHA256`**
end to end, client ↔ device. It lives in lp-link behind the **`secure`**
cargo feature (`lp-base/lp-link/src/secure_channel/`), not in a new crate
(D6); it splits out as `lp-link-secure` only if a second mode or a separate
review boundary ever needs it. The initiator is the client (Studio, lp-cli,
an app) and holds a key; the responder is the device and holds none until it
looks one up. Sans-IO like the rest of lp-link: the edge injects entropy
(`fn(&mut [u8])`, 32 bytes per handshake per end) and time; nothing reads a
clock or draws randomness.

### 2. The handshake rides lp-link's SYN

The SYN keeps its 12 bytes; its flags byte gains bit 1 `SECURE` and bits 2–3
the Noise content, and the message follows:

| content | sent by | after the 12 bytes | body |
|---|---|---|---|
| presence | responder, nobody heard | — | 12 B |
| msg1 (`psk, e`) | initiator, every SYN while connecting | `key_id[16] ‖ e_i[32] ‖ tag[16]` | 76 B |
| msg2 (`e, ee`) | responder | `e_r[32] ‖ enc(responder nonce)[4] ‖ tag[16]` | 64 B |
| refusal | responder | `reason[1] ‖ retry_after_ms[4]` (unknown, wrong, backoff, busy) | 17 B |

The **prologue** is `"lp-link/secure/1" ‖ key_id ‖ initiator nonce`, msg1's
payload is empty, and msg2's is the responder's lp-link nonce, so both
lp-link nonces are bound into the transcript: **one Noise session is exactly
one lp-link session**. One ephemeral per initiator session (a resent msg1 is
the same bytes). The responder parks msg1 and asks its edge for the key
(`KeyLookup`), tries each candidate PSK (no DH per candidate), writes msg2 for
the first that verifies and goes half-open; the initiator's first sealed
frame is the key confirmation and brings the responder up. Host request at 1
RTT plus one server tick for the lookup; both up at 1.5 RTT, as a plain link.
**Every reset wipes the session's keys**; the next session is a fresh
handshake with fresh ephemerals.

**A plain link's bytes do not move** (pinned by
`lp-base/lp-link/tests/plain_bytes_golden.rs`, captured from `main` before
the codec changed), and without the feature a plain link compiles to exactly
the code it did (the size probe's plain variants are byte-for-byte main's).

### 3. The key id is the entry's salt; the key match is the login

- **Key id = the access entry's 16-byte salt**, in the clear in msg1. It is
  already each entry's public identity (`AccessRemove` names it; a login
  challenge offers every salt to anyone in range). The client knows its own
  salts without asking.
- **PSK = `HMAC-SHA256(K, "lp-link psk/1")`** (`lpc_access::link_psk`),
  derived on both ends and domain-separated from the login MAC, which keys
  HMAC with `K` directly.
- **The handshake that completes is the login.** The link is
  `LinkTrust::Keyed`; the server answers the lookup from its installed
  secrets (store and loaded sidecars, the same base-fs read as a login) and
  grants the matched entry's tier; the hello on that link reports it.
- **Backoff:** a known salt whose PSK does not match is a failed guess,
  charged to the device's backoff, the same one logins use; an **unknown
  salt tested no secret and is not charged**; a lookup in backoff is refused
  with its wait and tries nothing. A real key's success clears the backoff,
  as a login's does.
- **The anonymous key** (all-zero salt, all-zero PSK) gets an encrypted
  session holding only what `open` gives. No entry may have an all-zero salt
  (`upsert_secret` refuses one).
- **The HMAC login is off on keyed links.** `LoginBegin` returns the offers
  (a typed-password client's salts and costs) without registering a login;
  `LoginAnswer` is refused. Otherwise a relay that sits in the middle of an
  anonymous session could pass an HMAC login through it. There is exactly one
  way to earn a tier on a secure link: a keyed handshake.

### 4. Every frame after the handshake is sealed

`header[4] ‖ ctr[4] ‖ ciphertext ‖ tag[16] ‖ crc`: ChaCha20-Poly1305 under
this direction's key from `Split()`, nonce = Noise's `0^32 ‖ LE64(ctr)`,
**the header as associated data** (Noise §11.4's out-of-order transport with
an explicit nonce). One 32-bit counter per direction, **re-sealed on every
transmission**, retransmissions included (a resend carries a new ACK and
window, so reusing its ciphertext would reuse a nonce under different AD).
The **CRC stays**: a CRC failure is line damage, a frame that passes it and
fails its tag is a forgery or a bug — two counters (`bad_frames`,
`seal_failures`). Replay: a **64-frame window on ARQ links** (dropped and
counted; ARQ resends what was lost); **strict on no-ARQ links**, where a gap
or a bad tag **resets** the session (nothing resends there, so a missing
frame is lost data, and through a relay a tamper signal). The last counter is
never used: a session that reaches it restarts (~49 days at 1,000 frames/s).
**A responder resets an established session only for a msg1 that verifies**;
a plain SYN or a failing msg1 never knocks it down.

### 5. TLS wraps, never replaces (D3)

TLS, behind a feature flag on chips that can afford it (S31-class), protects
only the outer hop (device ↔ relay, or a future LAN `wss://`). Noise runs
inside it unchanged, so every chip speaks the same protocol and the flag
changes how a socket opens, not what runs over it.

### 6. The relay is untrusted; what anyone on the path learns

- **Learns:** that a secure lp-link session starts; the **key id** (which
  holder — browser, account or password entry — and, because one holder uses
  one salt everywhere, linkable across devices); both lp-link nonces; frame
  kinds, channels, sequence/ACK/window fields, sizes and timing (the header
  is authenticated, not encrypted); refusal reasons.
- **Does not learn:** any payload, the tier, labels, or a PSK.
- **Can:** drop, delay or reorder frames (ARQ heals it; a no-ARQ link
  resets); forge a refusal or a reset-inducing SYN, or replay an old msg1 to
  knock a session down (on-path denial of service, which such an attacker has
  anyway); and **guess a password-derived PSK offline** from one recorded
  msg1 (below).

### 7. Crypto sources

Curve and AEAD arithmetic are RustCrypto's: `x25519-dalek` 2 /
`curve25519-dalek` 4 with **`precomputed-tables` off** (both the public key
and the DH go through the Montgomery ladder, so no Edwards code is linked),
`chacha20poly1305` 0.10, the workspace `sha2`. HMAC-SHA256, Noise's HKDF and
the state machine are written from RFC 2104 and the Noise spec (rev 34).
`snow` (interop in both roles: equal split keys and handshake hash), RFC 8439
§2.8.2 and RustCrypto `hmac`/`hkdf` are **dev-dependency oracles only**.
`check-lp-link-targets` fails if `getrandom` or the precomputed tables reach
the rv32 or wasm32 graph.

### 8. Which links turn it on

| When | Link |
|---|---|
| M4 (this ADR) | **None in product.** lp-link's simulator on every preset, the host end-to-end test, the wasm build, a C6 diagnostic image |
| M6 | The C6 LAN WebSocket server — the first product secure link, **with the `WIRE_PROTO_VERSION` bump** |
| M7 | The relay (the same link inside the relay's routing frames) — **done in PR A, 2026-10-06** (`2026-10-06-cloud-relay.md`): a relay link is `LinkTrust::Relayed`, keyed like the LAN, but the board's open/"Anyone" setting never applies to it, so its anonymous key holds nothing |
| M8 | Studio's browser WebSocket provider (and the client key policy) |
| later | BLE, by its own decision (`ble().secured()` keeps a sealed frame in one notification) |
| never | USB and UART: the cable is the trust |

## Accepted limitations

- **D4 — password-derived PSKs can be guessed offline.** One recorded msg1
  lets an attacker test password guesses offline; a correct guess allows
  active impersonation in later sessions. Recorded sessions stay sealed
  (`ee` gives forward secrecy against a later PSK compromise). Generated
  256-bit browser and account keys, the default path, are not guessable.
- **D5 — the cloud could MITM account-key sessions it relays.** It mints
  account keys, so it could actively sit in the middle of a session keyed
  with one. It cannot passively decrypt one, and browser-key sessions are out
  of its reach.
- **Salt linkability.** The key id names the holder, linkable across devices
  (the easy-access ADR's "Revisit: per-device keys" fixes BLE and network
  links together).
- **Anonymous sessions** are MITM-able by design and hold only `open`'s
  tier. A client must not fall back to anonymous silently after a keyed
  handshake fails (M8's policy).
- **On-path denial of service:** replayed msg1s, forged refusals and SYNs,
  dropped frames. Accepted.
- **Visible headers and SYN fields.** Frame kinds, channels, sequence
  numbers, sizes and timing are visible; a SYN's `max_payload` and window
  are not bound into the handshake (tampering with them is denial of
  service, which an on-path attacker has anyway).
- **Text outside frames** on a stream link (boot text, a panic) is not
  authenticated; a network link carries none.

## Consequences

- **No `WIRE_PROTO_VERSION` bump in M4:** plain links are byte-identical and
  no wire message changed shape. Turning `secure` on for a product link is a
  wire change and bumps it (M6); so is adding the secure counters
  (`handshakes`, `seal_failures`, `replays`, `counter_gaps`, …) to the
  heartbeat.
- **What it costs on the C6** (`diag_secure_link`, the product image with a
  secure pair run at boot; numbers from `lp-emu:esp32c6:t1@f06c77c6d`,
  where a PMU cycle is an instruction — **never time**; silicon timing is
  owed by M6's desk walk):

  | | |
  |---|---|
  | flash, the secure feature | ~25 KB (curve25519 9.5 KB, ChaCha20-Poly1305 3.7 KB, `secure_channel` 5.0–5.8 KB, the link's secure paths 5.7 KB, sha2 0.9 KB) |
  | flash, the measurement image over the product | +40,624 B (the above, plus the no-ARQ link's own monomorphization 8.2 KB and the probe) |
  | RAM per secure link | +752 B over a plain link of the same config (27,232 vs 26,480 B, `ws()` with the board's cut) |
  | a handshake, both ends | 12.07 M instructions (four X25519 operations) |
  | sealing | 34,361 instructions per 64 B message; 531,329 per 2 KB |
  | stack, one handshake | 3,860 B through the links; 3,204 B for the Noise core alone |

  The stack figure is over the plan's 2.5 KB line and over M1's 3 KB link IO
  thread: **where the secure link's handshake runs, and on what stack, is
  M6's decision**, and this is its input.
- **The product image:** lp-link's plain code is unchanged, but the
  server-side keyed-link handling (`LinkTrust::Keyed`, the transport hooks,
  the keyed login paths) is unconditional and lives in every image: +3,552 B
  on the C6 (2,963,952 B against 2,960,400 B; see the PR for the
  attribution).
- **M6's flash projection:** today's headroom 181,776 B, minus Wi-Fi + WS +
  mDNS (82,080 B measured), the secure link (24.9–25.8 KB) and a no-ARQ
  link's own code (8.2 KB), leaves ~65.7–66.6 KB: **within about 1 KB of CI's
  64 KiB (65,536 B) floor**. M6 fits before M3's partition redraw only just,
  with no margin; after it (~442 KB) comfortably.
- **A lookup belongs to the session it opens.** A transport tags a key
  lookup that arrives while a session is up with the *next* session's link
  id (the responder resets only once the msg1 verifies); the host test's
  transport (`lp-app/lpa-server/tests/support/secure_link_transport.rs`) is
  the worked example.

## Alternatives Considered

- **TLS end to end.** Too big for the C6 (above), and a relay that
  terminates it reads everything.
- **A separate crate.** D6: we have enough crates; a module behind a feature
  splits out later if it must.
- **Sealing per message, before fragmentation.** Leaves headers and ACKs
  unauthenticated and needs a nonce per channel; per frame survives ARQ's
  reorder and loss with one counter.
- **A blinded or absent key id with trial decryption.** Hides which holder
  connects, but clients must try their keys blind, and a typed-password
  client still needs the offers.
- **A synchronous key table shared with the link task.** A second copy of
  the secrets and a threading question on the C6; the deferred lookup costs
  at most one server tick per session.
- **The old HMAC login inside an NN session, with channel binding.** Two
  mechanisms where one does.
- **XXpsk / IK with device static keys.** Needs device identity keys; future
  work if a product need appears.

## Follow-ups

- M6: the first product secure link (LAN WebSocket), its stack and thread,
  the wire bump, the heartbeat's secure counters, silicon timing.
- Per-device keys (salt linkability), the easy-access "Revisit".
- Dropping the CRC on sealed frames (−4 B/frame), measured.
- Deduplicating HMAC-SHA256 between `lpc-access` and `secure_channel`.
- A no-ARQ receiver drops what overflows its budget without a reset
  (`rx_no_room`), plain or secure; the first product no-ARQ link settles it.
