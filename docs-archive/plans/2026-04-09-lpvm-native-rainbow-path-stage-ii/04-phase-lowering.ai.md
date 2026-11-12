# Phase 4: Control Flow Lowering

## Scope of Phase

Extend `lower_ops` to handle LPIR control flow ops and produce VInst labels and branches.

## Code Organization Reminders

- Restructure `lower_ops` from simple loop to recursive descent for blocks
- Keep the main lowering logic in `lower_op` (per-op)
- Add new `lower_block` helper for processing contiguous op sequences

## Implementation Details

### 1. The Problem

Current `lower_ops` is a simple loop:
```rust
pub fn lower_ops(func: &IrFunction, float_mode: FloatMode) -> Result<Vec<VInst>, LowerError> {
    let mut out = Vec::with_capacity(func.body.len());
    for (i, op) in func.body.iter().enumerate() {
        out.push(lower_op(op, float_mode, Some(i as u32), func)?);
    }
    Ok(out)
}
```

This doesn't work for control flow because:
- `IfStart` causes us to skip to else block when lowering the then-block
- `Else` and `End` are structural markers, not actual operations to lower
- We need to manage label allocation and track which ops have been processed

### 2. Revised Structure

```rust
pub fn lower_ops(func: &IrFunction, float_mode: FloatMode) -> Result<Vec<VInst>, LowerError> {
    let mut ctx = LowerCtx {
        func,
        float_mode,
        out: Vec::with_capacity(func.body.len() * 2), // Extra space for branches/labels
        next_label: 0,
        processed: alloc::vec![false; func.body.len()],
    };
    
    ctx.lower_block(0, func.body.len())?;
    Ok(ctx.out)
}

struct LowerCtx<'a> {
    func: &'a IrFunction,
    float_mode: FloatMode,
    out: Vec<VInst>,
    next_label: LabelId,
    processed: Vec<bool>,
}

impl<'a> LowerCtx<'a> {
    fn alloc_label(&mut self) -> LabelId {
        let id = self.next_label;
        self.next_label += 1;
        id
    }
    
    /// Lower ops from start (inclusive) to end (exclusive).
    fn lower_block(&mut self, start: usize, end: usize) -> Result<(), LowerError> {
        let mut i = start;
        while i < end {
            if self.processed[i] {
                i += 1;
                continue;
            }
            self.processed[i] = true;
            
            match &self.func.body[i] {
                Op::IfStart { cond, else_offset, end_offset } => {
                    let else_label = self.alloc_label();
                    let end_label = self.alloc_label();
                    
                    // Branch to else if condition is false
                    self.out.push(VInst::BrIf {
                        cond: *cond,
                        target: else_label,
                        invert: true, // branch when cond == 0
                        src_op: Some(i as u32),
                    });
                    
                    // Then block: from i+1 to else_offset
                    self.lower_block(i + 1, *else_offset as usize)?;
                    
                    // Jump over else block
                    self.out.push(VInst::Br {
                        target: end_label,
                        src_op: Some(i as u32),
                    });
                    
                    // Else label and block
                    self.out.push(VInst::Label(else_label, Some(*else_offset)));
                    self.processed[*else_offset as usize] = true; // mark Else as processed
                    self.lower_block((*else_offset + 1) as usize, *end_offset as usize)?;
                    
                    // End label
                    self.out.push(VInst::Label(end_label, Some(*end_offset)));
                    self.processed[*end_offset as usize] = true; // mark End as processed
                    
                    i = *end_offset as usize + 1;
                }
                
                Op::Else | Op::End => {
                    // These are handled by IfStart processing
                    i += 1;
                }
                
                Op::BrIfNot { cond } => {
                    // Branch to end of innermost loop if condition false
                    // For now, if we're not in a loop, this is an error or unreachable
                    // TODO: track loop context
                    return Err(LowerError::UnsupportedOp {
                        description: String::from("BrIfNot outside loop (loops not yet supported)"),
                    });
                }
                
                other => {
                    // Normal op - lower directly
                    self.out.push(lower_op(other, self.float_mode, Some(i as u32), self.func)?);
                    i += 1;
                }
            }
        }
        Ok(())
    }
}
```

### 3. Update `lower_op`

Ensure `lower_op` returns `Err` for control flow ops it shouldn't see:

```rust
Op::IfStart { .. } | Op::Else | Op::End | Op::BrIfNot { .. } => {
    Err(LowerError::Internal(String::from(
        "control flow op should be handled by lower_block, not lower_op"
    )))
}
```

### 4. Handle nested if/else

The recursive `lower_block` call handles this naturally:
- `IfStart` in then-block → recursive call for nested if
- Labels are allocated per-IfStart, so nested ifs get unique labels

### 5. Simplify: Don't track processed flags

Alternative: Don't use `processed` flags. Instead, trust the offsets from IfStart and just process linearly with a while loop that jumps forward after handling IfStart.

Simpler approach:
```rust
fn lower_ops(func: &IrFunction, float_mode: FloatMode) -> Result<Vec<VInst>, LowerError> {
    let mut out = Vec::with_capacity(func.body.len() * 2);
    let mut i = 0;
    let mut next_label = 0;
    
    while i < func.body.len() {
        match &func.body[i] {
            Op::IfStart { cond, else_offset, end_offset } => {
                let else_label = next_label;
                let end_label = next_label + 1;
                next_label += 2;
                
                // Branch to else if false
                out.push(VInst::BrIf {
                    cond: *cond,
                    target: else_label,
                    invert: true,
                    src_op: Some(i as u32),
                });
                
                // Then block ops (i+1 .. else_offset)
                for j in (i + 1)..(*else_offset as usize) {
                    out.push(lower_op(&func.body[j], float_mode, Some(j as u32), func)?);
                }
                
                // Jump over else
                out.push(VInst::Br {
                    target: end_label,
                    src_op: Some(i as u32),
                });
                
                // Else label
                out.push(VInst::Label(else_label, Some(*else_offset)));
                
                // Else block ops (else_offset+1 .. end_offset)
                for j in ((*else_offset + 1) as usize)..(*end_offset as usize) {
                    out.push(lower_op(&func.body[j], float_mode, Some(j as u32), func)?);
                }
                
                // End label
                out.push(VInst::Label(end_label, Some(*end_offset)));
                
                i = *end_offset as usize + 1;
            }
            
            Op::Else | Op::End => {
                // Skip - processed as part of IfStart
                i += 1;
            }
            
            other => {
                out.push(lower_op(other, float_mode, Some(i as u32), func)?);
                i += 1;
            }
        }
    }
    
    Ok(out)
}
```

This is much simpler and doesn't need recursion or processed flags. It handles nested ifs because when processing the then-block, if we encounter another `IfStart`, we handle it the same way.

Let's go with this simpler approach.

### 6. Problem: Nested IfStart in then-block

The simple loop has a problem: when we do the inner `for` loop for the then-block:
```rust
for j in (i + 1)..(*else_offset as usize) {
    out.push(lower_op(...)?);
}
```

If there's a nested `IfStart` in that range, `lower_op` will fail (it doesn't handle control flow). We need to make this recursive.

### 7. Final approach: Recursive helper

```rust
pub fn lower_ops(func: &IrFunction, float_mode: FloatMode) -> Result<Vec<VInst>, LowerError> {
    let mut ctx = LowerCtx {
        func,
        float_mode,
        out: Vec::with_capacity(func.body.len() * 2),
        next_label: 0,
    };
    ctx.lower_range(0, func.body.len())?;
    Ok(ctx.out)
}

struct LowerCtx<'a> {
    func: &'a IrFunction,
    float_mode: FloatMode,
    out: Vec<VInst>,
    next_label: LabelId,
}

impl<'a> LowerCtx<'a> {
    fn alloc_label(&mut self) -> LabelId {
        let id = self.next_label;
        self.next_label += 1;
        id
    }
    
    fn lower_range(&mut self, start: usize, end: usize) -> Result<(), LowerError> {
        let mut i = start;
        while i < end {
            match &self.func.body[i] {
                Op::IfStart { cond, else_offset, end_offset } => {
                    let else_label = self.alloc_label();
                    let end_label = self.alloc_label();
                    
                    // Branch to else if condition is false
                    self.out.push(VInst::BrIf {
                        cond: *cond,
                        target: else_label,
                        invert: true,
                        src_op: Some(i as u32),
                    });
                    
                    // Recursively lower then block
                    self.lower_range(i + 1, *else_offset as usize)?;
                    
                    // Jump over else
                    self.out.push(VInst::Br {
                        target: end_label,
                        src_op: Some(i as u32),
                    });
                    
                    // Else label
                    self.out.push(VInst::Label(else_label, Some(*else_offset)));
                    
                    // Recursively lower else block
                    self.lower_range((*else_offset + 1) as usize, *end_offset as usize)?;
                    
                    // End label
                    self.out.push(VInst::Label(end_label, Some(*end_offset)));
                    
                    i = *end_offset as usize + 1;
                }
                
                Op::Else | Op::End => {
                    // These are structural markers, skip
                    i += 1;
                }
                
                other => {
                    self.out.push(lower_op(other, self.float_mode, Some(i as u32), self.func)?);
                    i += 1;
                }
            }
        }
        Ok(())
    }
}
```

This handles nested ifs correctly through recursion.

## Tests

Add tests in `lower.rs`:

```rust
#[test]
fn lower_simple_if_else() {
    let func = IrFunction {
        name: String::from("test"),
        is_entry: true,
        vmctx_vreg: VReg(0),
        param_count: 0,
        return_types: vec![IrType::I32],
        vreg_types: vec![IrType::I32, IrType::I32, IrType::Bool],
        slots: vec![],
        body: vec![
            // v0 = param, v1 = result, v2 = condition
            Op::IconstI32 { dst: v(2), value: 1 }, // condition = true
            Op::IfStart { cond: v(2), else_offset: 3, end_offset: 6 },
            Op::IconstI32 { dst: v(1), value: 10 }, // then: result = 10
            Op::Copy { dst: v(0), src: v(1) },
            Op::Else,
            Op::IconstI32 { dst: v(1), value: 20 }, // else: result = 20
            Op::Copy { dst: v(0), src: v(1) },
            Op::End,
            Op::Return { values: VRegRange { start: 0, count: 1 } },
        ],
        vreg_pool: vec![v(0)],
    };
    
    let vinsts = lower_ops(&func, FloatMode::Q32).expect("lower ok");
    
    // Check structure: BrIf, then-body, Br, Label, else-body, Label
    assert!(matches!(vinsts[0], VInst::BrIf { invert: true, .. }));
    assert!(matches!(vinsts[3], VInst::Br { .. }));
    assert!(matches!(vinsts[4], VInst::Label(0, _)));
    assert!(matches!(vinsts[7], VInst::Label(1, _)));
}
```

## Validate

```bash
cargo test -p lpvm-native lower_if
cargo test -p lpvm-native
```

Expected: New tests pass, existing tests still pass.
