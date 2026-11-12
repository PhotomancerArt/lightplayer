# Phase 1: Core Types

## Scope

Define the axis enums, `Target`, `TargetFilter`, `Annotation`, and
`AnnotationKind` types. These are the foundation everything else builds on.
No behavioral changes yet — just new types with tests.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### Create `src/target/` module

Replace the existing `src/test_run/target.rs` with a new `src/target/` module
that holds the canonical types. The old `test_run/target.rs` will be removed
in a later phase (phase 4) when the runner is updated.

#### `src/target/mod.rs`

```rust
pub mod display;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Backend {
    Cranelift,
    Wasm,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Isa {
    Riscv32,
    Wasm32,
    Native,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecMode {
    Jit,
    Emulator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatMode {
    Q32,
    F32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Target {
    pub backend: Backend,
    pub float_mode: FloatMode,
    pub isa: Isa,
    pub exec_mode: ExecMode,
}

pub const DEFAULT_TARGETS: &[Target] = &[
    Target {
        backend: Backend::Cranelift,
        float_mode: FloatMode::Q32,
        isa: Isa::Riscv32,
        exec_mode: ExecMode::Emulator,
    },
    Target {
        backend: Backend::Wasm,
        float_mode: FloatMode::Q32,
        isa: Isa::Wasm32,
        exec_mode: ExecMode::Emulator,
    },
];

#[derive(Debug, Clone, Default)]
pub struct TargetFilter {
    pub backend: Option<Backend>,
    pub float_mode: Option<FloatMode>,
    pub isa: Option<Isa>,
    pub exec_mode: Option<ExecMode>,
}

impl TargetFilter {
    pub fn matches(&self, target: &Target) -> bool {
        self.backend.map_or(true, |b| b == target.backend)
            && self.float_mode.map_or(true, |f| f == target.float_mode)
            && self.isa.map_or(true, |i| i == target.isa)
            && self.exec_mode.map_or(true, |e| e == target.exec_mode)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnotationKind {
    Unimplemented,
    Broken,
    Ignore,
}

#[derive(Debug, Clone)]
pub struct Annotation {
    pub kind: AnnotationKind,
    pub filter: TargetFilter,
    pub reason: Option<String>,
    pub line_number: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    ExpectSuccess,
    ExpectFailure,
    Skip,
}

pub fn directive_disposition(
    file_annotations: &[Annotation],
    directive_annotations: &[Annotation],
    target: &Target,
) -> Disposition {
    for ann in directive_annotations.iter().chain(file_annotations.iter()) {
        if ann.filter.matches(target) {
            return match ann.kind {
                AnnotationKind::Ignore => Disposition::Skip,
                AnnotationKind::Unimplemented | AnnotationKind::Broken => {
                    Disposition::ExpectFailure
                }
            };
        }
    }
    Disposition::ExpectSuccess
}
```

#### `src/target/display.rs`

Implement `Display` for `Target` (produces `"cranelift.q32"`, `"wasm.q32"`
etc.) and a `Target::from_name(s)` parser for CLI usage.

```rust
impl Target {
    pub fn name(&self) -> String {
        format!("{}.{}", self.backend, self.float_mode)
    }

    pub fn from_name(s: &str) -> Result<&'static Target, String> {
        DEFAULT_TARGETS
            .iter()
            .find(|t| t.name() == s)
            .ok_or_else(|| {
                let valid: Vec<String> = DEFAULT_TARGETS.iter().map(|t| t.name()).collect();
                format!("unknown target '{}'. Valid targets: {}", s, valid.join(", "))
            })
    }
}
```

Implement `Display` for `Backend` and `FloatMode` to support the dotted name
format.

### Register the module

Add `pub mod target;` to `src/lib.rs`.

### Tests

In `src/target/mod.rs`, add `#[cfg(test)] mod tests`:

- `test_target_filter_empty_matches_all` — default TargetFilter matches every
  DEFAULT_TARGET
- `test_target_filter_backend` — `{ backend: Some(Wasm), .. }` matches only
  wasm targets
- `test_target_filter_float_mode` — `{ float_mode: Some(Q32), .. }` matches
  all q32 targets
- `test_target_filter_full` — all fields specified, matches exactly one target
- `test_target_filter_no_match` — contradictory filter matches nothing
- `test_disposition_no_annotations` — returns ExpectSuccess
- `test_disposition_matching_unimplemented` — returns ExpectFailure
- `test_disposition_matching_ignore` — returns Skip
- `test_disposition_non_matching` — returns ExpectSuccess
- `test_disposition_directive_overrides_file` — directive annotation checked
  before file annotation
- `test_default_targets_are_valid` — sanity check: each default target has
  consistent backend/isa/exec_mode (cranelift→riscv32→emu, wasm→wasm32→emu)

In `src/target/display.rs`:

- `test_target_name_cranelift_q32` — `"cranelift.q32"`
- `test_target_name_wasm_q32` — `"wasm.q32"`
- `test_target_from_name_valid` — round-trips correctly
- `test_target_from_name_invalid` — returns error with valid names listed

## Validate

```
cargo build -p lps-filetests
cargo test -p lps-filetests
cargo +nightly fmt -- --check
```

All existing tests must still pass (the new module is additive).
