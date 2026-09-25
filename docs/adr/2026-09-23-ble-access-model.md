# ADR: Access over untrusted links — shared secrets, HMAC login, tiers by link

- **Status:** Accepted (shipped in PR #794, 2026-09-24; see Amendments)
- **Date:** 2026-09-23
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

A LightPlayer device has only ever had one link: the USB cable. Whoever
holds the cable holds the device, and the server never had to ask who sent
a message — `ClientMessage { id, msg }` carried no connection identity and
`lpa-server` kept no per-client state.

BLE remote control (plan `lp2025/2026-09-23-1428-ble-remote-control`) puts
a second link on the device that anyone within radio range can open. The
product need is small — turn the knobs of a choker from a phone, and let a
friend do the same without being able to rewrite the show — but "the board
enforces tiers, not Studio" is not negotiable: a phone running a modified
Studio must get no further than a phone running the real one.

Two constraints shape everything below. The device is an ESP32-C6 with a
tight flash budget and no appetite for a deliberately slow KDF on every
login. And the threat model is someone cheeky within range, not a cracker
with the flash chip on a bench: low-security passwords are acceptable.

## Decision

### Access, not ownership: a list of labelled secrets, each with a tier

A device accepts a short list of **shared secrets**, each with a label
(`"camp"`, `"mine"`) and a **tier**:

- **play** — the panel (`PanelWrite`/`PanelClear`) plus reads: project
  reads, the project's overlay and inventory (`ReadOverlay`,
  `ReadInventory`), project listings, and read-only fs inside the projects
  directory;
- **edit** — everything. Edit implies play.

There is no owner and no account on the device. Whoever knows a password
has that password's tier.

### The split: the client derives, the board only MACs

A secret is stored as the **output** of the KDF:

```
K = PBKDF2-HMAC-SHA256(password, salt, iterations)      — client only
stored:  { label, tier, salt (16 B), iterations, k: K (32 B) }
```

Login is two messages:

1. `LoginBegin` → the board answers `LoginChallenge { nonce, offers }`: a
   fresh 32-byte nonce from caller-supplied randomness, and every installed
   secret's `(salt, iterations)` — **without labels**, so labels stay
   private before login.
2. `LoginAnswer { macs }` — the client does not know which secret its
   password belongs to, so it derives `K_i` for every offer and answers
   `HMAC-SHA256(K_i, nonce)` for each, in order. The board checks every
   entry with a constant-time compare and no early exit, grants the
   **highest** tier among those that verify, and names that secret's label
   (`LoginResult::Granted { tier, label }`).

The password never crosses the air and is never stored. `K` is
login-equivalent, which is accepted — and is exactly why no link, at any
tier, can read an access file back (below).

HMAC-SHA256 (RFC 2104) and PBKDF2-HMAC-SHA256 (RFC 8018) are small custom
code over the workspace `sha2` in `lp-core/lpc-access`, checked against
RFC 4231, the published PBKDF2 vectors, and RustCrypto's `hmac`/`pbkdf2`
as **dev-dependency oracles only** — the repo's "build tiny custom, a
reference library is a spec" rule.

### One login in flight, backoff per device

Only one challenge is outstanding per device: a second `LoginBegin` from
any link is refused (`Refused { retry_after_ms }`) until the challenge is
answered, expires (30 s, on caller-supplied time), or its link closes. Only
the link that began a login may answer it. A challenge is single-use.

Failures back off per **device**, not per connection: three free attempts,
then 2 s, 4 s, … capped at 60 s, and a device in backoff refuses to begin.
Reconnecting does not reset it. It lives in RAM; a reboot resets it, which
the threat model accepts.

### Trust is a property of the link

`ServerTransport` now yields `Incoming { link, trust, msg }` and `send`
names the `LinkId` a reply is for. **Trust is set by the transport** — the
firmware knows whether bytes came off the USB cable or the radio — and
never by a message:

- `Trusted` (USB, host, browser worker, emulator): holds **edit** without a
  login. Physical possession is the recovery path for a board whose
  passwords are lost.
- `Untrusted` (BLE; WiFi later): holds its login's grant; failing that,
  **play** if the device is `open`; failing that, nothing.

A grant belongs to its link and ends when the transport reports the link
closed. Multi-link transports mint link ids monotonically and never reuse
one, so a new connection can never inherit an old one's grant.

### The gate: an exhaustive classifier

At the top of `LpServer::tick_and_send`, before anything is answered,
`classify(&ClientRequest)` maps every request to `Public | Play | Edit`. It
is an exhaustive `match` over `ClientRequest`, `WireProjectCommand` and
`FsRequest` with **no wildcard arm**: a new wire variant does not compile
until someone classifies it. A request its link's tier does not cover is
answered `NotPermitted { needs }` — a reply, never a dropped frame, so a
client can say "log in with an edit password". Before login only `Hello`,
`LoginBegin` and `LoginAnswer` are answered.

The hello carries `auth { required, granted }` computed for the link it is
sent on, and a heartbeat to a link that holds no tier carries nothing the
hello does not.

### The access files

Installed secrets = **account default ∪ project sidecar ∪ device-only**,
from two persisted formats, each its own `version: 1` with a schema in
`schemas/`:

- **The project sidecar**, `<project>/.lp/access.json`
  (`ProjectAccessFile`). It lives in `/.lp/`, which is outside the
  canonical content hash — so changing a secret never changes a project's
  identity (bind-by-hash keeps working), and cloud push, publish and fork
  skip it with the rest of `/.lp/`. Zip export and the pasted package
  envelope skip it by name. A device **push** carries it: that is how a
  project's secrets reach a board.
- **The device store**, root `/.lp/access.json` (`DeviceAccessFile`): the
  device-only secrets, the account default (one more secret, labelled by
  Studio), `bleEnabled`, and `open`. **Locked by default**: a missing or
  unreadable store means BLE off, not open, no secrets — damage only ever
  takes access away. Enabling BLE is an edit-tier write Studio makes over
  USB. `open` ("open, no password") is explicit, and grants play, never
  edit.

**Write-only, on every link.** Beneath the tier check and applying to the
trusted link too, the fs handlers never return an access file's bytes: a
read is refused, a listing may name the file, a changes-since walk skips
it, and a package hash that would cover one is refused (a hash of the bytes
is an offline oracle on the key). Writes and deletes are edit-tier. The
consequence, chosen on purpose: **a pull cannot bring a sidecar back**, and
the library copy is the source. The server reads the files itself, through
its own fs, never through the wire path.

**Nothing else reads them either — not a loaded project.** The wire path is
one door; a project's runtime is the other. A project (shared, catalog or
authored) naming `.lp/access.json` as a shader source or any other resource
would have the engine read the sidecar, and the shader compiler's parse
error quotes the source it choked on — label, salt and `k` — into the
node's status, which a play-tier `ProjectRead` returns. So every loaded
project's filesystem is wrapped in `AccessGuardedFs`, which refuses a read
of any access-file path: such a project fails to load (or its asset reports
the refusal), and no byte reaches the engine, the registry, the inventory
or the overlay's base-value parse. On the board the complete list of readers is
therefore two, both the server's own and both through the **base** fs: the
login's installed-secrets assembly and the device-store read behind
`open`. `lpa-server/tests/access_file_resource.rs` pins it,
with a control that shows the same bytes under another name do come out.

### Sans-IO

`lpc-access` reads no clock, draws no random numbers and touches no
filesystem. The server's clock is the sum of the frame deltas its embedder
hands `tick_and_send`; its randomness is an injected entropy source (the
C6 wires its hardware RNG). A server with no source refuses to log in
rather than mint predictable challenges.

### WiFi reuses the model

Nothing here is BLE-specific. A future WiFi (or any radio) link is one more
`Untrusted` link on the same transport seam, gated by the same classifier,
logging in with the same messages against the same access files.

## Consequences

- The board, not Studio, enforces tiers; a modified client gets exactly
  what its link's tier allows.
- Every `ServerTransport` implementer changed shape (link-tagged receive,
  link-addressed send). Single-link transports report one trusted link and
  behave exactly as before.
- `WIRE_PROTO_VERSION` bumped for new login and refusal messages and a
  required `auth` on the hello. Written as 20 → 21; it shipped as **21 → 22**,
  because lean-wire (#791) landed first and took 21 (the plan's merge-order
  rule: whoever lands second bumps again).
- A new wire request cannot ship unclassified: the compiler refuses it, and
  the host table test (`lpa-server/tests/access_gate.rs`) enumerates every
  variant from serde's own variant lists against five link states.
- Two new persisted formats. A serde change to either is a format change:
  bump its `VERSION` and ship the reader for the old one with it.
- A lost password with no edit secret left is recovered over USB, which
  always holds edit.
- Board cost: an HMAC wrapper over the already-linked `sha2`, the gate, and
  the login state — no KDF on the device.

## Alternatives Considered

- **Ownership / accounts on the device (rejected, D10).** A device "owned"
  by one account makes lending a choker to a friend, or running a camp
  booth, a transfer ceremony. Shared secrets with tiers express "you may
  turn the knobs" directly.
- **Secrets in `project.json` (rejected, D13).** `project.json` is hashed
  content: every secret change would change the project's identity, break
  bind-by-hash, and ride cloud publish and forks to strangers. `/.lp/` is
  already the non-content sidecar namespace.
- **An on-device KDF (rejected).** Storing a password hash and running
  PBKDF2 on the C6 per login costs flash and a long stall on a chip with no
  crypto headroom to spare, and buys nothing: whatever the board stores is
  login-equivalent either way, and the KDF's purpose — making a stolen
  store expensive to crack — is served just as well when the client runs it
  once at install time.
- **Labels in the challenge.** Simpler for the client (one KDF instead of
  one per offer) but tells anyone in range what the secrets are called.
  With a handful of secrets, N KDFs on a phone is affordable.

## Follow-ups

- ~~M4 (the BLE link) is the first `Untrusted` transport~~ — shipped in PR
  #810: the link mux in `fw-esp32-common`, a 10 s unauthenticated-connection
  drop and at most two connections
  (`docs/adr/2026-09-24-ble-transport.md`). `Identify` was **not** built
  (below).
- M6 (Studio) writes the device store and sidecars and runs the login.
- ~~`docs/design/device-identity.md` §8 (the auth gap)~~ — updated in M7:
  closed by this ADR.

## Amendments

### 2026-09-24 (M7): what shipped, checked against the text above

- **Wire version.** See Consequences: the login bump is proto 22, not 21.
- **`Identify` is not built** (plan DD22). It needs a new `ClientRequest`
  (so a wire bump), a per-device rate limit and an output-stage hook in the
  engine. When it lands it is classified `Play`, like the panel. Until then
  a wrong-device connect is caught by the advertised name
  (`LP-<project name>`, or the last four MAC hex digits), not by a blink.
- **Enabling BLE takes a reboot** (plan DD23). The firmware reads
  `bleEnabled` from the device store once, at boot
  (`docs/adr/2026-09-24-ble-transport.md`, decision 4), so the edit-tier write
  that enables it is not enough on its own. The write-only and locked-by-default
  rules above are unchanged by it.
- **Two connections allowed, one gates** (plan DD12). The transport admits two
  BLE links, and each holds its own grant, but nothing in this slice was
  tested or gated on the second one. The 2-connection heap figure has never
  been measured on the product image.
- The tiers, the classifier, the write-only rule and `AccessGuardedFs` shipped
  as written; `lpa-server/tests/access_gate.rs` and
  `tests/access_file_resource.rs` are the proof. The end-to-end refusal was
  seen on silicon from a desktop central (Run J in the plan's
  `spike-results.md`: a locked XIAO C6 answered `listLoadedProjects` with
  `notPermitted { needs: play }` before login, from Mac Chrome 153 over CDP),
  **not yet from a phone** — that is the plan's G4 walk.
