# lpc-access

The access core for links a device does not physically trust (BLE first,
WiFi later): shared secrets, tiers, the HMAC login, and the backoff that
makes guessing slow. `no_std` + `alloc`, sans-IO — no clock, no RNG, no
filesystem, no notion of a link. `lpa-server` owns links and trust and asks
this crate for verdicts.

**Threat model, in two lines:** someone cheeky within radio range, not a
cracker with the flash chip. Low-security passwords are acceptable; the
password never crosses the air and is never stored, and the stored key is
never readable back over any link.

## What is here

| File | Concept |
|---|---|
| `hmac_sha256.rs` | HMAC-SHA256 from RFC 2104 — the only primitive the board runs |
| `pbkdf2_sha256.rs` | PBKDF2-HMAC-SHA256 from RFC 8018 — **client side only** |
| `constant_time_eq.rs` | MAC comparison with no early exit |
| `tier.rs` | `Tier { Play, Edit }`; edit implies play |
| `secret_entry.rs` | `SecretEntry { label, kind, tier, salt, iterations, k, addedAt? }` |
| `secret_kind.rs` | `SecretKind { Browser, Account, Password }` — who holds a secret |
| `project_access_file.rs` | `<project>/.lp/access.json`, `version: 2` (v1 still reads) |
| `device_access_file.rs` | root `/.lp/access.json`, `version: 2` (v1 still reads); a missing store is `fresh()` (Bluetooth on, locked), a damaged one `locked()`; merge by salt |
| `access_file_path.rs` | which paths are access files (the fs gate's predicate) |
| `login_state.rs` | begin → challenge → answer → verdict; one login in flight |
| `rate_limit.rs` | per-device backoff: 3 free, then 2 s doubling to 60 s |

Both access files are persisted formats with schemas under `schemas/`
(`project-access.schema.json`, `device-access.schema.json`). A change to
their serde shape is a format change: bump the file's `VERSION`, and ship the
reader for the old one with it.

RustCrypto's `hmac` and `pbkdf2` crates are dev-dependency **oracles** only;
the tests also carry RFC 4231 and the published PBKDF2-SHA256 vectors.

Decision record: [`docs/adr/2026-09-23-ble-access-model.md`](../../docs/adr/2026-09-23-ble-access-model.md).
