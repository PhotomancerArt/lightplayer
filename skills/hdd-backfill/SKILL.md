---
name: hdd-backfill
description: Backfill human-driven-development (HDD) intent for part of an existing codebase so it can be changed through intent — guide the user to write intent for the area they want to change, generate a matching `.gen.md` describing the current code in their style, reconcile each difference (reword or keep), then checkpoint with an implementation round. Use when the user wants to change software whose intent isn't written yet, says "hdd backfill", or when hdd-change finds the area to change has no detailed intent.
---

# HDD backfill

Help the user backfill intent in a repository that has adopted HDD, so they can change the software by changing intent that isn't written yet.

The user writes the intent. You guide them, describe the code when asked, and compare their intent with the code. Do not write the user's intent files.

## Conventions

These are shared by all HDD skills.

- `<repo>` is the root of the primary checkout of the repo.
- `<change-slug>` is a short kebab-case identifier for the change, e.g. `add-profile-page`, `fix-loading-animation`.
- `<change-id>` is the current local timestamp plus the slug: `YYYY-MM-DD-hhmm-<change-slug>`, e.g. `2026-11-07-0030-intent-skills`.
- `<change-dir>` is `<repo>/.changes/<change-id>`. It holds ephemeral content for the change, is gitignored and is managed by agents.
- `<change-branch>` is `change/<change-slug>`.
- `<change-worktree>` is where the work on the change happens. Its standard location is `<repo>/.worktrees/<change-id>`. If the harness or IDE supplied a worktree, use it, and symlink it to `<repo>/.worktrees/<change-id>`.

Intent commits include changes to `_intent/` and contain only intent. Omit co-authorship trailers such as `Co-Authored-By` from them unless you directly authored the intent. Quote the intent in the message where possible.

## 1. Start from the change

Backfill starts with the user stating the change they want to make, e.g. "separate the device cards so they're sorted by connection type: cloud, Bluetooth, USB". Keep this as the goal; it isn't written into intent yet.

If the change has no `<change-slug>`, `<change-dir>` and `<change-worktree>` yet, set them up as `hdd-change` does, and tell the user the full paths.

Look at the code to understand its structure: where the area being changed lives, and the concepts the change depends on.

## 2. Decide what needs intent

The part of the software being changed, and its descendants in the theory hierarchy, need enough detail to make the change. Detail forms a hierarchy:

- Higher-level components need only a general description, often a sentence or two.
- The portions being changed need enough detail to capture the change.

Check what intent already exists. The user decides how to structure the intent so it makes sense to them; they are backfilling their own understanding of the theory. Help them decide where new statements belong, but don't impose the code's structure on them.

## 3. The user writes their intent

Guide the user through writing intent for that area: ask questions, answer questions about how the code works, and point out what the change will need defined.

The aim is to describe the software as written, with minimal changes. Some change is acceptable if the user wants it: they're focused on this area now, and it's a reasonable time to bring it in line with their theory.

## 4. Generate the comparison

After the user has written their intent, write an `<intent-name>.gen.md` beside each intent file they wrote or changed (e.g. `devices.gen.md` beside `devices.md`). It describes what the code currently does for that portion, **in the style of the user's intent**: same structure, granularity and phrasing where possible, so the diff shows real differences rather than wording.

`.gen.md` files are not committed. Check that `_intent/.gitignore` (or another ignore rule) covers `*.gen.md`; if not, tell the user.

The user's intent is the desired state. The diff from the `.gen.md` to the user's intent drives the checkpoint's implementation round.

## 5. Reconcile differences

Before the checkpoint, walk the user through the differences between their intent and the `.gen.md`. Label them `D1`, `D2`, ….

Only contradictions need a decision. Detail in the `.gen.md` that the user's intent doesn't mention is not a difference.

For each difference, show what the user's intent says and what the code does, and suggest an intent diff that would match the code, if it's reasonably small. The user decides:

1. **Reword:** the code is right; the user updates their intent to match it.
2. **Keep:** the user's intent is what they want; the implementation should change.

The user may answer several at once ("keep D2, reword the rest"). After they reword, re-compare and present any differences that remain.

Once the user has stated that every remaining difference is intentional, proceed to the checkpoint.

## 6. Checkpoint

When the intent likely matches the current implementation, suggest making a checkpoint.

A checkpoint is an `hdd-implement` round that isn't expected to change much but establishes a baseline for intent changes. Small changes from the differences the user chose to **Keep** bring the code in line with the intent as written.

1. Commit the user's intent as an intent commit.
2. Run an implementation round with the `hdd-implement` skill, using the `.gen.md` comparison as its intent diff. Give it the `<change-worktree>`, the `<change-dir>` and the list of kept differences.

The round records its start commit, so later rounds diff only from the checkpoint.

## 7. Make the actual change

Then the user makes the change they actually want against the intent, and the usual process follows: refine it as in `hdd-change`, then implement with `hdd-implement`.
