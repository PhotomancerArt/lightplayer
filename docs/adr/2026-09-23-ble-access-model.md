# ADR: Access over untrusted links — shared secrets, HMAC login, tiers by link

- **Status:** Proposed
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
- `WIRE_PROTO_VERSION` 20 → 21: new login and refusal messages and a
  required `auth` on the hello.
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

- M4 (the BLE link) is the first `Untrusted` transport: a link mux in
  `fw-esp32-common`, unauthenticated-connection timeout and count, and the
  `Identify` request if cheap.
- M6 (Studio) writes the device store and sidecars and runs the login.
- `docs/design/device-identity.md` §8 (the auth gap) is closed by this ADR;
  M7 updates it.

## Amendment 2026-09-24: who has access, on the board; Bluetooth on by default

Plan `lp2025/2026-09-24-1953-ble-easy-access` (P1). Yona's rule for it:
most of the time people should not have to think about access. A physical
connection is access, and Bluetooth should be one click, or none.
The whole decision, with Studio's half (key holders, the silent add over
USB, account keys, the rejected alternatives), is
`2026-09-24-easy-bluetooth-access.md`; this amendment is the board's half.

### Generated keys, one salt per holder

Most secrets are no longer typed. A browser generates one 32-byte random
secret and one 16-byte salt for itself. The signed-in account has one of
each too. Each holder installs the same `(salt, K)` on **every** device it
touches, with `iterations: 1`: a 256-bit random secret needs no stretching.
Typed passwords keep the client's PBKDF2 cost. The login protocol does not
change. A client recognises its own offers in a challenge by salt, so the
automatic unlock never has to guess and never trips the backoff.

**Accepted consequence.** A salt is visible before login, since every
challenge offers it. An observer in range can therefore tell that two
devices share a holder (the same browser or account). A flash dump of one
device yields that holder's `K`, and that `K` also opens the holder's
other devices. The threat model above (someone cheeky nearby) accepts
both. Per-device salts would have needed every browser to keep a record
for every device.

**Revisit — flagged for any security review (Yona, 2026-09-25).** Yona
accepts this for now but wants it revisited: "any security analysis would
flag that for sure. its an acceptable decision now, but we probably should
revisit it." The fix is to stop sharing one `(salt, K)` across devices. A
holder could derive a per-device key from its one secret, for example
`K_dev = HMAC(secret, device uid)` with a fresh salt per device, so one
dump opens one device. A client would then match offers against the key
for the device it is talking to, not by a salt shared everywhere. The cost
is that a phone must know which device it is unlocking before it answers.
That identity arrives in the hello (`/.lp/device.json`'s uid), so no
per-device record should be needed. It is a new entry shape and a wire
change, so it waits for its own plan.

### Access files v2

`SecretEntry` gains `kind` (`browser` | `account` | `password`) and an
optional `addedAt` (epoch seconds, supplied by the client; the board has no
wall clock). Both access files go to `version: 2`. The readers still accept
v1, where each entry reads as `kind: password` with no time, through a
private v1 shape that keeps `deny_unknown_fields`. Writers always write v2.
The board decides nothing by kind. It is there for people and for a client
recognising its own rows. Schemas regenerated.

### The board merges; no client rewrites the store

Before this amendment a client wrote the whole device store with an fs
write. A second browser's write would erase the first browser's key, and no
client could list what was installed. Four new requests fix both. All are
**edit** tier: the classifier names them, and the table test covers them in
all five link states.

- `AccessList` answers `AccessList { bleEnabled, open, entries }`. Each
  entry is `{ label, kind, tier, salt, addedAt? }` and **never** `k` or
  `iterations`. The salt is the entry's identity, and it is already public
  in every challenge.
- `AccessAdd { entry }` merges the entry into the store. An entry with the
  same salt replaces the old one, which is how a rename re-labels. A new
  entry past `MAX_SECRETS_PER_FILE` is refused.
- `AccessRemove { salt }` removes that entry. A salt that is not there is
  a no-op.
- `AccessSetSwitches { bleEnabled?, open? }` sets the switches. A
  `bleEnabled` change still applies at the next boot, and the client
  restarts the device.

Each change answers with the list as it now stands. The server does the
read-modify-write through its **base** fs (`access_store.rs`). The wire fs
path stays write-only for access files. Nothing here relaxes it: a raw fs
write is still possible over a trusted link, but no client needs one now.
Project sidecars are still honoured and are not listed.

### Bluetooth on by default

This replaces "Locked by default" above for the device store:

- A **missing** store is `DeviceAccessFile::fresh()`: Bluetooth on, not
  open, no secrets. The radio is up, but nothing gets past the gate until
  a key is added over USB (Studio adds its keys on a USB connect, P3 of
  the plan).
- A **damaged** store is still `locked()`: Bluetooth off. Damage still only
  takes access away.
- "Missing" is decided by `file_exists`, not by a read error. Not every fs
  reports a missing file as `NotFound`.

`WIRE_PROTO_VERSION` 23 → 25 (24 was taken by #785's palette pin first).
`PROJECT_FORMAT_VERSION` is untouched: the access files are their own
formats.
