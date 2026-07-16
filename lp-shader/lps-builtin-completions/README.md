# lps-builtin-completions

Generated completion manifest for the GLSL editor: one `no_std`, zero-dependency
`const COMPLETIONS: &[CompletionEntry]` table covering every **user-callable**
shader builtin — LPFN typed overloads with descriptions, standard GLSL
(runtime imports like `sin`/`pow` plus the compiler-inlined set like
`mix`/`clamp`/`dot` from `lps_glsl::builtin_inventory`) and texture builtins
with name+arity snippets. IR/VM internals are deliberately absent. Consumed
by the studio editor (autocomplete).

**Do not edit `src/lib.rs` by hand** — it is emitted by `lps-builtins-gen-app`
from the builtin sources in `lps-builtins/src/builtins/` and the compiler's
inlined-builtin inventory. Completions must always be generated from the
compiler's builtin source, never hand-authored (roadmap D5): to change an
entry, change the builtin's signature, doc comment, or inventory row and
regenerate:

```bash
cargo run -p lps-builtins-gen-app   # or: just generate-builtins / scripts/build-builtins.sh
```

`tests/drift.rs` ties the manifest to `BuiltinId::all()`, the generated
name→`BuiltinId` mapping tables, and `lps_glsl::builtin_inventory` (itself
unit-tested against the compiler's own name/arity checks), so a phantom or
missing name fails the build.
