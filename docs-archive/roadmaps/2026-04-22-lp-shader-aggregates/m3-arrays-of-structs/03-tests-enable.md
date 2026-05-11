# Phase 3 — Outer-field, Test Corpus, Enablement

## Goal

Complete M3 by handling the `s.ps[i].x` pattern (outer struct containing
array-of-struct field), enabling all tests, and filing deferred follow-ups.

## Files to modify

| File                                                                   | Changes                                                                                                       |
| ---------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| `lp-shader/lps-frontend/src/lower_struct.rs`                           | Extend `peel_arrayofstruct_chain` to recognize array-of-struct fields inside outer slot-backed struct locals. |
| `lp-shader/lps-filetests/filetests/struct/array-of-struct.glsl`        | Flip `// test error` → `// test run`. Verify all 19 test cases pass.                                          |
| `lp-shader/lps-filetests/filetests/const/array-size/struct-field.glsl` | Remove `// @unimplemented(...)` markers for the three M3 targets.                                             |
| `lp-shader/lps-filetests/filetests/array/of-struct/` (new dir)         | Add per-shape test files (see below).                                                                         |

## Extended peeler for outer-struct-field arrays

The `s.ps[i].x` pattern in Naga:

```
AccessIndex {
    base: Access {
        base: AccessIndex { base: s_local, index: ps_field_idx },
        index: i_expr
    },
    index: x_idx
}
```

The peeler Phase 2 implementation should already handle chains of AccessIndex
before and after the array Access. What's new in Phase 3 is that the array
root is not a direct local variable, but a struct field of a local.

Extend the peeler to recognize this root pattern:

```rust
enum ArrayRoot {
    /// Direct local array: Point ps[4];
    Local(AggregateInfo),  // info for the array
    /// Field of outer struct: MyStruct s; where s.ps is Point[4]
    StructField {
        outer_local: Handle<LocalVariable>,
        outer_struct_info: AggregateInfo,  // info for the outer struct s
        field_idx: u32,                   // index of ps field in outer struct
        field_array_info: AggregateInfo,  // info for the array-of-struct field
    },
}
```

When the root is `StructField`:

1. Get base address of `outer_local` via `aggregate_storage_base_vreg`
2. Get member offset of `field_idx` from `outer_struct_info.layout`
3. Add to get array base address
4. Proceed with index × stride as normal

## New test files (array/of-struct/)

Create directory `lp-shader/lps-filetests/filetests/array/of-struct/` with:

### 1. `declare-init-list.glsl`

```glsl
// test run
// expected: 1 2 3 4

struct Point { float x; float y; };

void main() {
    Point ps[2] = Point[2](Point(1.0, 2.0), Point(3.0, 4.0));
    output_f32(ps[0].x);
    output_f32(ps[0].y);
    output_f32(ps[1].x);
    output_f32(ps[1].y);
}
```

### 2. `dynamic-index-rw.glsl`

```glsl
// test run
// expected: 5 6 7 8

struct Point { float x; float y; };

void main() {
    Point ps[4];
    for (int i = 0; i < 4; i++) {
        ps[i].x = float(i) * 2.0 + 1.0;  // 1, 3, 5, 7
        ps[i].y = float(i) * 2.0 + 2.0;  // 2, 4, 6, 8
    }
    // Read back with different dynamic indices
    int j = 2;
    output_f32(ps[j].x);  // 5
    output_f32(ps[j].y);  // 6
    j = 3;
    output_f32(ps[j].x);  // 7
    output_f32(ps[j].y);  // 8
}
```

### 3. `inout-param.glsl`

```glsl
// test run
// expected: 10 20

struct Point { float x; float y; };

void modify(inout Point ps[2]) {
    ps[0].x = 10.0;
    ps[0].y = 20.0;
}

void main() {
    Point ps[2];
    ps[0].x = 0.0;
    ps[0].y = 0.0;
    modify(ps);
    output_f32(ps[0].x);
    output_f32(ps[0].y);
}
```

### 4. `out-param.glsl`

```glsl
// test run
// expected: 30 40

struct Point { float x; float y; };

void fill(out Point ps[2]) {
    ps[0] = Point(30.0, 40.0);
}

void main() {
    Point ps[2];
    fill(ps);
    output_f32(ps[0].x);
    output_f32(ps[0].y);
}
```

### 5. `nested-field.glsl` (s.ps[i].x)

```glsl
// test run
// expected: 100 200

struct Point { float x; float y; };
struct Container {
    Point ps[2];
};

void main() {
    Container c;
    c.ps[0].x = 100.0;
    c.ps[0].y = 200.0;
    output_f32(c.ps[0].x);
    output_f32(c.ps[0].y);
}
```

### 6. `zero-init.glsl`

```glsl
// test run
// expected: 0 0

struct Point { float x; float y; };

void main() {
    Point ps[2];  // zero-initialized
    output_f32(ps[0].x);
    output_f32(ps[0].y);
}
```

## Enabling existing tests

### struct/array-of-struct.glsl

Current state: `// test error` (expecting compile failure).

After Phase 2/3: should pass. Change to `// test run` and run:

```bash
scripts/filetests.sh --file struct/array-of-struct.glsl
```

Fix any failures. The file has 19 test cases covering various patterns.

### const/array-size/struct-field.glsl

Current state: Has `// @unimplemented(wasm.q32)`, `// @unimplemented(rv32c.q32)`,
etc.

After Phase 3: remove these markers (or run with `LP_FIX_XFAIL=1` or `--fix`).

## Deferred work (file follow-up bugs)

- **Array-of-struct equality** (`ps == qs`): Add a test file
  `array/of-struct/eq.glsl` with `// @unimplemented(all)` and file bug N.
  The test:

  ```glsl
  // @unimplemented(all)
  // TODO(bug-NN): array-of-struct equality
  void main() {
      Point a[2], b[2];
      bool same = (a == b);  // should work but out of M3 scope
  }
  ```

- **Multidim array-of-struct** (`Point ps[4][4]`): If not working after Phase 3,
  add `// @unimplemented` test and file bug.

## Acceptance criteria

- All new `array/of-struct/*.glsl` tests pass
- `struct/array-of-struct.glsl` passes (flipped to `// test run`)
- `const/array-size/struct-field.glsl` passes with `@unimplemented` removed
- `rv32c.q32` and `rv32n.q32` have parity (same set of passing tests)
- Any failure orthogonal to M3 scope has `// TODO(bug-N)` marker and filed issue
- `cargo test -p lps-filetests --test filetests` passes (or script equivalent)

## Filetest runner commands

```bash
# Run all tests
scripts/filetests.sh

# Run specific test
scripts/filetests.sh --file struct/array-of-struct.glsl

# Fix xfail annotations
scripts/filetests.sh --fix
# Or: LP_FIX_XFAIL=1 scripts/filetests.sh

# Run with specific target filter
scripts/filetests.sh --target wasm.q32
```

## Post-M3 cleanup

After all phases complete and tests pass:

1. Verify no debug prints or TODOs left in code
2. Run `cargo clippy -p lps-frontend -D warnings`
3. Update `docs/roadmaps/2026-04-22-lp-shader-aggregates/m3-arrays-of-structs.md`
   with status: "Complete — YYYY-MM-DD"
4. Add link to the new plan directory in the roadmap
