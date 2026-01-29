# Phase 1: Add PointerBased Variant

## Description

Add the new `PointerBased` variant to the `LValue` enum and create the `PointerAccessPattern` enum to describe pointer access patterns. This phase adds the infrastructure without changing any behavior.

## Success Criteria

- [ ] `PointerBased` variant added to `LValue` enum
- [ ] `PointerAccessPattern` enum created with `Direct`, `Component`, and `ArrayElement` variants
- [ ] `LValue::ty()` method updated to handle `PointerBased` variant
- [ ] Code compiles without errors
- [ ] No functional changes (new variant not used yet)

## Implementation Notes

### Files to Modify

- `lp-glsl/crates/lp-glsl-compiler/src/frontend/codegen/lvalue/types.rs`

### Changes

1. Add `PointerAccessPattern` enum:
   ```rust
   #[derive(Debug, Clone)]
   pub enum PointerAccessPattern {
       /// Direct access: entire variable/vector/matrix
       Direct {
           component_count: usize,
       },
       /// Component access: `v.x`, `arr[i].xy`
       Component {
           indices: Vec<usize>,
           result_ty: GlslType,
       },
       /// Array element access: `arr[i]`
       ArrayElement {
           index: Option<usize>,
           index_val: Option<Value>,
           element_ty: GlslType,
           element_size_bytes: usize,
           component_indices: Option<Vec<usize>>,
       },
   }
   ```

2. Add `PointerBased` variant to `LValue` enum:
   ```rust
   /// Pointer-based storage: arrays, out/inout params, future structs
   PointerBased {
       ptr: Value,
       base_ty: GlslType,
       access_pattern: PointerAccessPattern,
   },
   ```

3. Update `LValue::ty()` method to handle `PointerBased`:
   ```rust
   LValue::PointerBased { base_ty, access_pattern, .. } => {
       match access_pattern {
           PointerAccessPattern::Direct { .. } => base_ty.clone(),
           PointerAccessPattern::Component { result_ty, .. } => result_ty.clone(),
           PointerAccessPattern::ArrayElement { element_ty, component_indices, .. } => {
               // Similar to ArrayElement variant logic
               if let Some(indices) = component_indices {
                   if indices.len() == 1 {
                       element_ty.vector_base_type().unwrap_or(element_ty.clone())
                   } else {
                       element_ty
                           .vector_base_type()
                           .and_then(|base| GlslType::vector_type(&base, indices.len()))
                           .unwrap_or(element_ty.clone())
                   }
               } else {
                   element_ty.clone()
               }
           }
       }
   }
   ```

### Code Organization

- Place `PointerAccessPattern` enum before `LValue` enum
- Keep related types grouped together
- Add clear documentation comments

### Formatting

- Run `cargo +nightly fmt` on changes before committing

### Language and Tone

- Use measured, factual descriptions
- Avoid overly optimistic language
- Code is a work in progress, not "complete"
