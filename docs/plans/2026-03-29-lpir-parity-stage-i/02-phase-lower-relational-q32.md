# Phase 2: Lower `isnan` / `isinf` per `q32.md` §6

## Scope of phase

Change `lower_relational` in `lps-naga/src/lower_expr.rs` so **`isnan`** and **`isinf`** emit
**always false** per lane on the Q32 filetest path, matching
[`docs/design/q32.md`](../../design/q32.md) §6 (no NaN/Inf encoding; div0 saturation values are *
*not**
`isinf`).

## Code organization reminders

- Remove or shrink `lower_isnan_component` / `lower_isinf_component` if replaced by a shared
  `lower_bool_false_lane(ctx)` helper at the **bottom** of the relevant section.
- Comment at call site: `// Q32: q32.md §6 — isnan/isinf always false`.

## Implementation details

- For each lane in `arg_vs` for `IsNan` / `IsInf`, allocate `IrType::I32` and
  `Op::IconstI32 { value: 0 }`
  (bool-as-i32).
- Delete IEEE-style `Fne(lhs, lhs)` and sentinel `0x7FFF_FFFF` / `i32::MIN` **comparison** paths
  used
  solely for `isinf` / `isnan` **in this relational lowering**. (If another caller reused those
  helpers, grep before deleting.)
- Keep **`All` / `Any`** logic unchanged unless a bug shows up in filetests.
- If Naga adds **`Relational::Not`**: per-lane `Ieq(lane, 0)` per [`q32.md`](../../design/q32.md)
  §6;
  only implement if the enum variant exists.

## Validate

```bash
cd lps && cargo test -p lps-naga && cargo check -p lps-naga
```

Spot-check LPIR or CLIF only if a filetest fails and you need to confirm no stray `Fne`
self-patterns
remain in `lower_relational` for isnan.
