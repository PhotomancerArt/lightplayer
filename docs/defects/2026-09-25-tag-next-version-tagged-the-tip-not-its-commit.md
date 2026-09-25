---
status: fixed
found: 2026-09-25      # ci — a stranded production deploy while shipping #795
fixed: this change
area: scripts/tag-next-version.sh + .github/workflows/main-push.yml
class: assumed-context
related:
  - two-green-prs-can-red-main.md (debt — the other back-to-back-merge hazard)
---
# Main push tagged the tip of main, not its own commit, and stranded a deploy

**Symptom** — #795 merged as 9a2dc1a54 and #829 as 14c2fb495 within about a
minute. The first "Main push" run tagged `v2026.09.25-7` onto **14c2fb495**;
9a2dc1a54 never got a tag. The second run failed with `Error: A version tag
already exists for this commit` (run 36130854039). Deploy Cloud Service, riding
the first run's success, checked out `workflow_run.head_sha` = 9a2dc1a54 and
failed `print-app-version.sh --require-tag` on both attempts with `Error: The
current commit is not tagged with a version.` (run 36130942931). Production did
not update until a manual `gh workflow run deploy-cloud.yml --ref main` (run
36131775385).

The same race was seen on 2026-08-29 (#459 then #464) and written down as a
"benign RED" in agent memory: that time the stranded deploy happened not to
matter, so nobody read the deploy side.

**Root cause** — `tag-next-version.sh` ran `git pull origin main` and then tagged
`HEAD`. A run is triggered for one commit (`$GITHUB_SHA`) but tagged whatever
main's tip was when the step ran. Two consumers disagree about which commit a
run is for: the tagger assumed "the tip", `deploy-cloud.yml` uses the run's
`head_sha`. The concurrency group made the runs queue, but it could not help —
the first run had already moved past its own commit. A second, latent hazard:
the version number was computed from `ls-remote` and then pushed without any
retry, so two unserialized runs could pick the same number.

**Fix** — the script tags `$TAG_SHA`, else `$GITHUB_SHA`, else `HEAD`, and never
pulls; it refuses a commit that is not on `origin/main`. A commit that already
carries a version tag on origin exits 0 (and fetches the tag so
`print-app-version.sh` reads it). The number is claimed by pushing
`<sha>:refs/tags/<tag>` directly — the remote refuses an existing name — and a
refusal re-reads the remote and tries the next number (or exits 0 if the winner
tagged this same commit). `create-release` skips a release that already exists,
so a re-run is green end to end.

**Regression coverage** — `scripts/tag-next-version-test.sh`, run by `just
lint-tag-next-version` in `check-lint`: back-to-back merges each get their own
tag, a re-run is a no-op, a lost race (simulated by the bare remote's `update`
hook claiming the name first) retries to the next number, a race lost to the
same commit exits 0, numbering is numeric per date, annotated tags count, and
off-main commits are refused. The pre-fix script fails 9 of its 11 checks.

**Lesson** — a CI job that is *triggered for* a commit must act on that commit,
by sha, and never re-derive it from a branch name. "Fetch the latest and act on
it" is right for a human at a terminal and wrong for a run that other runs
(here, a `workflow_run` deploy) will key off by `head_sha`. A red run that
"did the right thing anyway" deserves one look at what else keyed off it.
