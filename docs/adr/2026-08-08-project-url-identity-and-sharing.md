# Project URL identity and sharing: one address, Google-Docs-lite access

- Status: accepted
- Date: 2026-08-08
- Plan: lp2025/2026-08-07-1630-project-identity-sharing

## Context

The cloud service shipped (`2026-08-06-cloud-service-architecture.md`)
with share URLs declared a forever contract, but the sharing product on
top of them was unshaped: `Visibility{Private,Link}`, read-only links, a
manual publish call, and no UI. A vision round settled the product
identity of a project's URL and its access model; this ADR records the
load-bearing choices as implemented (PR #388).

## Decisions

**One URL is the project's identity — and the address bar is the share
link.** A project lives at `lightplayer.app/p/<slug>-<uid>` (e.g.
`/p/radiance-dome-prjhk7q9xy2mq4tb8wz`). Opening a project in Studio
puts you at that URL whether you own it or not — the canonical URL loads
the project **in the simulator**, and the old `/sim/` route retired with
no shim (the sim has no identity of its own; the project loads the sim;
multiple sim instances of one project is a deliberately uncrossed
bridge). Device routes are unaffected — a device has its own `dev_`
identity. Sharing is copying the address bar; the Share popover is
access control only. The uid is authoritative and immutable; the slug is
cosmetic, generated from the display name, and changes freely with it.
Old links never break: routing extracts the uid, ignores the slug, and
canonicalizes via `history.replaceState`.

**The forever contract is uid-extractability, nothing else.** What we
commit to for the life of the product is: the project uid appears in the
URL and is mechanically extractable. Slug placement, the separator, even
the `/p/` prefix are revisable later behind canonicalization, because
the server owns the origin and the uid resolves the project. This is
what makes the scheme low-regret to promise forever.

**No publish step.** Projects created while signed in publish at
creation; every save pushes. Internally the system stays local-first
(the OPFS library is the source of truth, offline works); the *product
posture* is web-app — created things exist at their URL without
ceremony. Projects created signed-out are local-only and upload/
auto-assign to the account at sign-in.

**Access is two orthogonal dimensions, replacing `Visibility`.**
*General access* — what the link grants: `none` (restricted) | `view` |
`edit`. *People* — per-user grants by email (pending-invite machinery
resolves at first sign-in): roles `owner`/`editor` now, `viewer` later.
Default general access at creation is **view**: every project is
anyone-with-link-viewable from birth. The 80-bit uid is the protection —
URLs are not guessable, so possession of the link implies it was shared
with you. Revocation is flipping general access to `none` (Google Docs
model; no rotatable tokens). When general access is `edit`, the uid is
the capability and **anonymous push is legal** — `push_commit`,
`have_blobs`, and the content-plane `PUT /b/{hash}` / `PUT /t/{hash}`
all answer anonymous callers (writes stay hash-verified and idempotent;
the abuse surface is owned by `docs/debt/cloud-abuse-quota-posture.md`,
whose quotas-before-announcement trigger this sharpens). The member
list travels only to members — an edit-link stranger can push but never
reads the email roster.

**Read-only is enforced at push, not in the editor.** Opening a share
link pulls a uid-preserving tracking copy into the local library and
opens the full editor — no locked viewer mode, no per-operation gating.
A two-state banner is the whole read-only UX: pristine copy → "viewing —
updates arrive as they happen" (fast-forwards auto-apply with a toast,
never over a dirty session overlay); locally edited → "updates are
paused — fork to keep your version" (fork = new uid, new URL, origin
recorded). A refused push latches — the client retries only after a
later pull observes changed access, never on a timer.

**URLs are case-insensitive end to end.** Uids are case-insensitive by
format (`2026-08-07-uid-format-single-token-base32.md`); slugs are
generated lowercase; the one shared parser (`lpc_cloud_api::share_link`,
used identically by the server page plane, the Studio router, and
`ProjectLink`) folds case, trims trailing sentence junk, splits on the
last `-`, and validates prefix + body. A link survives the case-mangling
chat apps inflict. Slug generation handles names as people write them
(`Yona's "radiance dome" Doors` → `yonas-radiance-dome-doors`); slugs
are deliberately NOT unique — the uid is the key.

**Archive, not delete.** Auto-publish means every experiment mints a
cloud row, so removal is required vocabulary: `archive` is reversible
(owner-only; the link stops resolving for visitors, members still read,
writes refuse) and `restore` undoes it. Hard delete deliberately does
not exist yet; when it arrives it lives only inside the archive
(no-irreversible-actions principle).

## Alternatives considered

Each of these was genuinely considered — independently over years of
URL-scheme scar tissue, and re-derived in the vision round — and
rejected for the same reasons:

**Owned-name URLs (GitHub-style `/<user>/<project>`).** The prettiest
scheme, rejected on three counts. It requires a unique-name registry,
and name registries accrete squatting, land-grabs, and support burden.
It couples the URL to an account — poison for anonymous creation,
ownership claims, and transfers, all of which this product wants. And
rename becomes redirect infrastructure with tombstoned names (GitHub
maintains exactly this machinery); under slug-plus-uid, rename is free
forever.

**Bare-uid URLs (Google-Docs-style `/d/<opaque-id>`).** Operationally
identical to the chosen scheme and strictly worse: the URL carries no
human scent. The slug costs nothing (routing ignores it) and pays off in
link unfurls, browser history, search, and the moment someone reads a
link aloud. URLs are product surface.

**Two-segment `/p/<uid>/<slug>` (StackOverflow/Amazon-style).** The one
serious rival: positional parsing needs no split rule and the slug could
contain anything. Rejected because it puts the machine part first, reads
as hierarchy where none exists, and its parsing advantage evaporated
once the split rule became trivial (last `-` + validated prefix). The
chosen form reads as a *name*; this one reads as a database row with a
comment.

**Slug-with-marker (`<slug>-prj_<uid>`, the earlier form).** Superseded,
not rejected: the uid format rework removed the underscore from uid
spelling, which collapses the special `prj_`-marker split rule into the
ordinary last-`-` rule. Same design, simpler grammar.

The chosen single-segment slug+uid shape is the Notion/Medium form —
convergent evolution from products with the same posture (no publish
step, rename-safe, link-as-capability) — with a validated prefix and
fixed body length making extraction unambiguous.

## Consequences

- `CLOUD_API_VERSION` is 3 (version-and-refuse; the version check runs
  before request decode so a v2 body answers `VersionMismatch`, not a
  400). `Visibility`/`SetVisibility` are gone; `Access`, `SetAccess`,
  `ArchiveProject`/`RestoreProject`, and members-in-`ProjectInfo` are
  the vocabulary. Migration 0003 carries the schema.
- Anonymous *cloud creation* is deferred (slice 2): owner `Anonymous`,
  general access locked to `edit`, `ClaimProject` on sign-in.
  `projects.owner_uid` keeps `NOT NULL` until then.
- The client sidecar (`SidecarMeta`) is produced at publish/push with
  the real name and format version; `preview_png` is deferred —
  `docs/debt/sidecar-preview-capture.md` (OG cards render title-only
  until the frame-capture work lands).
- The 2026-08-06 architecture ADR's URL notation (`/p/<slug>-prj_<uid>`)
  is amended to the underscore-free spelling; its "share URLs are
  forever" clause narrows to precisely the uid-extractability contract
  stated here.
- The share-envelope ADR (2026-07-28) is unaffected: envelopes remain
  the anonymous copy-semantics path; URLs are the identity-preserving
  path.
