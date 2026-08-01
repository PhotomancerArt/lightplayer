---
status: fixed
found: 2026-07-30      # how: ci
fixed: this change
area: .github/workflows/pre-merge.yml (Validate Xtensa (host))
class: assumed-context
related:
  - docs/defects/2026-07-30-xtensa-integer-div-by-zero-trap.md
---
# Defect: the Xtensa vacuity guard failed on its own doctest exclusion

- **Date:** 2026-07-30
- **Status:** fixed
- **Area:** `.github/workflows/pre-merge.yml` — `Validate Xtensa (host)`
- **Found by:** CI (main went red immediately after the guard landed)

## Symptom

`main` red, and every open PR blocked, within hours of the guard being added:

```
##[error]a test binary reported 'running 0 tests' — a file-level #![cfg] almost certainly emptied it
total tests passed: 351
```

351 tests passed, the canary ran, the floor held. Only the assertion misfired.

## Cause

The guard scrapes the test log and rejects any `running 0 tests`, since that is
the signature of a file a `#![cfg]` emptied. Doctest sections legitimately have
none, so it excludes them:

```awk
/^ *Doc-tests/ {doc=1} /^running 0 tests/ && !doc {found=1}
```

The same change also rewrote `test-xt-host` from an explicit `--test` list to a
plain `cargo test -p lpvm-native --features emu-xt,xt-corpus`, which now runs
doctests too — and `lpvm-native` has none. So the exclusion became load-bearing
on its first run.

It could not fire. The job sets `CARGO_TERM_COLOR: always`, and cargo colors its
status headers, so the line reaching the log is:

```
\e[1m\e[92m   Doc-tests\e[0m lpvm_native
```

`^ *Doc-tests` cannot match a line beginning with an escape sequence. `doc`
stayed `0`, and the doctest section tripped the check written to ignore it.

The two neighbouring assertions survived only by luck: cargo does not color
`running 0 tests` or `test result: ok.`, so their `^`-anchored patterns still
matched. The bug was one cargo release away from being three bugs.

## Fix

Strip ANSI escapes once, where the log is captured, rather than patching the one
pattern that happened to break:

```bash
just test-xt-host 2>&1 | sed -E 's/\x1b\[[0-9;]*[mGKHF]//g' | tee /tmp/xt-tests.log
```

Every assertion in the step anchors at start-of-line, so normalizing the input
once is what makes all three mean what they read as.

## Regression coverage

None automated: this is a workflow step, and the repo has no harness that
executes CI shell against synthetic logs. Verified by hand against a log
reproducing the exact colored header — the guard trips before the strip, passes
after, and **still trips on a genuinely vacuous binary** (a `running 0 tests`
appearing before any `Doc-tests` header), which is the property that matters.
If log-scraping gates keep accumulating, a fixture-driven test for them is
worth building.

## Lesson

A CI assertion that pattern-matches log text owns that text's format. This one
was written against the shape of a log the author read in a terminal, while the
job it runs in sets `CARGO_TERM_COLOR: always` — the guard and its input
disagreed about the input, and nothing in either place said so.

The generalizable move is the one the fix takes: **normalize at the boundary,
assert on the normalized form.** Patching the single pattern that broke would
have left two more anchored patterns depending on cargo's undocumented choice
of which lines to colorize.

Worth recording that it failed in the safe direction. A vacuity guard exists to
stop a green run from meaning nothing; this one produced a false *positive*, so
it blocked rather than admitted. A check of this kind that fails closed costs an
afternoon; one that fails open costs whatever it was supposed to catch.
