# trouble-host — LP fork

Vendored from crates.io **trouble-host 0.6.0** (upstream
<https://github.com/embassy-rs/trouble>, `host/` at commit
`29b0c515831e4be2bba5e55616bfa6297d75db31` per the crate's
`.cargo_vcs_info.json`; checksum
`1df7817cead4b83dfbeeaa59736ecbc97c30b21dc3bfc0cc2ce8a6687f70b37a` in
`Cargo.lock` before the patch), `Apache-2.0 OR MIT`; the crate ships no
license file, and LightPlayer takes it under Apache-2.0, whose text is
`licenses/Apache-2.0.txt`. Patched in through the root `Cargo.toml`'s
`[patch.crates-io]`; the version stays `0.6.0`. Verbatim except that the
packaging files (`.cargo-ok`, `.cargo_vcs_info.json`, `Cargo.toml.orig`) were
dropped, and the diff below.

## The diff

### 1. A link-layer parameter request is answered with the Reply alone (`src/connection.rs`, `ConnectionParamsRequest::accept`)

A central can ask a peripheral for connection parameters two ways: an L2CAP
signalling request, or the link layer's Connection Parameters Request
procedure, which the controller hands the host as an HCI **LE Remote
Connection Parameter Request** event and holds open until the host answers
with **LE Remote Connection Parameter Request Reply** or its Negative Reply.
Android uses the link layer on every connection (an interval of 7.5 ms, seen
from a Pixel 8a, 2026-10-17).

Upstream's `accept` (feature `connection-params-update`) sent an **LE
Connection Update** first in both cases and the Reply only after it
succeeded. While the link-layer procedure waits on the host, the C6's
controller refuses that update, so every Android request ended in `info!
("Connection parameters request procedure failed")` and a Negative Reply —
which bt-hci 0.8.1 then mis-parsed into a panic
(`third_party/bt-hci/README-LP.md`,
`docs/defects/2026-10-17-an-android-phone-connecting-over-bluetooth-panics-the-c6.md`).
The fork answers a link-layer request (`l2cap == false`) with the Reply alone;
the L2CAP path is unchanged.

### 2. The request's maximum interval is the central's (`src/host.rs`, RX runner)

The LE Remote Connection Parameter Request event's `interval_max` was read
into the request as `interval_min` again, so `accept(None, …)` answered with
the central's minimum for both. One field.

`grep -n "LP fork" -r src` finds both hunks. Upstream candidates.

## Re-syncing with upstream

Copy the new version out of the cargo registry, delete `.cargo-ok`,
`.cargo_vcs_info.json` and `Cargo.toml.orig`, then re-apply the hunks — or
drop the fork once upstream answers a link-layer request with the Reply.
