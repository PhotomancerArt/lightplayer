# Phase 1 — `LpsFnKind` on `LpsFnSig`

## Scope

Add a structured "is this user code or a synthesised helper?"
discriminant to `LpsFnSig` so consumers can filter without parsing
the `__` name prefix. Pure prep step; no behavioural change beyond
the new field. Lands first because Phases 2–4 add new functions to
`LpsModuleSig.functions` and want to tag them as `Synthetic` from the
moment they're created.

Closes Q11 in [`00-notes.md`](./00-notes.md).

## Code organisation reminders

- The new enum is a *coarse* two-variant ("user vs synthetic"), not
  per-synthetic-function variants. Finer-grained variants can be
  added later if a consumer actually needs to pattern-match on them.
- All call sites that construct `LpsFnSig` need a `kind` value; default
  is `LpsFnKind::UserDefined` (provided via `Default`) so existing
  test-construction patterns can use `..Default::default()` where
  the file already does.
- The synthesised `__shader_init` (in `lps-frontend/src/lower.rs`) is
  the one existing call site that should be tagged `Synthetic` in
  this phase. M2.0's `__render_texture_<format>` synthesis (Phase 3)
  will tag itself `Synthetic` as well.

## Implementation details

### `lp-shader/lps-shared/src/sig.rs`

Add the enum and the field:

```rust
/// Whether a function in `LpsModuleSig.functions` is user-authored
/// or synthesised by the toolchain.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LpsFnKind {
    /// Lowered from user GLSL.
    #[default]
    UserDefined,
    /// Synthesised by lps-frontend or lp-shader (e.g. `__shader_init`,
    /// `__render_texture_<format>`). Convention: name begins with `__`.
    Synthetic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LpsFnSig {
    pub name: String,
    pub return_type: LpsType,
    pub parameters: Vec<FnParam>,
    pub kind: LpsFnKind,           // NEW
}
```

Re-export from `lps-shared/src/lib.rs` if `LpsFnSig` itself is
re-exported there (likely yes — confirm and add `LpsFnKind` to the
same `pub use`).

Update the file's existing test (`minimal_module_meta_fields`,
around `sig.rs:92`) to set `kind: LpsFnKind::UserDefined`.

### `lp-shader/lps-frontend/src/lower.rs`

Two construction sites:

- Around line 65 (user-lowered functions):
  set `kind: LpsFnKind::UserDefined`.
- Around line 76 (`__shader_init` synthesis):
  set `kind: LpsFnKind::Synthetic`.

### Other construction sites (test / fixture files)

Compile-only: every `LpsFnSig { … }` literal in the workspace needs a
`kind` value. Options per site:

- For literals that can use `..LpsFnSig::default()`: rely on
  `LpsFnKind::default() == UserDefined`.
- For exhaustive struct literals: add `kind: LpsFnKind::UserDefined`.

Sites flagged by `cargo build` (full list):

```
lp-shader/lpvm-cranelift/src/lib.rs            (around line 160)
lp-shader/lpvm-emu/src/lib.rs                  (around line 71)
lp-shader/lpvm-native/src/abi/frame.rs         (around line 184)
lp-shader/lpvm-native/src/abi/func_abi.rs      (~12 sites, lines 202–340)
lp-shader/lpvm-native/src/compile.rs           (lines 183, 281)
lp-shader/lpvm-native/src/debug_asm.rs         (around line 108)
lp-shader/lpvm-native/src/emit.rs              (around line 170)
lp-shader/lpvm-native/src/isa/rv32/abi.rs      (around line 302)
lp-shader/lpvm-native/src/isa/rv32/emit.rs     (around line 1006)
lp-shader/lpvm-native/src/regalloc/test/abi_fixtures.rs   (line 13)
lp-shader/lpvm-native/src/regalloc/test/builder.rs        (lines 95, 104)
lp-shader/lps-filetests/tests/rv32n_smoke.rs   (around line 49)
lp-cli/src/commands/shader_debug/collect.rs    (around line 40)
```

Mechanical update — all are `UserDefined` (these construct stub or
user-test sigs).

`px_shader.rs:65` (`render_sig`) only *reads* `LpsFnSig`; no change.

### Tests added in this phase

```rust
// lp-shader/lps-shared/src/sig.rs (extend the existing tests module)

#[test]
fn fn_kind_default_is_user_defined() {
    assert_eq!(LpsFnKind::default(), LpsFnKind::UserDefined);
}
```

```rust
// lp-shader/lps-frontend/src/tests.rs (or wherever lowering tests live)
//
// Verify the __shader_init function (when synthesised) carries
// kind == Synthetic, while user functions carry kind == UserDefined.

#[test]
fn shader_init_is_marked_synthetic() {
    let glsl = r#"
        float gShared = 0.5;
        vec4 render(vec2 pos) {
            gShared = pos.x;
            return vec4(gShared);
        }
    "#;
    let naga = lps_frontend::compile(glsl).unwrap();
    let (_ir, meta) = lps_frontend::lower(&naga).unwrap();

    let init = meta.functions.iter().find(|f| f.name == "__shader_init")
        .expect("expected __shader_init for module with non-const global");
    assert_eq!(init.kind, LpsFnKind::Synthetic);

    let render = meta.functions.iter().find(|f| f.name == "render").unwrap();
    assert_eq!(render.kind, LpsFnKind::UserDefined);
}
```

(If the test fixture above doesn't trigger `__shader_init` synthesis,
adjust to whatever does — check `lower.rs:73-82` for the trigger
condition: `!global_map.is_empty() && synthesize_shader_init(...)
returns Some`.)

## Validate

```bash
cargo check -p lps-shared
cargo check -p lps-frontend
cargo build --workspace --all-features    # catches every LpsFnSig literal
cargo test  -p lps-shared
cargo test  -p lps-frontend
```

No behavioural change expected; existing tests continue to pass.
The new field is additive and ignored by every current consumer.
