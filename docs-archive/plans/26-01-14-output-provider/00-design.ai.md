# Output Provider System - Design

## File Structure

```
lp-app/crates/lp-engine/src/
├── output/
│   ├── mod.rs
│   ├── provider.rs          # OutputProvider trait, OutputFormat enum, OutputChannelHandle
│   └── memory.rs            # MemoryOutputProvider implementation
├── runtime/
│   └── contexts.rs          # Add output_provider() methods to traits
├── nodes/
│   └── output/
│       └── runtime.rs       # Update OutputRuntime to use provider
└── project/
    └── runtime.rs           # Add Arc<dyn OutputProvider> field, pass to contexts
```

## Type Tree

### Core Types

**`output/provider.rs`**:
- `pub trait OutputProvider`: 
  - `fn open(&self, pin: u32, byte_count: u32, format: OutputFormat) -> Result<OutputChannelHandle, Error>`
  - `fn write(&self, handle: OutputChannelHandle, data: &[u8]) -> Result<(), Error>`
  - `fn close(&self, handle: OutputChannelHandle) -> Result<(), Error>`
- `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum OutputFormat`:
  - `Ws2811` (only variant for now)
- `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)] pub struct OutputChannelHandle(i32)`:
  - `pub fn new(id: i32) -> Self`
  - `pub fn as_i32(&self) -> i32`
  - Allows -1 for invalid/None handle

**`output/memory.rs`**:
- `pub struct MemoryOutputProvider`:
  - `channels: BTreeMap<OutputChannelHandle, ChannelState>`
  - `next_handle: i32`
  - `open_pins: BTreeSet<u32>` (prevents duplicate opens on same pin)
- `struct ChannelState`:
  - `pin: u32`
  - `byte_count: u32`
  - `format: OutputFormat`
  - `data: Vec<u8>` (for testing - stores last written data)
- `impl OutputProvider for MemoryOutputProvider`:
  - `open()`: Validates pin not already open, creates handle, stores state
  - `write()`: Validates handle exists, validates data length, stores data
  - `close()`: Removes handle and pin from tracking
- `impl MemoryOutputProvider`:
  - `pub fn new() -> Self`
  - `pub fn get_data(&self, handle: OutputChannelHandle) -> Option<&[u8]>` (for testing)

### Integration Types

**`runtime/contexts.rs`**:
- `trait NodeInitContext`:
  - `fn output_provider(&self) -> &dyn OutputProvider;` (new)
- `trait RenderContext`:
  - `fn output_provider(&self) -> &dyn OutputProvider;` (new)

**`project/runtime.rs`**:
- `pub struct ProjectRuntime`:
  - `output_provider: Arc<dyn OutputProvider>` (new field)
- `impl ProjectRuntime`:
  - `pub fn new(fs: Box<dyn LpFs>, output_provider: Arc<dyn OutputProvider>) -> Result<Self, Error>` (updated signature)
- `struct NodeInitContextImpl`:
  - `output_provider: Arc<dyn OutputProvider>` (new field)
- `struct RenderContextImpl`:
  - `output_provider: Arc<dyn OutputProvider>` (new field)

**`nodes/output/runtime.rs`**:
- `pub struct OutputRuntime`:
  - `channel_data: Vec<u8>` (existing)
  - `channel_handle: Option<OutputChannelHandle>` (new)
- `impl NodeRuntime for OutputRuntime`:
  - `init()`: Extract pin from config, calculate byte_count (from fixtures or default), call `ctx.output_provider().open()`, store handle, allocate buffer
  - `render()`: If handle exists, call `ctx.output_provider().write(handle, &channel_data)`
  - `destroy()`: If handle exists, call `ctx.output_provider().close(handle)`

## Design Notes

### Byte Count Calculation

For now, `OutputRuntime::init()` will calculate `byte_count` by:
1. Finding all fixtures that reference this output
2. Finding the maximum `start_ch + ch_count` across all fixtures
3. Using that as `byte_count`, or a default minimum (e.g., 3 bytes for single RGB pixel)

**Future**: `OutputConfig` should have a `byte_count` or `channel_count` field for explicit configuration.

### Format Default

For now, `OutputFormat::Ws2811` will be hardcoded in `OutputRuntime::init()`. 

**Future**: `OutputConfig` should have a `format` field.

### Error Handling

- `open()` returns error if pin already open
- `write()` returns error if handle invalid or data length mismatch
- `close()` returns error if handle invalid
- All errors use `crate::error::Error`

### Testing

- `MemoryOutputProvider` stores written data for verification
- End-to-end test will verify data flows to memory provider
- Test helpers can query `MemoryOutputProvider::get_data()` to check output values

## Implementation Order

1. Create `output/provider.rs` with trait, enum, handle type
2. Create `output/memory.rs` with in-memory implementation
3. Update `runtime/contexts.rs` to add provider access methods
4. Update `project/runtime.rs` to store and pass provider
5. Update `nodes/output/runtime.rs` to integrate with provider
6. Update end-to-end test to use `MemoryOutputProvider` and verify output
7. Update `ProjectBuilder` if needed for test setup
