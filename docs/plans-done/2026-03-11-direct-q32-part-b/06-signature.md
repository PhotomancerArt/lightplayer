# Phase 6: Signature Mapping

Implement `Q32Strategy::map_signature`.

## Source

`backend/transform/q32/signature.rs`

## Logic

Replace F32 parameters and returns with I32:

```rust
fn map_signature(&self, sig: &Signature) -> Signature {
    let mut new_sig = Signature::new(sig.call_conv);
    for param in &sig.params {
        let ty = if param.value_type == types::F32 { types::I32 } else { param.value_type };
        if param.purpose == ArgumentPurpose::Normal {
            new_sig.params.push(AbiParam::new(ty));
        } else {
            new_sig.params.push(AbiParam::special(ty, param.purpose));
        }
    }
    for ret in &sig.returns {
        let ty = if ret.value_type == types::F32 { types::I32 } else { ret.value_type };
        if ret.purpose == ArgumentPurpose::Normal {
            new_sig.returns.push(AbiParam::new(ty));
        } else {
            new_sig.returns.push(AbiParam::special(ty, ret.purpose));
        }
    }
    new_sig
}
```

This is identical to `convert_signature` in `backend/transform/q32/signature.rs`,
just expressed as a method on Q32Strategy.

## Implementation notes

- Need to add `AbiParam` to the imports in `numeric.rs`.
- The purpose-preservation logic is important for struct return parameters.
- `FloatStrategy::map_signature` is `sig.clone()` — already implemented in
  Plan A.
