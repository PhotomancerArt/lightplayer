# Phase 3: Test Triage

## Scope of phase

Triage Naga-limited tests:

1. Rewrite tests using non-standard GLSL syntax to use standard syntax
2. Annotate valid GLSL that Naga can't parse with `@unimplemented(reason="Naga frontend limitation")`

## Code organization reminders

- Filetest annotations go immediately before the `// run:` directive
- Use `@unimplemented(backend=jit)` or `@unimplemented(backend=wasm)` as appropriate
- Keep GLSL rewrites minimal - just fix the syntax issue

## Implementation details

### Identify test files to triage

From roadmap analysis, focus on:

- `vec/bvec{2,3,4}/fn-mix.glsl` - Naga can't resolve `mix(bvec, bvec, bvec)` overload
- `control/while/variable-scope.glsl` - check if non-standard syntax exists

### Case 1: Valid GLSL, Naga limitation (annotate)

`vec/bvec2/fn-mix.glsl` - `mix(bvec, bvec, bvec)` is valid GLSL but Naga can't resolve the overload.

Current state: already has `@unimplemented(backend=jit)` on line 17 for first test.

Verify all failing cases are annotated. If not, add annotation.

### Case 2: Non-standard syntax (rewrite)

Check `control/while/variable-scope.glsl` for C++ style `while (bool j = expr)`.

```glsl
// NON-STANDARD (C++ style)
while (bool j = someCondition()) { ... }

// STANDARD GLSL
bool j;
while (j = someCondition()) { ... }
// OR
bool j = someCondition();
while (j) { ...; j = nextCondition(); }
```

If found, rewrite to standard syntax.

### Other files to check

From parity audit:

- `builtins/matrix-compmult.glsl` - `matrixCompMult` unknown function (Naga parse)
- `builtins/common-isnan.glsl` / `common-isinf.glsl` - `Float literal is infinite` parse error
- `control/while/variable-scope.glsl` - variable declaration in condition

Apply decision tree:

- Can it be rewritten to standard GLSL? -> Rewrite
- Is it valid standard GLSL Naga can't handle? -> Annotate

## Validate

```bash
# Run triaged tests
cd /Users/yona/dev/photomancer/lp2025

# Check fn-mix tests are properly annotated
./scripts/filetests.sh --target jit.q32 "vec/bvec2/fn-mix.glsl"
./scripts/filetests.sh --target jit.q32 "vec/bvec3/fn-mix.glsl"
./scripts/filetests.sh --target jit.q32 "vec/bvec4/fn-mix.glsl"

# Check while/variable-scope after any rewrite
./scripts/filetests.sh --target jit.q32 "control/while/variable-scope.glsl"

# Verify suite still passes overall
./scripts/filetests.sh --target jit.q32
```

## Notes

- Do not modify Naga fork - follow "no fork work" principle from roadmap
- `matrixCompMult` and `isnan`/`isinf` literal issues: evaluate if tests can be rewritten
- If rewrite is impractical, annotate with reason
