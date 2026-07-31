# Filetest parse errors vanished instead of failing

**Found:** 2026-07-31, while implementing axis-scoped filetest dispositions (f32 M3).
**Fixed:** same change (`lps-filetests/src/lib.rs`, `parse/mod.rs`).

## What happened

A filetest whose directives failed to parse reported **zero failures** and
dropped out of the counts entirely. Two mechanisms stacked:

1. `parse/mod.rs` called the annotation parser as
   `if let Ok(Some(ann)) = parse_annotation_line(...)`, discarding the `Err`
   arm. A malformed annotation was simply not an annotation.
2. `run_filetest_with_line_filter` turned a whole-file parse failure into
   `Ok((Err(e), early_stats, …))`. The runner decides pass/fail from
   `stats.failed > 0 || !harness_completed`, and a parse failure produced
   `failed == 0` with `harness_completed == true`. The `Err` was carried
   around and never consulted.

So the file counted as passing, with no cases.

## What it hid

- **37 `@ignore(...)` annotations that had never been implemented.** There was
  no `ignore` arm in `parse_annotation_kind` and never had been. Every one was
  a no-op — including 34 `@ignore(backend=wasm)` markers predating wasm's
  control-flow support, which read as "this test is disabled on wasm" while the
  test was in fact running and passing there.
- **6 `@unimplemented(backend=wasm)` annotations in generated files**, plus 4
  more generators still emitting the line into output that no longer contained
  it. Predicate syntax was documented in the README and used in the corpus
  without ever being parseable.
- **`texture/error_texture_spec_missing_in_struct.glsl`**, which used three
  directives that do not exist (`// test compile-error`, `// target:`,
  `// expect-compile-failure:`). It had never run once. Its real content was
  also wrong twice over: the struct uniform needed `layout(binding = …)`, and
  the assertion it was reaching for was about a spec naming a non-sampler field.

## Mechanism worth remembering

Both halves are the same shape: **an error path that returns a success-shaped
value.** `if let Ok(..)` on a `Result` you did not intend to ignore, and
`Ok((Err(..), zeroed_stats))` where the caller only reads the stats. In a test
harness this failure mode is especially expensive, because the thing that
disappears is coverage — and coverage that disappears looks exactly like
coverage that passes.

Related: `docs/defects/README.md`'s note on assertions that silently stop
asserting. The lint-level answer taken here is that annotation parsing is
`?`-propagated and a parse failure is a harness failure, so a typo'd selector
is a red on the next run rather than a slow leak.

## Guard

`lps-filetests/src/parse/mod.rs` — `unknown_axis_value_fails_the_file`,
`broken_needs_a_reason_line_above_it`, `a_directive_comment_is_not_a_reason`.
