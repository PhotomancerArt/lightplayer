# ADR: Viewing is stateless; fork at explicit save

- **Status:** Accepted (refined 2026-08-30 — see the Decision section)
- **Date:** 2026-08-28
- **Deciders:** Photomancer
- **Refined 2026-08-30** (`2026-08-30-relationship-control-three-segment-bar.md`):
  fork is still the explicit save; it is no longer gated on the overlay
  being dirty, so a pristine example can be kept without editing it first.
- **Plan:** lp2025/2026-08-28-1459-examples-url-handling (PR #461)
- **Supersedes:** the seed-on-click example model — the 2026-07-06
  studio-project-management plan's `06-examples-place.md` deferred this
  exact replacement as "remix-on-first-edit / ephemeral playground" —
  and the tracking-copy-on-open half of D17's share-open flow for
  **View-access** links; D17's uid-preservation principle itself
  survives (the uid still names the cloud document; local copies exist
  only for members/Edit links and explicit forks). Q4 ruling: existing
  seeded/tracking copies in user libraries are left alone — ordinary
  self-sufficient packages whose provenance simply stops being
  consulted; dead `/p/<slug>-<uid>` links to never-published projects
  keep landing on the calm not-found line.
- **Superseded by:** None

## Context

Beta demos exposed the seed-on-click model as broken three ways
compounding. Clicking an example installed a copy into that browser's
OPFS library and minted a fresh `prj…` uid **at install time**, before
any edit — every viewer accumulated library data for things they merely
looked at. The address bar then healed to `/p/<slug>-<uid>`, so a person
who had only looked held a URL naming a project that existed nowhere but
their browser: a beta user shared `/p/plasma-duo-prj…` and nobody else
could open it, and the seed-once dedupe (`find_seeded_from`) made the
failure unreproducible from any other browser. Opening someone else's
View-access share link did the same through a tracking copy
(`install_synced`). Meanwhile the card copy already promised the model
we didn't have: "it becomes yours on first save."

## Decision

**An example is a first-party published project, and viewing any
View-access project — example or someone's share link — is stateless.**
One model, two entry points:

- **Transient view sessions.** Opening `/p/<slug>` (an embedded
  example's canonical bare-slug address) or a View-access share link
  runs the project through the ordinary open funnel over **memory-backed
  stores** (`OpenedProject` with `LpFsMemory` package/history, a
  nothing-to-undo receipt): full editor — save/dirty/history — with no
  catalog transaction, no OPFS write, no library entry, no URL change.
  Panel twiddling is play, not authorship; navigating away with unsaved
  transient edits gets the ordinary unsaved-work prompt and nothing
  more.
- **The session uid is minted at open, in RAM, from platform entropy,
  and written into the memory manifest.** It is never persisted and
  never shown while the session stays transient — but because it is in
  the manifest, the runtime copy and the memory copy can never disagree
  about identity, and the fork can install the files **verbatim**. It
  must be real entropy because of what happens next.
- **The explicit save gesture is the fork — the identity moment.** On a
  transient session, save commits as usual (overlay commit, save-as-pull
  into the memory copy), then installs a copy into the real library. An
  EXAMPLE session's uid is **promoted** — the files (manifest included)
  and history install verbatim (`InstallSyncedProject`, `SeededFrom`),
  and from that moment the uid is the share link's unguessable access
  token — so the in-place handle swap needs no re-push and no reload. A
  SHARED VIEW runs the parent cloud document's uid, which the fork must
  NOT claim: `ForkTransientCopy` mints a fresh identity with a fresh
  `ForkedFrom` history (the parent's log stays the parent's), and the
  runtime is re-pushed once so its manifest agrees. Either way the URL
  heals to the fork's `/p/<slug>-<uid>` through the ordinary lens
  reconciliation, a toast confirms ("Forked — now editing your copy.",
  ruled at G1), the top bar's "example" pill vanishes, and subsequent
  saves flow to OPFS — with the install's catalog broadcast triggering
  the same auto-publish any fork gets.

> **Amended 2026-08-30 — the gesture is unchanged; the dirty gate is
> gone** (`2026-08-30-relationship-control-three-segment-bar.md`).
> Save-is-the-fork stands exactly as written, but it was only ever
> reachable through the Save button, which exists only while the overlay
> is dirty — so keeping an example required editing it first, which is
> this model biting the person who decided to stop viewing. The project
> popover's **"Save a copy"** is now available whenever the session is
> transient, pristine or not: it dispatches the same
> `ProjectOp::SaveOverlay`, and a pristine save commits nothing
> (`written == 0`) while still running the fork, so both paths are one
> path. Fork-on-first-*edit* remains rejected (see Alternatives). The
> "example pill vanishes" above is a state change rather than a
> disappearance now: the bar's relationship face flips Example → Private
> at the same moment the URL heals.

## Consequences

- Viewing creates **nothing**, so a thousand demo visitors cost a
  thousand nothing — no per-browser copies, no phantom URLs, no
  seed-once dedupe (`EnsureExampleSeeded` / `find_seeded_from` are
  deleted; existing seeded copies in user libraries keep working as
  ordinary self-sufficient packages — leave-alone ruling).
- The bare `/p/<slug>` grammar is a new resolution leg AFTER
  `share_link::split_segment` refuses — the split rule stays
  single-copy (the pre-#384 lesson). An unknown bare segment is the
  landing, never a guess.
- The transient marker rides the view
  (`UiStudioView::open_project_transient`); the web layer keys the bare
  address, the healing skip, and the fork toast off it. The session uid
  must never reach a URL or the cloud while transient — `notify_saved`
  is suppressed and transient handles never enter the close queue.
- Edit-access links keep the tracking-copy + visitor-push flow: an Edit
  save means push-to-cloud collaboration, where a persistent local copy
  is the right model.
- A failed fork leaves the session transient with a warning; the next
  save retries the whole install (the uid is not yet in the library, so
  retry cannot collide).

## Guest ownership (anonymous fork + publish, D3/D8)

Sign-in must not gate saving or sharing. A signed-out fork's publish
mints a **guest account**: a real `CloudUser` (so `require_user`,
publish, and the content plane work with zero per-callsite changes)
with a synthetic subject (`anon:<usr-uid>`), no email (a guest must
never resolve a pending member invite), `provider = "anonymous"`, and a
dedicated **`anonymous` column — the D8 pruning lever**: "which rows
are guest-owned" is `SELECT … FROM users WHERE anonymous = 1`, joined
through `projects.owner`. `POST /auth/guest` installs a year-long
session cookie (idempotent by cookie: a live session mints nothing);
**the cookie IS the ownership** — browser-held, by ruling. The client
mints it lazily at the first anonymous fork and refreshes its session;
the sync engine's signed-in edge then sweeps the library and publishes.
A guest session must never render as a signed-in account: the chrome
keeps the sign-in affordance, the account page keeps the invitation,
and the account switcher never remembers a guest.

Accepted consequences: losing the browser profile loses ownership
(claim-on-sign-in is parked future work); a guest can only ever touch
its own rows (uid-as-capability unchanged), and the abuse posture is
the marking + the ordinary rate limits, with pruning as the lever
against build-up.

## Alternatives Considered

- **Keep seed-on-open but mark copies unclaimed** (reuse/reset on
  reopen, claim on first edit): rejected — viewing still creates data,
  and the ruled principle is that it must not.
- **Fork on first edit** instead of explicit save: rejected for now —
  explicit save is the existing, legible gesture ("for now at least",
  D7), and pre-save play stays free.
- **Ephemeral uid + fresh mint at install**: rejected — the manifest
  patch at fork would diverge the library copy from the runtime
  (project.json is part of the canonical content hash), forcing either
  a runtime re-push (visible reload) or a permanent save-as-pull
  hash-tripwire mismatch. Promoting a manifest-carried uid makes the
  fork a pure install.

## Follow-ups

- Claim-on-sign-in for guest projects — parked (losing the browser
  profile loses ownership; accepted, D8).
- Registry-backed example curation (gallery vision M4) — embedded bytes
  remain the offline/dev content source for the same canonical
  identities.
