# Phase 6: errors/

## Scope

Create `const/errors/` files using `// test error` and inline `// expected-error` (f66023c).

## Code Organization

- Error tests: expect compile failure with matching diagnostics
- Use inline syntax: `stmt;  // expected-error [E0xxx:] {{message}}`

## Implementation Details

1. **non-const-init.glsl** — Non-constant expression in const init:
   ```
   // test error
   // target riscv32.q32

   float non_const = 1.0;
   const float BAD = non_const;  // expected-error {{...}}
   ```
   - Error code/message: discover from compiler when writing; may need `{{not a constant expression}}` or similar

2. **user-func.glsl** — User-defined function in const init (spec: cannot form constant expr):
   ```
   float get_val() { return 1.0; }
   const float BAD = get_val();  // expected-error {{...}}
   ```

Note: Exact error messages depend on compiler output. Run compile to capture actual message, then use substring in `{{...}}`.

## Validate

```
just filetest const/errors/
```

Error tests should pass (compiler rejects invalid const; expectations match).
