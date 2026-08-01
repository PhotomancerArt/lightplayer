# Defect: a PR based on a non-`main` branch got no CI at all

- **Date:** 2026-07-30
- **Status:** fixed
- **Class:** `ungated-variant`
- **Area:** `.github/workflows/pre-merge.yml` (workflow trigger)

## Symptom

PR #195 was opened with `--base claude/xtensa-filetest-plan-f5fb69` to stack it on
#194. It reported **no checks whatsoever** — not pending, not red, not skipped.
An empty checks list, which reads exactly like "nothing to worry about".

It sat that way through review discussion. The author believed it validated
because a full local `just check && just test` was green. It was caught only
because a human noticed the PR was pointed at an odd base and asked.

## Cause

```yaml
on:
  pull_request:
    branches: ["main", "feature/*"]
```

A PR whose **base** is `claude/xtensa-filetest-plan-f5fb69` matches neither
pattern, so the workflow never fired. Branch protection had nothing to require,
because no check ever existed to be required.

Second half of the trap: when #194 merged and #195 was retargeted to `main`,
that still did not trigger anything. Changing a PR's base fires the `edited`
activity type, which is **not** in the default set (`opened`, `synchronize`,
`reopened`). So the PR became mergeable-to-main, still never validated, and now
with a base that made it *look* ordinary. Recovering required a close/reopen to
force `reopened`.

## Fix

`types: [opened, synchronize, reopened, edited]`.

Deliberate cost: `edited` also fires on title and body edits, so editing a
description re-runs the suite. Accepted — wasteful but never wrong, and
wrongness is the failure mode being fixed. The workflow comment records how to
tighten it later (gate on `github.event.changes.base`) and the trap to check
first: what skipped jobs do to required status checks.

## Why it survived

Nobody had stacked a PR in this repo before, so the configuration existed only
once and only briefly. The `branches:` filter is correct for its original
purpose — don't burn runners on every scratch branch — and nothing about it
looks wrong when read. The bug is not in the filter but in the **absence of a
signal when the filter excludes something**: GitHub renders "no CI configured for
this PR" and "CI passed" as very similar-looking UI.

## Lesson

**An empty checks list is not a green checks list, and the UI barely
distinguishes them.** When a PR shows no checks, that is a finding, not a
formality — establish *why* before merging.

The generalization, which cost real time twice on the same day: a local gate
passing says nothing about a configuration the local gate cannot express. That
applied here, and in the same PR it applied to a missing rv32 builtins image
that no developer machine can be without (`2026-07-30-xtensa-call-argument-clobber.md`
and `2026-07-30-xtensa-sret-pointer-clobber.md` are the compiler-side instances
of the same shape).
