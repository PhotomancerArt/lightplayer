# lpc-relay

The cloud relay's device-leg protocol. `no_std` + `alloc`, sans-IO: one
crate that the hub (`lp-cloud-server`), the board's firmware, lp-cli's host
board (`lp-cli serve --relay`) and the tests all share.

Decision record: `docs/adr/2026-10-06-cloud-relay.md`.

## The shape

```text
browser ──wss /relay/board/<mac>──► hub ◄──ws /relay/device── board
         (bare lp-link frames,        │      (one socket per board,
          one socket per session)     │       relay frames, route ids)
                                      │
                 the hub joins a browser socket to a route on the board
```

The lp-link session inside a route is Noise-sealed end to end. The hub sees
route ids and frame lengths, never their contents.

## What is here

| Module | What |
|---|---|
| `relay_frame` | `RelayFrame` and its codec: one byte of tag, fixed little-endian fields, length-checked, ≤ `MAX_RELAY_FRAME` (2 KiB). The table of tags is the module doc. A board uses its direction's half (`decode_from_hub`, `encode_to_hub`: the same bytes, only the frames that travel that way, pinned against the full codec by a test), so the C6's core links none of the hub's readers and writers: 3,888 B of core saved, which kept relay protocol 2 at +3.3 KB of core instead of +7.2 KB. |
| `relay_hello` | `RelayHello`: the board's MAC, name, wire version, LAN address and account salts. `RelayHello::new` is the protocol 1 hello; `.with_firmware(…)` makes it protocol 2, with the firmware version as its tail. |
| `relay_project` | Protocol 2. `RelayProject` (the project's name, its uid tag, its content tag) and the tags: `project_tag_key(K) = HMAC(K, "lp-relay project/1")`, `project_uid_tag`, `project_content_tag`. A uid and a package hash are read capabilities and never cross the leg; only their tags do. |
| `relay_picture` | Protocol 2. `RelayPicture`: lamps per output and point-sampled sRGB8 colours; its doc is the picture's meaning. `write_picture_header` writes the frame's head into a buffer a board keeps (its colours appended behind it, at most `MAX_BOARD_PICTURE_FRAME` bytes), byte-identical to encoding a `RelayPicture`; `picture_sample_count` is how many colours a board sends. |
| `picture_rate` | Protocol 2. `PictureRate` (hub → board) and the board's clamp, `PictureRate::clamped`. |
| `relay_proof` | `relay_auth_key(K) = HMAC(K, "lp-relay auth/1")`, `relay_proof(A, nonce, mac) = HMAC(A, nonce ‖ mac)`, and the hub's constant-time check. |
| `relay_version` | `RELAY_PROTO_1`, `RELAY_PROTO_2`, `RELAY_PROTO_VERSION`, `SUPPORTED_RELAY_PROTO_VERSIONS` and `check_relay_version`: version-and-refuse. |
| `refuse_reason`, `route_close_reason` | The one-byte reason codes. |
| `relay_limits` | Frame size, accounts per hello, routes per board, ping and silence intervals; protocol 2's firmware, name, tag and picture limits, and the board's clamps on a `PictureRate`. |
| `relay_client` | `RelayClient`: the board's state machine — when to dial (`may_dial`: joined, Cloud relay on, an account entry; the C6's driver and relay task both ask it), backoff, the challenge, the route table, the status; and (protocol 2) the hello's firmware, the project report (tags, never the uid or the hash), and the picture schedule (`picture_schedule.rs`: `TakePicture` out, `PictureReady` in, at most one in flight). |

## The registration

```text
board                              hub
  │── Hello {v, mac, salts…} ─────►│  refuse a version it does not list
  │◄──────────── Challenge {nonce} ─│  32 fresh bytes
  │── Proof {one per salt} ────────►│  find each account by salt, verify
  │◄── Registered {ok bits} ────────│  or Refused {reason}
  │◄──────────── Open {route} ──────│  a browser signed in to one of them
  │◄═════ Frame {route, lp-link} ══►│
```

`K` is the account entry's key as the board stores it — what Studio installs,
`PBKDF2(key_secret, key_salt, 1)`. The cloud minted the account key and can
compute the same `K`, so nothing secret crosses the wire.

## Versioning

The device wire (`lpc-wire`) keeps no compatibility, because Studio, lp-cli
and firmware ship together. The relay cannot: a lamp's firmware outlives
many cloud deploys. So the device leg carries `RELAY_PROTO_VERSION`, the hub
accepts exactly the versions in `SUPPORTED_RELAY_PROTO_VERSIONS`, and a
board it refuses is told so by name.

Two protocols exist, and the hub accepts both:

| Protocol | Since | What it adds | Golden bytes |
|---|---|---|---|
| 1 | 2026-10-06, the first relay | tags `0x01`–`0x09` | `tests/relay_frame_golden.rs` |
| 2 | 2026-10-08, pictures through the cloud | the hello's firmware tail; `Project` (`0x0a`), `Picture` (`0x0b`), `PictureRate` (`0x0c`); the project tags | `tests/relay_frame_golden_v2.rs` |

Fielded cores speak protocol 1, so the hub accepts it forever and **never
sends a protocol 1 board a frame protocol 1 does not have**
(`RelayFrame::protocol`, `frame_protocol`): a protocol 1 board closes its
leg on any frame it does not know. A change to a frame's bytes, a reason
code or the proof is a new protocol, added beside the old ones; a golden
file is never edited to make a change pass, and protocol 1's is never
edited at all.

## Tests

```bash
cargo test -p lpc-relay
cargo check -p lpc-relay --target riscv32imac-unknown-none-elf
cargo check -p lpc-relay --target wasm32-unknown-unknown
```

`tests/relay_client_rules.rs` pins each of the client's rules (the module
doc of `relay_client/relay_client.rs` lists them).
