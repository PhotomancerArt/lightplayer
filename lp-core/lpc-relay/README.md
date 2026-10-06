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
| `relay_frame` | `RelayFrame` and its codec: one byte of tag, fixed little-endian fields, length-checked, ≤ `MAX_RELAY_FRAME` (2 KiB). The table of tags is the module doc. |
| `relay_hello` | `RelayHello`: the board's MAC, name, wire version, LAN address and account salts. |
| `relay_proof` | `relay_auth_key(K) = HMAC(K, "lp-relay auth/1")`, `relay_proof(A, nonce, mac) = HMAC(A, nonce ‖ mac)`, and the hub's constant-time check. |
| `relay_version` | `RELAY_PROTO_VERSION` and `check_relay_version`: version-and-refuse. |
| `refuse_reason`, `route_close_reason` | The one-byte reason codes. |
| `relay_limits` | Frame size, accounts per hello, routes per board, ping and silence intervals. |
| `relay_client` | `RelayClient`: the board's state machine — when to dial, backoff, the challenge, the route table, the status. |

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
board it refuses is told so by name. Bump the version on any change to a
frame's bytes, a reason code or the proof; `tests/relay_frame_golden.rs`
holds the bytes and must never be edited to make a change pass.

## Tests

```bash
cargo test -p lpc-relay
cargo check -p lpc-relay --target riscv32imac-unknown-none-elf
cargo check -p lpc-relay --target wasm32-unknown-unknown
```

`tests/relay_client_rules.rs` pins each of the client's rules (the module
doc of `relay_client/relay_client.rs` lists them).
