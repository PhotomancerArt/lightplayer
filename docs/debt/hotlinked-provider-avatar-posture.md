---
status: carried
since: 2026-08-07
logged: 2026-08-07
area: lp-cloud-domain (CloudUser.picture_url) + lpa-studio-web avatar rendering
related:
  - docs/adr/2026-08-07-provider-based-auth.md
  - docs/debt/cloud-abuse-quota-posture.md
  - Planning/lp2025/2026-08-07-0936-cloud-login-account (Q5, P2/P3)
---
# Provider avatars are hotlinked with no verified posture at scale

**Shape.** `CloudUser.picture_url` (migration `0002`) stores whatever URL
the provider's userinfo endpoint reports, refreshed on every login (Q5:
deliberately live, unlike the sticky name fields). The client never
fetches or stores the image bytes — it renders the URL directly in an
`<img>` with an initials fallback. Nobody has verified, at any scale
beyond a handful of known users, three things this posture assumes:

- **URL expiry.** Google's profile photo URLs are not documented as
  permanent; a URL captured at login N could 404 or redirect by login
  N+1's next render (not next login — the render happens on every page
  load between logins too, from the cached `picture_url`).
- **Rate limits.** Every signed-in browser tab loads the image directly
  from Google's CDN on every render of the chrome/dropdown/account page.
  No caching, no proxy, no `Cache-Control` policy set by us — at crew
  scale this is noise; at any real user count it is an unbounded number
  of browsers hot-linking a Google-owned URL with no coordination.
- **Privacy.** A hotlinked image is a live request to Google's servers
  every time it renders, which tells Google when and how often a
  LightPlayer user's chrome is on screen (referrer-bearing or not,
  depending on browser/extension state we do not control). Nobody has
  weighed whether that leak matters for this product's users.

**Why it stands.** Ruled at planning (Q5, "lgtm"): hotlink + never store
bytes was the deliberate v1 posture, chosen over a thumbnail-fetch-and-
cache pipeline that is real infrastructure (storage, refresh policy,
provider ToS review) for a cosmetic feature, at a scale (personally-known
users) where none of the three risks above have ever fired.

**Trigger to fix.** Before any public announcement of accounts/login, or
the first sign that Google photo URLs are expiring/erroring in practice
(a broken-image icon report is the tell). Cheap first step when
triggered: fetch-once-and-cache-in-blob-store behind the existing content-
addressed blob plane (`PUT /b/{hash}`), keyed by provider+subject+fetch-
time, with a TTL-driven refresh — this reuses infrastructure that already
exists rather than inventing a new avatar-storage concept.

**Incident log.** (none yet)
