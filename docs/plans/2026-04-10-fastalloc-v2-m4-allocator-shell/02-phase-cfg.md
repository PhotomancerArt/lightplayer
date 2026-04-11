# Phase 2: Region Tree CFG and Display

## Scope

Replace flat CFG blocks with a **region tree** structure built during lowering. The tree represents structured control flow (linear, if/then/else, loops) and enables recursive liveness without fixed-point iteration.

## Implementation

### 1. Add `Region` enum to lowerer

In `lp-shader/lpvm-native/src/lower.rs`, add the region tree alongside `LoweredFunction`:

```rust
/// Region of VInsts with structured control flow.
/// All indices are into the flat VInst slice (no copies).
pub enum Region {
    /// Linear VInst range [start..end), no internal branches
    Linear { start: u16, end: u16 },
    
    /// if (BrIf at head_end-1) { then_body } else { else_body }
    IfThenElse {
        head: Box<Region>,       // ends with BrIf
        then_body: Box<Region>,
        else_body: Box<Region>,  // may be Linear(n,n) if empty
    },
    
    /// loop { header; body; back-edge }
    Loop {
        header: Box<Region>,
        body: Box<Region>,
    },
    
    /// Sequential composition (for regions that don't fit above)
    Seq(Vec<Region>),
}

/// Updated LoweredFunction with region tree.
pub struct LoweredFunction {
    pub vinsts: Vec<VInst>,
    pub region: Region,        // NEW: structured overlay
    pub loop_regions: Vec<LoopRegion>,
}
```

### 2. Build regions during lowering

Modify `LowerCtx::lower_range()` to track regions:

```rust
struct LowerCtx<'a> {
    func: &'a IrFunction,
    ir: &'a IrModule,
    abi: &'a ModuleAbi,
    float_mode: FloatMode,
    out: Vec<VInst>,
    regions: Vec<Region>,    // NEW: stack of regions being built
    // ...
}

impl<'a> LowerCtx<'a> {
    /// Build region tree from LPIR ops in [start..end).
    /// Returns the region covering this range.
    fn lower_range_with_region(&mut self, start: usize, end: usize) 
        -> Result<Region, LowerError> {
        let mut i = start;
        let mut seq = Vec::new();
        
        while i < end {
            match &self.func.body[i] {
                Op::IfStart { cond, else_offset, end_offset } => {
                    let eo = *else_offset as usize;
                    let merge = *end_offset as usize;
                    
                    // Head: BrIf
                    let head_start = self.out.len() as u16;
                    self.out.push(VInst::BrIf { 
                        cond: *cond, 
                        target: 0,  // filled later
                        invert: true,
                        src_op: Some(i as u32),
                    });
                    let head_end = self.out.len() as u16;
                    let head = Region::Linear { start: head_start, end: head_end };
                    
                    // Then body
                    let then_start = self.out.len() as u16;
                    let then_region = self.lower_range_with_region(i + 1, eo)?;
                    let then_end = self.out.len() as u16;
                    
                    // Else body (may be empty)
                    let else_region = if eo < merge {
                        self.lower_range_with_region(eo + 1, merge)?
                    } else {
                        Region::Linear { start: then_end, end: then_end }
                    };
                    
                    let region = Region::IfThenElse {
                        head: Box::new(head),
                        then_body: Box::new(then_region),
                        else_body: Box::new(else_region),
                    };
                    seq.push(region);
                    i = merge;
                }
                
                Op::LoopStart { continuing_offset, end_offset } => {
                    let co = *continuing_offset as usize;
                    let eo = *end_offset as usize;
                    
                    let header_start = self.out.len() as u16;
                    self.out.push(VInst::Label(0, Some((i + 1) as u32)));
                    let header_end = self.out.len() as u16;
                    
                    let body_start = self.out.len() as u16;
                    let body = self.lower_range_with_region(i + 1, co)?;
                    let body_end = self.out.len() as u16;
                    
                    // Record loop metadata for back-edge
                    self.loop_regions.push(LoopRegion {
                        header_idx: header_start as usize,
                        backedge_idx: self.out.len(),
                    });
                    
                    // Back-edge branch
                    self.out.push(VInst::Br {
                        target: 0,
                        src_op: Some((eo - 1) as u32),
                    });
                    self.out.push(VInst::Label(0, Some(*end_offset)));
                    
                    let region = Region::Loop {
                        header: Box::new(Region::Linear { 
                            start: header_start, 
                            end: header_end 
                        }),
                        body: Box::new(body),
                    };
                    seq.push(region);
                    i = eo;
                }
                
                _ => {
                    // Single instruction - accumulate into current linear region
                    let inst_start = self.out.len() as u16;
                    self.lower_op_at(i)?;
                    let inst_end = self.out.len() as u16;
                    seq.push(Region::Linear { start: inst_start, end: inst_end });
                    i += 1;
                }
            }
        }
        
        // Coalesce consecutive Linears in seq
        Ok(coalesce_linears(seq))
    }
}

/// Merge consecutive Linear regions.
fn coalesce_linears(seq: Vec<Region>) -> Region {
    if seq.is_empty() {
        return Region::Linear { start: 0, end: 0 };
    }
    if seq.len() == 1 {
        return seq.into_iter().next().unwrap();
    }
    
    let mut merged = Vec::new();
    let mut current: Option<(u16, u16)> = None;
    
    for r in seq {
        match r {
            Region::Linear { start, end } => {
                if let Some((cs, ce)) = current {
                    if ce == start {
                        current = Some((cs, end));
                    } else {
                        merged.push(Region::Linear { start: cs, end: ce });
                        current = Some((start, end));
                    }
                } else {
                    current = Some((start, end));
                }
            }
            non_linear => {
                if let Some((cs, ce)) = current {
                    merged.push(Region::Linear { start: cs, end: ce });
                    current = None;
                }
                merged.push(non_linear);
            }
        }
    }
    
    if let Some((cs, ce)) = current {
        merged.push(Region::Linear { start: cs, end: ce });
    }
    
    if merged.len() == 1 {
        merged.into_iter().next().unwrap()
    } else {
        Region::Seq(merged)
    }
}
```

### 3. Update `LoweredFunction` return

In `lower_ops()`:

```rust
pub fn lower_ops(...) -> Result<LoweredFunction, LowerError> {
    let mut ctx = LowerCtx::new(func, ir, abi, float_mode);
    let region = ctx.lower_range_with_region(0, func.body.len())?;
    
    Ok(LoweredFunction {
        vinsts: ctx.out,
        region,
        loop_regions: ctx.loop_regions,
    })
}
```

### 4. Add `format_region()` for debug display

In `lp-shader/lpvm-native/src/isa/rv32fa/debug/region.rs`:

```rust
use crate::lower::Region;
use crate::debug::vinst;

/// Format region tree for debug output.
pub fn format_region(region: &Region, vinsts: &[VInst], indent: usize) -> String {
    let mut lines = Vec::new();
    let prefix = "  ".repeat(indent);
    
    match region {
        Region::Linear { start, end } => {
            lines.push(format!("{}Linear [{}..{}]", prefix, start, end));
            for i in *start..*end {
                let vinst_text = vinst::format_vinst(&vinsts[i as usize]);
                lines.push(format!("{}  {}: {}", prefix, i, vinst_text));
            }
        }
        
        Region::IfThenElse { head, then_body, else_body } => {
            lines.push(format!("{}IfThenElse", prefix));
            lines.push(format!("{}  head:", prefix));
            lines.push(format_region(head, vinsts, indent + 2));
            lines.push(format!("{}  then:", prefix));
            lines.push(format_region(then_body, vinsts, indent + 2));
            lines.push(format!("{}  else:", prefix));
            lines.push(format_region(else_body, vinsts, indent + 2));
        }
        
        Region::Loop { header, body } => {
            lines.push(format!("{}Loop", prefix));
            lines.push(format!("{}  header:", prefix));
            lines.push(format_region(header, vinsts, indent + 2));
            lines.push(format!("{}  body:", prefix));
            lines.push(format_region(body, vinsts, indent + 2));
        }
        
        Region::Seq(regions) => {
            lines.push(format!("{}Seq", prefix));
            for r in regions {
                lines.push(format_region(r, vinsts, indent + 1));
            }
        }
    }
    
    lines.join("\n")
}
```

### 5. Wire into CLI

Update `lp-cli/src/commands/shader_rv32fa/`:

```rust
// In handler.rs, when --show-region is passed:
let lowered = lower_ops(&func, &ir, &abi, float_mode)?;
let region_text = lpvm_native::debug::region::format_region(
    &lowered.region, 
    &lowered.vinsts,
    0
);
println!("=== Region Tree ===\n{}", region_text);
```

### 6. Add unit tests

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lower::{lower_ops, Region};
    use crate::test_util::simple_module;
    
    #[test]
    fn test_region_linear() {
        let vinsts = vec![
            VInst::IConst32 { dst: VReg(0), val: 42, src_op: None },
            VInst::Ret { vals: vec![VReg(0)], src_op: None },
        ];
        
        let region = Region::Linear { start: 0, end: 2 };
        assert!(matches!(region, Region::Linear { .. }));
    }
    
    #[test]
    fn test_region_if_then_else() {
        // Lower a simple if/then/else GLSL function
        let glsl = r#"
            int test(int x) {
                if (x > 0) return 1;
                else return 0;
            }
        "#;
        
        let lowered = lower_simple_glsl(glsl).unwrap();
        
        // Should have IfThenElse region
        assert!(matches!(&lowered.region, Region::IfThenElse { .. }),
            "Expected IfThenElse, got {:?}", lowered.region);
    }
    
    #[test]
    fn test_region_loop() {
        let glsl = r#"
            int test() {
                int sum = 0;
                for (int i = 0; i < 10; i++) {
                    sum += i;
                }
                return sum;
            }
        "#;
        
        let lowered = lower_simple_glsl(glsl).unwrap();
        
        // Should have Loop region somewhere in the tree
        fn has_loop(r: &Region) -> bool {
            match r {
                Region::Loop { .. } => true,
                Region::IfThenElse { head, then_body, else_body } => {
                    has_loop(head) || has_loop(then_body) || has_loop(else_body)
                }
                Region::Loop { header, body } => {
                    has_loop(header) || has_loop(body)
                }
                Region::Seq(seq) => seq.iter().any(has_loop),
                _ => false,
            }
        }
        
        assert!(has_loop(&lowered.region), "Expected Loop in region tree");
    }
}
```

## Memory Characteristics

| Aspect | Region Tree | Flat CFG (old) |
|--------|-------------|----------------|
| VInst copies | 0 (indices only) | 1+ per block |
| Liveness | Recursive descent | Fixed-point iteration |
| Allocator walk | Tree traversal | Graph traversal |
| Heap pressure | ~4 bytes per region (indices) | ~40 bytes per block (Vecs) |
| Build complexity | During lowering (free) | Separate pass |

## Validate

```bash
# Test region building
cargo test -p lpvm-native --lib -- lower::tests

# Test formatting
cargo test -p lpvm-native --lib -- debug::region

# CLI with region display
cargo run --bin lp-cli -- shader-rv32fa test.glsl --show-region
```
