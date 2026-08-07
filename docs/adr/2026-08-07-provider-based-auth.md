# ADR: Provider-based auth — connections and methods, not Google-shaped code

- **Status:** Accepted
- **Date:** 2026-08-07
- **Deciders:** Photomancer
- **Supersedes:** None (amends `2026-08-06-cloud-service-architecture`'s
  "auth is Google-only" posture)
- **Superseded by:** None

## Context

`lp-cloud`'s first auth slice (2026-08-06) hard-coded Google: the server
had one OAuth handler, the (nonexistent) client had nowhere to render a
choice, and "sign in" meant "sign in with Google" at every call site. The
login/account UI round (`spikes/cloud-login/index.html`, PR #379,
gate-judged 2026-08-07) needed to add a signed-out chrome affordance, an
identity dropdown, and an `/account` page — and Yona added a constraint
while judging the spike: **auth stays generic and provider-based** — prod
is Google, local dev is a passwordless pick-a-profile picker, and a future
self-host deployment needs a local password store, none of which should
require a client code fork.

The reference shape came from exploring skybridge
(`skybridgeskills-monorepo`), whose auth-connections model was carried
over in full (spike §4):

- Connection **kinds** are `local | oidc`. Password and pick-a-profile are
  *methods of the local connection*, not siblings of Google — a self-host
  password login is not "a third provider next to Google and dev picker",
  it is the local connection's other method.
- A public **login-options** discovery call: the server reports which
  connections/methods exist; the client renders sign-in from that answer.
  Misconfigured or disabled connections are listed disabled, not hidden
  (this repo's dev picker instead omits itself entirely when off, since
  there is no "disabled, but visible" state worth showing pre-launch).
- The dev self-asserted connection is triple-gated: the flag has to be
  set, the origin has to be localhost, and the route re-checks both —
  never host-sniffed. LP's `/auth/dev` (`dev_auth_allowed`,
  `lp-cloud-server/src/auth/dev_auth.rs`) already had this shape; the
  round kept it rather than rebuilding it.
- Sessions are provider-agnostic: every connection converges on one
  mint. LP already had this true before this round — `dev_auth` and
  `google_auth` both called `CloudService::open_session` — this round
  just made it the load-bearing architectural fact rather than a
  coincidence of two handlers sharing a helper.
- An identities link table `(connection, subject) → user` would enable
  multi-provider accounts on one user later. Not built this round — LP
  keeps the single `google_sub` column on `CloudUser` — but no further
  provider-specific columns were added that would fight adding that table
  later (see "Names and pictures" below).

## Decision

**Connection kinds `local | oidc`; password and pick-a-profile are
methods of `local`.** `lp-cloud-domain::LoginProviders` carries `oidc:
Vec<OidcConnection>` (today: one row, Google) and `dev_picker:
Option<DevPickerConnection>` (present only when `LP_CLOUD_DEV_AUTH` is on
and the origin is localhost). The dev picker is deliberately modeled as
the `local` connection's dev-only method, not a third OIDC-shaped
provider — a future self-host password method is the same connection's
other method, addable without touching the `oidc` branch of anything.

**`LoginOptionsInfo` is the discovery contract; the client renders from
it.** `LoginOptions` (a vocabulary call, not a plain HTTP `GET` — Q1)
answers `{ oidc: [OidcOption], dev_picker: Option<DevPickerOptions> }`.
Each `OidcOption` is `{ id, label, start_path }` — a config row, not a
provider name baked into a match arm. `DevPickerOptions` additionally
carries `choices: [DevChoice]`, queried live from the store (today's
seeded accounts), not from static config, so the picker always reflects
who has actually signed in. The Studio chrome's sign-in affordance is
built entirely from this answer: one `oidc` entry with no dev picker
links straight to `start_path` (`/auth/google?next=<path>`); more than
one option, or a present dev picker, opens a popover listing every
choice. Adding a second OIDC provider or turning on a self-host password
method changes zero client code — only what `LoginOptionsInfo` reports.

**Sessions are provider-agnostic; the cookie never records the
provider.** `CloudService::open_session` is the one session mint every
auth path calls — `dev_auth` and `google_auth` (and any future local
password path) all converge here. The session cookie (`lp_session`,
hashed-token row) carries no connection/method field; a session looks
identical regardless of how it was minted. This is what makes "switch
account" a re-auth-and-replace rather than a provider-aware operation:
the client does not need to remember, and the server does not need to
report, which door a session came through.

**Lean multi-account: switch = re-auth, one active session, client
memory.** (G4 ruling, spike §5.) Switching accounts opens the provider's
own picker again — Google's `prompt=select_account` in prod, the dev
picker locally — and the new session cookie simply replaces the old one.
There is exactly one active session at a time; the identity dropdown's
"switch to…" list is `lp_accounts`, a client-side `localStorage` record
of past sign-ins (names and photo URLs only, never a token — Q8). Two
alternatives were considered and rejected:

- **Client token vault** (holding multiple live session tokens in the
  browser to switch without a round trip): rejected for the XSS blast
  radius — an HttpOnly cookie is not readable by injected script, a
  vault necessarily would be.
- **Server session-set** (the server tracking several concurrently-valid
  sessions per browser and switching between them without a re-auth
  round trip): deferred, not rejected. It is real complexity (a
  session-set concept, a "which one is active" selector, more surface on
  every session-touching handler) for a UX win — skipping the picker
  round trip — that is only worth it if switching turns out to be
  frequent. Listed in the Deferred Decisions index
  (`docs/adr/README.md`) with its trigger below.

**Names seed at creation; pictures refresh every login; image bytes are
never stored.** (Q4/Q5.) `given_name`/`family_name` are written once,
from whatever the provider reports at *account creation* — a later
login's provider profile never overwrites a name the user (or an earlier
login) already set, so a rename made on `/account` is permanent against
the provider re-asserting its own value. `display_name` is derived from
`given + family` when both are present, falling back to the historical
bare field otherwise (`CloudUser::display_name`,
`cloud_service.rs::me_info`, the one place this derivation happens so
`GetMe`/`UpdateMe` cannot disagree on it). `picture_url` is the opposite:
mirrored from the provider on *every* login, because a stale avatar is a
worse failure mode than an occasionally-changing one. The URL is
hotlinked — Google's CDN, not ours — and the bytes are never fetched or
stored; the client falls back to initials when there is no URL or the
hotlink fails to load. This asymmetry (names sticky, picture live) is
deliberate, not an oversight: names are identity the user owns on
LightPlayer once created, pictures are borrowed decoration.

## Consequences

- `CloudUser` carries `given_name: Option<String>`, `family_name:
  Option<String>`, `picture_url: Option<String>` alongside the existing
  `google_sub` (migration `0002_profile_and_sessions.sql`). No
  connection/method column was added to sessions or users — the
  identities link table skybridge's model calls for stays a later
  addition, not precluded by anything shipped here.
- Adding a second OIDC provider (e.g. GitHub) is a `LoginProviders`
  config change plus a second OAuth handler that also calls
  `open_session` — no client change, because the client already renders
  from `LoginOptionsInfo`.
- The self-host local password method (methods of the `local` connection,
  alongside the dev picker) is not built this round and must not be
  precluded; nothing shipped here blocks it — see the Deferred Decisions
  entry below.
- `docs/debt/hotlinked-provider-avatar-posture.md` records the accepted
  gap: avatar URLs are Google's, un-cached, with no verified posture at
  scale (expiry, rate limits, privacy of a hotlink revealing viewing
  activity to Google).

## Alternatives Considered

- **Provider enum as a client-side match** (`match provider { Google =>
  …, DevPicker => … }`): the shape the spike explicitly rejected — every
  new provider or method would be a client code change instead of a
  server config change. `LoginOptionsInfo` exists to prevent this.
- **A `provider` field on the session cookie/row**: would have let the
  client show "you're signed in via Google" without a round trip, but
  makes the session model provider-aware for a cosmetic win, and every
  session-consuming handler would carry the field whether or not it
  cared. Rejected; `GetMe`'s `provider_label` (derived from `google_sub`
  presence today) covers the one place this actually matters to the UI.
- **Client token vault for instant account switching**: see "Lean
  multi-account" above — rejected for the XSS blast radius.
- **Server session-set for instant account switching**: see "Lean
  multi-account" above — deferred, not rejected; the ~1 round-trip re-auth
  cost was walked and ruled acceptable at G1 (2026-08-07, question 4).

## Follow-ups

- **Server session-set switching** — build only if lean switching (re-auth
  per account change) proves demonstrably hot in practice; the cost
  otherwise is one more `?next=`-carrying round trip through the
  provider's picker. *Revisit when:* switch-account usage or complaints
  show the round trip is the friction point, not a hypothetical one.
- **Local password method (self-host)** — the `local` connection's
  password method, sibling to the dev picker, for deployments that are
  not "trust Google" shaped. Not built, must not be precluded; the
  `local | oidc` connection-kind split and the `LoginOptionsInfo`
  discovery contract are exactly the seam it needs. *Revisit when:* a
  self-host deployment target is prioritized.
- **Identities link table** `(connection, subject) → user` for
  multi-provider accounts on one user (sign in with either Google or a
  local password to the same account). Not needed while every account
  has exactly one `google_sub`. *Revisit when:* a second connection type
  ships and accounts need to merge across them.
- Added to the Deferred Decisions index (`docs/adr/README.md`).
