# Phase 7: Update Gen-App and Regenerate

## Scope

Update the `lp-glsl-filetests-gen-app` templates to emit the new annotation
format instead of `// target riscv32.q32`. Regenerate all `.gen.glsl` files.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Update `lp-glsl-filetests-gen-app/src/util.rs`

The `generate_header` function currently returns:

```
// This file is GENERATED. Do not edit manually.
// To regenerate, run:
//   lp-glsl-filetests-gen-app {specifier} --write
//
```

Keep this as-is. The `// test run` line and annotations are added by
each generator, not the header.

### Update each generator in `src/vec/`

Every generator file (`op_add.rs`, `op_equal.rs`, `op_multiply.rs`,
`fn_equal.rs`, `fn_greater_equal.rs`, `fn_greater_than.rs`,
`fn_less_equal.rs`, `fn_less_than.rs`, `fn_max.rs`, `fn_min.rs`) has:

```rust
content.push_str("// test run\n");
content.push_str("// target riscv32.q32\n");
```

Change to:

```rust
content.push_str("// test run\n");
content.push_str("// @unimplemented(backend=wasm)\n");
```

All generated vector tests use vector types, which wasm doesn't support.
The file-level `@unimplemented(backend=wasm)` is correct for all of them.

### Handle `[expect-fail]` in generated output

The gen-app itself doesn't emit `[expect-fail]` — any such markers in
`.gen.glsl` files were added manually. After regeneration they'll be gone.
If a generated test needs `@unimplemented()`, it should be added to the
generator template.

Check `fn_equal.rs` and `ivec2/fn-equal.gen.glsl` — these have a manually
added `[expect-fail]` on the last test case
(`test_*_equal_function_in_expression`). If this is a real expected failure,
add it to the generator:

```rust
// Before the last run directive:
content.push_str("// @unimplemented()\n");
content.push_str(&format!("// run: ..."));
```

### Regenerate all files

```bash
cd lp-glsl
cargo run -p lp-glsl-filetests-gen-app -- --write
```

Or regenerate specific categories:

```bash
cargo run -p lp-glsl-filetests-gen-app -- vec --write
```

### Verification

After regeneration, no `.gen.glsl` file should contain `// target` or
`[expect-fail]`:

```bash
grep -r '// target' lp-glsl/lp-glsl-filetests/filetests/ --include='*.gen.glsl'
grep -r '\[expect-fail\]' lp-glsl/lp-glsl-filetests/filetests/ --include='*.gen.glsl'
```

## Validate

```
scripts/glsl-filetests.sh
cargo +nightly fmt -- --check
```
