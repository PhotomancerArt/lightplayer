# ADR: Uids are a single token — prefix + lowercase base-32 body, no separator

- **Status:** Accepted
- **Date:** 2026-08-07
- **Deciders:** Photomancer (Yona)
- **Supersedes:** None (amends the uid shape used since prefixed uids landed)
- **Superseded by:** None

## Context

`PrefixedUid` was `<prefix>_<body>`: a three-letter kind prefix (`prj`,
`mod`, `dev`, `usr`), an underscore, and a 16-character base-62 body
(~95 bits). Two problems, raised while the format could still change
freely (pre-lock-in — no outside users hold links or files yet):

1. **The underscore makes one identity read as two things**, and it
   spends the one natural delimiter. A composite key that *embeds* a
   uid (`backup_prj…_v2`) cannot split on `_` if the uid itself
   contains one.
2. **Base-62 is case-sensitive.** Uids live in URLs (`/p/<slug>-<uid>`
   is the share link, and the uid IS the link token, D24) and in
   filesystem paths (`/history/<uid>/`). Case-folding contexts —
   macOS's case-insensitive filesystem, humans reading a link aloud,
   case-mangling middleware — can alias or corrupt a case-sensitive
   id.

## Decision

The canonical uid form is **`<prefix><body>`** — one token, no
separator — where the body is exactly **16 characters of lowercase
Crockford base-32** (`0-9a-z` minus the confusables `i l o u`), e.g.
`prjhk7q9xy2mq4tb8wz`. 19 characters total, one shorter than before.

- **No separator is needed for exact parsing**: prefixes are a closed
  enum and the body length is fixed, so the split point is always
  known. `_` is now fully reserved as a delimiter for composite keys.
- **80 bits of keyspace** (16 × 5). Minting keeps the low 80 bits of
  the caller's 128 random bits — a power of two, so the old base-62
  modulo bias is gone. 80 bits is collision-safe at any plausible
  scale and remains an unguessable unlisted-link token behind a
  rate-limited server; 100+ bits was judged not worth the extra URL
  length.
- **The efuse-MAC embed survives unchanged**: `HardwareId::device_uid`
  packs a value < 2^56, and mint is the identity below 2^80 (was: the
  identity below 62^16). The derivation bytes (G1 contract) did not
  change — only the rendering of the derived uid.
- **Split-point rules changed where code anchored on `prj_`**: the
  share-path parsers (server `page::share_path`, web `router.rs`)
  now take the last `"prj".len() + UID_BODY_LEN` characters of the
  segment instead of `rfind("prj_")` — with no separator, `prj` can
  occur *inside* a body, so anchor-search is no longer sound.
  `ProjectLink`'s last-`-` split stays sound (a body never contains
  `-`), and `LibraryStore::resolve_key` now classifies uid-vs-slug by
  strict parse instead of prefix sniffing.

## Consequences

- Hard cutover, no legacy parse: old-format strings refuse with
  `BadLength`. Nothing shipped holds them — device-side identity is an
  opaque `String` (old stamped `/.lp/device.json` files still read;
  efuse-derived boards re-derive), and no committed project.json
  carries a uid.
- Dev-machine state minted under the old format (local device-registry
  rows, cloud dev DB rows) orphans and re-mints. The live
  lightplayer.app store should be wiped or re-minted at next deploy —
  alpha posture, no outside data.
- Uids are now safe as filenames on case-insensitive filesystems and
  survive being read aloud (no `0/o`, `1/l/i` ambiguity).

## Alternatives Considered

- **Keep `prj_` (Stripe convention).** Familiar, scannable — but keeps
  the underscore spent and the two-things reading.
- **A different separator (`prj0x…` etc.).** Any in-alphabet separator
  is no separator at all (parse is positional anyway), and `0x`
  falsely advertises hex.
- **100-bit body (20 chars).** Stronger link token, but 23-char uids
  in every URL; 80 bits already clears both the collision and the
  online-guessing bar.
- **Case-sensitive base-62 without a separator.** Shorter for the same
  bits, but keeps the case-folding hazard that motivated half of this
  change.

## Follow-ups

- Wipe/re-mint the lightplayer.app dev store on next deploy.
- The lineup of dated ADRs and design docs written before this one
  show the old `prj_…` style in prose; they are historical records and
  were left as written (this ADR is the format authority).
