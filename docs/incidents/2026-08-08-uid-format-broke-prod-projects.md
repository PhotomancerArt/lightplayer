---
status: actions-open
date: 2026-08-08
severity: severe
duration: 5h 10m (fix availability; per-project recovery = one Upgrade click)
related:
  [
    "../adr/2026-08-07-uid-format-single-token-base32.md",
    "../adr/2026-08-04-project-format-migration-architecture.md",
    "../debt/library-format-migration-gap.md",
    "PR #384 (the change)",
    "PR #391 (the repair)",
  ]
---
# Uid format change broke every deployed project

**Impact** — From the merge of PR #384 until PR #391 shipped, every
deployed v5 project whose manifest carried an old-style uid refused to
load: *"project.json is at the current format but could not be read
(project.json uid: uid body must be exactly 16 characters). Export a
copy to repair it by hand, or delete the project."* All of the
production library was affected. The offered remedies (export-and-hand-
edit, delete) were destructive or manual; the real remedy (the v5→v6
upgrader) did not exist yet. No data was lost.

**Timeline** (UTC, 2026-08-08; PR timestamps are exact, human events
approximate)

- Aug 7 (day) — the uid re-rendering (`prj_…` base-62 → `prj…`
  base-32) is designed and built as PR #384. The ADR calls the moment
  "pre-lock-in — no outside users hold links or files"; all in-repo
  fixtures are migrated inside the same PR. No format bump: no field
  changed shape, only a string's rendering.
- 00:07 — #384's CI turns green on the substantive tree.
- **00:59 — #384 merges. Impact begins**: deployed v5 projects now
  fail the manifest read. Nothing signals it; CI stays green because
  the tree is self-consistent.
- ~03:30 — user reports every prod project showing the unreadable-
  manifest error; proposes shipping a format version + updater.
- ~03:45 — diagnosis: the classifier reads `format: 5` as *current*,
  so the upgrade path never engages; the change was a persisted-bytes
  break without a version to key on.
- 03:45–05:30 — repair built: `PROJECT_FORMAT_VERSION` 6, frozen v5
  snapshot, `lpa-upgrade` v5→v6 value-preserving uid transcode, v5
  corpus + goldens, AGENTS.md drill. #384 had merged mid-build, so the
  work lands as follow-up PR #391.
- **06:09 — #391 merges. Fix available**; each project recovers via
  the normal open/Upgrade flow, pre-migration bytes kept in history.

**Contributing causes**

1. **"Pre-lock-in" was assessed against outside users, not deployed
   data.** The team's own production library already held v5 bytes;
   dogfood data was not on the change's compatibility checklist, so a
   true statement ("no outside users") licensed a false conclusion
   ("nothing breaks").
2. **CI is structurally blind to persisted-compat breaks.** The same
   PR migrates every fixture and example, so the tree is always
   self-consistent and green. Green CI here is evidence of internal
   consistency — it says nothing about bytes that live outside the
   repo. (The debt file `library-format-migration-gap.md` had named
   this hazard class in July; the migration *tool* existed, but
   nothing forced this change to use it.)
3. **The wire no-compat culture generalized to persisted bytes.** The
   AGENTS.md compatibility section covered the wire ("delete the old
   form outright") and was silent on persistence; a string re-render
   with zero structural change didn't pattern-match to "format bump"
   under that framing.
4. **The merge raced in-flight compat work.** The compat gap was
   discovered and the upgrader was being built while the PR sat green
   and mergeable; nothing on the PR (draft state, label, comment)
   signaled "known-incomplete — upgrader in flight."

**What went well** — Detection-to-fix was ~2.5 h of work, same
session; the `just format-bump` procedure worked exactly as designed
on first real use; the transcode is value-preserving, so efuse-derived
device uids came out identical to what hardware now derives (device
associations survived); history kept pre-migration bytes as undo.

**Where we got lucky** — Only string renderings changed, so a
value-preserving transcode could exist at all; device-side identity
treats uids as opaque strings, so fielded firmware never noticed;
production had a single-digit userbase (a fact about impact, not severity — see the README rubric); the cloud store's old-format rows were
already scheduled for a wipe.

**Action items**

- [x] Standing rule: *a change to persisted bytes IS a format bump* —
  enforcement: AGENTS.md "Persisted-format compatibility" section
  (shipped in #391).
- [x] Old-bytes-through-current-reader coverage: v5 corpus project with
  old-style uids + version-generic goldens harness — enforcement: CI
  (`lpa-upgrade` corpus goldens, shipped in #391).
- [x] Cloud store still holds old-format uid rows — enforcement: ADR
  deferred table row, trigger = next lightplayer.app deploy.
- [x] A PR with a known-incomplete compat story must say so on the PR
  (convert to draft or comment "DO NOT MERGE: <what's in flight>") the
  moment the gap is found — enforcement: AGENTS.md, alongside the
  persisted-format drill (shipped with this entry).
- [ ] Establish this registry and its process — enforcement: this
  directory's README; optionally a `yona-postmortem` skill wrapping the
  "Running a post-mortem" steps.
