# Phase 2: Update Handle Types and Add Node Resolution

## Goal

Update `TextureHandle`/`OutputHandle` to wrap `NodeHandle`, and implement `InitContext` with node resolution and filesystem access.

## Dependencies

- Phase 1

## Implementation

### 1. Update Handle Types

**File**: `lp-engine/src/runtime/contexts.rs`

```rust
pub struct TextureHandle(NodeHandle);  // Changed from u32
pub struct OutputHandle(NodeHandle);   // Changed from u32

impl TextureHandle {
    pub fn new(handle: NodeHandle) -> Self {
        Self(handle)
    }
    
    pub fn as_node_handle(&self) -> NodeHandle {
        self.0
    }
}

impl OutputHandle {
    pub fn new(handle: NodeHandle) -> Self {
        Self(handle)
    }
    
    pub fn as_node_handle(&self) -> NodeHandle {
        self.0
    }
}
```

### 2. Implement InitContext

**File**: `lp-engine/src/project/runtime.rs`

```rust
pub struct InitContext<'a> {
    runtime: &'a ProjectRuntime,
    node_path: &'a LpPath,
    node_fs: Option<Box<dyn LpFs>>,  // Cached chroot filesystem
}

impl<'a> InitContext<'a> {
    pub fn new(runtime: &'a ProjectRuntime, node_path: &'a LpPath) -> Self {
        Self {
            runtime,
            node_path,
            node_fs: None,
        }
    }
}

impl<'a> NodeInitContext for InitContext<'a> {
    fn resolve_node(&self, spec: &NodeSpecifier) -> Result<NodeHandle, Error> {
        let spec_path = spec.as_str();
        let node_path = if spec_path.starts_with('/') {
            // Absolute path
            LpPath::from(spec_path)
        } else {
            // Relative path - resolve from node directory
            // For now, assume relative paths are not supported (todo!)
            return Err(Error::NotFound {
                path: spec_path.to_string(),
            });
        };
        
        // Look up node by path
        for (handle, entry) in &self.runtime.nodes {
            if entry.path == node_path {
                return Ok(*handle);
            }
        }
        
        Err(Error::NotFound {
            path: spec_path.to_string(),
        })
    }
    
    fn resolve_output(&self, spec: &NodeSpecifier) -> Result<OutputHandle, Error> {
        let handle = self.resolve_node(spec)?;
        let entry = self.runtime.nodes.get(&handle)
            .ok_or_else(|| Error::NotFound {
                path: spec.as_str().to_string(),
            })?;
        
        if entry.kind != NodeKind::Output {
            return Err(Error::WrongNodeKind {
                specifier: spec.as_str().to_string(),
                expected: NodeKind::Output,
                actual: entry.kind,
            });
        }
        
        Ok(OutputHandle::new(handle))
    }
    
    fn resolve_texture(&self, spec: &NodeSpecifier) -> Result<TextureHandle, Error> {
        let handle = self.resolve_node(spec)?;
        let entry = self.runtime.nodes.get(&handle)
            .ok_or_else(|| Error::NotFound {
                path: spec.as_str().to_string(),
            })?;
        
        if entry.kind != NodeKind::Texture {
            return Err(Error::WrongNodeKind {
                specifier: spec.as_str().to_string(),
                expected: NodeKind::Texture,
                actual: entry.kind,
            });
        }
        
        Ok(TextureHandle::new(handle))
    }
    
    fn get_node_fs(&self) -> &dyn LpFs {
        // Lazy initialization of chroot filesystem
        if self.node_fs.is_none() {
            // Create chroot at node directory
            // Note: This requires mutable access, so we'll need to use interior mutability
            // or create it in new() and store it
            // For now, return the base filesystem (will fix in implementation)
            todo!("Implement chroot filesystem")
        }
        // Return cached filesystem
        self.node_fs.as_ref().unwrap().as_ref()
    }
}
```

**Note**: The chroot filesystem caching requires interior mutability. We'll use `RefCell` or create it in `new()`.

### 3. Update ProjectRuntime::initialize_nodes()

**File**: `lp-engine/src/project/runtime.rs`

```rust
impl ProjectRuntime {
    pub fn initialize_nodes(&mut self) -> Result<(), Error> {
        // ... existing code ...
        
        for handle in handles {
            if let Some(entry) = self.nodes.get_mut(&handle) {
                // ... create runtime ...
                
                // Create init context with node path
                let node_path = &entry.path;
                let ctx = InitContext::new(self, node_path);
                
                // Initialize
                match runtime.init(&ctx) {
                    // ... existing code ...
                }
            }
        }
        
        Ok(())
    }
}
```

### 4. Fix Chroot Filesystem

**File**: `lp-engine/src/project/runtime.rs`

```rust
use alloc::cell::RefCell;

pub struct InitContext<'a> {
    runtime: &'a ProjectRuntime,
    node_path: &'a LpPath,
    node_fs: RefCell<Option<Box<dyn LpFs>>>,
}

impl<'a> InitContext<'a> {
    pub fn new(runtime: &'a ProjectRuntime, node_path: &'a LpPath) -> Self {
        Self {
            runtime,
            node_path,
            node_fs: RefCell::new(None),
        }
    }
}

impl<'a> NodeInitContext for InitContext<'a> {
    fn get_node_fs(&self) -> &dyn LpFs {
        let mut fs_opt = self.node_fs.borrow_mut();
        if fs_opt.is_none() {
            // Create chroot at node directory
            let node_dir = self.node_path.as_str();
            *fs_opt = Some(self.runtime.fs.chroot(node_dir)
                .map_err(|e| Error::Io {
                    path: node_dir.to_string(),
                    details: format!("Failed to chroot: {:?}", e),
                })?);
        }
        // Return reference (this is safe because we're borrowing from RefCell)
        // Note: This requires unsafe or a different approach
        // Actually, we can't return &dyn LpFs from RefCell easily
        // Better approach: create in new() and store as Box
        todo!("Fix chroot filesystem access")
    }
}
```

Actually, better approach - create filesystem in `new()`:

```rust
impl<'a> InitContext<'a> {
    pub fn new(runtime: &'a ProjectRuntime, node_path: &'a LpPath) -> Result<Self, Error> {
        let node_dir = node_path.as_str();
        let node_fs = runtime.fs.chroot(node_dir)
            .map_err(|e| Error::Io {
                path: node_dir.to_string(),
                details: format!("Failed to chroot: {:?}", e),
            })?;
        
        Ok(Self {
            runtime,
            node_path,
            node_fs,
        })
    }
}

pub struct InitContext<'a> {
    runtime: &'a ProjectRuntime,
    node_path: &'a LpPath,
    node_fs: Box<dyn LpFs>,  // Owned chroot filesystem
}

impl<'a> NodeInitContext for InitContext<'a> {
    fn get_node_fs(&self) -> &dyn LpFs {
        self.node_fs.as_ref()
    }
}
```

## Success Criteria

- All code compiles
- `TextureHandle`/`OutputHandle` wrap `NodeHandle` correctly
- `resolve_node()` finds nodes by path
- `resolve_output()`/`resolve_texture()` check node kind and return appropriate handles
- `get_node_fs()` returns chroot filesystem for node directory
- Tests pass

## Notes

- Handle types now provide type safety for texture vs output lookups
- Node resolution looks up by path in `ProjectRuntime.nodes`
- Chroot filesystem created once per `InitContext` and cached
- Relative paths not supported yet (todo!)
