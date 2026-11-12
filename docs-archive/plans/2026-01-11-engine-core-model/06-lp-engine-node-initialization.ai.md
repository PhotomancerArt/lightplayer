# Phase 6: lp-engine - Node Initialization (Minimal)

## Goal

Implement node initialization in `ProjectRuntime`. Initialize nodes in order (textures → shaders → fixtures → outputs) and store runtimes in entries.

## Dependencies

- `lp-engine` Phase 5

## Implementation

### Update ProjectRuntime

**File**: Update `lp-engine/src/project/runtime.rs`

```rust
use crate::error::Error;
use crate::nodes::{
    NodeRuntime, TextureRuntime, ShaderRuntime, OutputRuntime, FixtureRuntime,
};
use crate::runtime::contexts::{NodeInitContext, RenderContext};
use lp_model::{
    FrameId, LpPath, NodeConfig, NodeHandle, NodeKind, ProjectConfig,
};
use lp_shared::fs::LpFs;
use alloc::collections::BTreeMap;
use alloc::boxed::Box;

// ... existing ProjectRuntime and NodeEntry definitions ...

impl ProjectRuntime {
    // ... existing methods ...

    /// Initialize all nodes in dependency order
    pub fn initialize_nodes(&mut self) -> Result<(), Error> {
        // Initialize in order: textures → shaders → fixtures → outputs
        let init_order = [
            NodeKind::Texture,
            NodeKind::Shader,
            NodeKind::Fixture,
            NodeKind::Output,
        ];

        for kind in init_order.iter() {
            let handles: Vec<NodeHandle> = self.nodes
                .iter()
                .filter(|(_, entry)| entry.kind == *kind && entry.status == NodeStatus::Created)
                .map(|(handle, _)| *handle)
                .collect();

            for handle in handles {
                if let Some(entry) = self.nodes.get_mut(&handle) {
                    // Create runtime based on kind
                    let mut runtime: Box<dyn NodeRuntime> = match entry.kind {
                        NodeKind::Texture => Box::new(TextureRuntime::new()),
                        NodeKind::Shader => Box::new(ShaderRuntime::new()),
                        NodeKind::Output => Box::new(OutputRuntime::new()),
                        NodeKind::Fixture => Box::new(FixtureRuntime::new()),
                    };

                    // Create init context (stub for now)
                    let ctx = InitContext {
                        fs: self.fs.as_ref(),
                        // todo!("Add node resolution to context")
                    };

                    // Initialize
                    match runtime.init(&ctx) {
                        Ok(()) => {
                            entry.status = NodeStatus::Ok;
                            entry.runtime = Some(runtime);
                        }
                        Err(e) => {
                            entry.status = NodeStatus::InitError(format!("{}", e));
                            entry.runtime = None;
                        }
                    }
                }
            }
        }

        Ok(())
    }
}

/// Stub init context implementation
struct InitContext<'a> {
    fs: &'a dyn LpFs,
}

impl<'a> NodeInitContext for InitContext<'a> {
    fn get_node_fs(&self) -> &dyn LpFs {
        self.fs
    }

    // Other methods use default todo!() implementations
}
```

**File**: Update `lp-engine/src/runtime/contexts.rs` to make methods have default implementations that can be overridden:

```rust
// ... existing code ...

impl NodeInitContext for InitContext<'_> {
    fn get_node_fs(&self) -> &dyn LpFs {
        self.fs
    }
}
```

## Success Criteria

- All code compiles
- Can call `initialize_nodes()` on `ProjectRuntime`
- Nodes are initialized in correct order
- Failed initializations set `InitError` status but keep entry
- Successful initializations store runtime and set `Ok` status

## Tests

- Create project with nodes of each type
- Call `load_nodes()` then `initialize_nodes()`
- Verify all nodes have status `Ok` or `InitError` (not `Created`)
- Verify runtimes are stored for successful nodes

## Notes

- Init context is a stub - node resolution will be added later
- Initialization order is hardcoded - matches dependency order
- Errors don't stop initialization of other nodes ("show must go on")
