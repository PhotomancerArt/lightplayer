---
status: carried
since: 2026-08-02      # first instance recorded here; the condition is older
logged: 2026-08-02
area: CI / merge policy
related:
  - conflicted-pr-gets-no-ci.md
---
# Two independently-green PRs can merge into a red main

**Shape** — CI validates each pull request against **its own branch**, never
against the state that will exist after it merges. When two PRs change opposite
sides of the same interface, both go green, both merge, and `main` breaks with
no check having failed anywhere. Nothing in the pipeline builds the merge
result, so the break is invisible until a human runs a build.

This is the ordinary semantic-merge-conflict problem. It is called out here not
because it is novel but because the repo's CI is slow enough that the usual
cheap fix — "require branches be up to date before merging" — is itself
expensive, so the tradeoff needs to be written down rather than re-litigated
each time it bites.

**Carrying cost** — low frequency, moderate blast radius. It has bitten a
handful of times. Each incident costs whoever next builds `main` a confused
detour: they reasonably assume their own branch broke something, and the
evidence looks like their fault until they check out `main` clean and reproduce
it there. The cost is diagnostic time and misattributed blame, not lost work.

⚠️ The sharpest version of the risk is not the compile error — those are loud.
It is a merge whose halves are *type-compatible but semantically wrong*. The
2026-08-02 incident below happened to fail loudly at the type level. Had the
new parameter been, say, a `bool` rather than a new enum, both sides would have
compiled and the oracle would have silently measured the wrong thing.

**Workarounds** — when `main` looks broken, check out `origin/main` clean and
reproduce before assuming your branch caused it:

```bash
git checkout -q origin/main && cargo test -p <crate> --test <name> --no-run
```

If it reproduces there, fix it in a separate clearly-labelled commit so the
repair is not buried inside unrelated work, and say so on the PR.

**Incident log**

- **2026-08-02** — `xt_classic_codemem_corpus.rs` (added by #288) versus a new
  `FloatMode` parameter on `synthesise_render_texture` /
  `synthesise_render_samples_rgba16` (added by #287). Both PRs green on their
  own branches; neither CI run compiled the other's code. `main` at `d3ee69f09`
  failed to build `lpvm-native`'s test target. Found by a third session running
  `just check test` for unrelated work, initially suspected to be its own
  change. Repaired in #291 (`21841dc93`).

  Worth noting what made the repair non-trivial: the fix is two lines, but the
  *value* was load-bearing. That test is the byte-exact oracle for Xtensa JIT
  code size, and it documents its own contract as "the same pipeline the device
  runs … in Q32". `FloatMode::F32` would have compiled cleanly and silently
  measured different code sizes, destroying agreement with silicon. The repair
  was verified by confirming the figures were unchanged (`examples/basic`
  6,516 B, `quad-strips-v3` 2,032 B — both still matching `[JIT] used=`
  readings taken off a classic ESP32 the same day).

**Exit criteria** — accepted as carried for now; the frequency does not justify
the fix cost. Options, cheapest first, recorded so the next incident starts from
a decision rather than a discussion:

1. **A `main` build canary** — a scheduled `just check test` on `main` that
   reports where it broke and which merges are candidates. Does not prevent the
   break, but collapses the diagnostic cost to near zero and removes the
   misattribution, which is where the actual cost sits. Cheapest by a wide
   margin.
2. **Require branches up to date before merge** (a GitHub branch-protection
   setting). Prevents this class outright, but forces a rebase and a full CI
   re-run on every merge — expensive against this repo's CI duration, and it
   serialises merges.
3. **A merge queue** — tests the merge result properly. The correct answer, and
   disproportionate to a handful of incidents.

Retire this entry if (1) lands and proves sufficient, or if the frequency rises
enough to justify (2) or (3).
