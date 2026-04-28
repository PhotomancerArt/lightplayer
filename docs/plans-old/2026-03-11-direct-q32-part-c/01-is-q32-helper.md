# Phase 1: Convenience Helper on CodegenContext

## Rationale

The codegen needs to know whether it's compiling for float or Q32 to
dispatch builtin calls correctly. Rather than adding a separate
`float_mode: DecimalFormat` field, we derive everything from the
existing `numeric: NumericMode` field — one source of truth.

## Changes

No new fields. Add a convenience method to `CodegenContext`:

```rust
impl<'a, M: Module> CodegenContext<'a, M> {
    pub fn is_q32(&self) -> bool {
        matches!(self.numeric, NumericMode::Q32(_))
    }
}
```

All subsequent phases use `self.is_q32()` to branch on numeric mode.

## Validate

```bash
cargo check -p lps-compiler --features std
scripts/filetests.sh
```

No behavioral change — just a helper.
