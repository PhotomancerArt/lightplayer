# Output Provider System - Review Questions

## Overview

This document contains review questions for the Output Provider System design plan. The plan is missing details about listing operations needed for the `OutputProvider` trait.

## Questions

### 1. Listing Operations - Core Trait Methods

**Question**: Should the `OutputProvider` trait include methods for listing/enumerating open channels?

**Context**: 
- The `MemoryOutputProvider` implementation has helper methods like `get_all_handles()`, `get_handle_for_pin()`, `is_pin_open()`, etc., but these are NOT part of the trait
- The plan doesn't specify what listing operations are needed or whether they should be part of the trait API

**Options**:
- **Option A**: Add listing methods to the trait (e.g., `list_channels()`, `get_channel_info(handle)`)
- **Option B**: Keep listing methods as implementation-specific helpers (not in trait)
- **Option C**: Add a separate trait for querying (e.g., `OutputProviderQuery`)

**What information should listing operations provide?**
- List all open handles?
- Get channel info (pin, byte_count, format) for a handle?
- Check if a pin is open?
- Get handle for a pin?
- Other?

---

### 2. Listing Operations - Return Types

**Question**: If listing operations are added to the trait, what should they return?

**Considerations**:
- Should they return iterators or owned collections?
- What information should be included (handle, pin, byte_count, format)?
- Should there be a `ChannelInfo` struct to bundle this information?

**Example structures**:
```rust
pub struct ChannelInfo {
    pub handle: OutputChannelHandle,
    pub pin: u32,
    pub byte_count: u32,
    pub format: OutputFormat,
}
```

---

### 3. Listing Operations - Use Cases

**Question**: What are the use cases for listing operations?

**Potential use cases**:
- Debugging/monitoring: See what channels are open
- State inspection: Query channel information
- Testing: Verify expected channels are open
- Error recovery: List channels before cleanup
- API/Server: Expose channel status to clients

**Which use cases are important enough to require trait methods vs implementation-specific helpers?**

---

### 4. Tenant-Home Reference

**Question**: What is "tenant-home" that should be reviewed?

**Context**: The review request mentioned reviewing "tenant-home" but this doesn't appear in the codebase. Is this:
- A directory or file that should exist?
- A concept/pattern that should be documented?
- A typo or shorthand for something else?
- Something that needs to be created?

---

### 5. Channel Information Access

**Question**: Should there be a way to query channel information (pin, byte_count, format) given a handle?

**Context**: 
- Currently, `write()` validates handle exists but doesn't expose channel info
- `MemoryOutputProvider` stores this info but doesn't expose it via trait
- Useful for debugging, validation, or higher-level APIs

**Options**:
- Add `get_channel_info(handle) -> Option<ChannelInfo>` to trait
- Keep it implementation-specific
- Add it only if needed for specific use cases

---

### 6. Pin-to-Handle Resolution

**Question**: Should there be a way to get a handle for a given pin?

**Context**:
- `MemoryOutputProvider` has `get_handle_for_pin()` as a helper
- Useful for: checking if pin is already open, getting handle when you only know the pin
- But pins are unique per provider, so this might be implementation-specific

**Should this be in the trait or implementation-specific?**

---

### 7. Error Handling for Listing Operations

**Question**: How should listing operations handle errors?

**Considerations**:
- Should they return `Result` or `Option`?
- What errors are possible (e.g., provider not initialized, handle invalid)?
- Should listing operations be infallible (always return empty list on error)?

---

### 8. Iterator vs Collection Return Types

**Question**: Should listing methods return iterators or owned collections?

**Considerations**:
- Iterators are more flexible and can be lazy
- Collections are simpler but require allocation
- Trait object limitations: can't return generic iterators easily
- Could use `Box<dyn Iterator<Item = ...>>` or return `Vec`

**Preference**: Iterator (with trait object) or `Vec`?

---

### 9. Thread Safety Considerations

**Question**: Are listing operations needed in multi-threaded contexts?

**Context**:
- `OutputProvider` trait uses `&self` (immutable reference)
- `MemoryOutputProvider` uses `RefCell` for interior mutability
- Hardware providers might need different synchronization

**Do listing operations need to be thread-safe, or is single-threaded access sufficient?**

---

### 10. Future Hardware Provider Compatibility

**Question**: Will listing operations be feasible/necessary for hardware providers (ESP32)?

**Considerations**:
- Hardware providers might maintain state differently
- Some operations might be expensive or impossible on hardware
- Should trait methods be optional (default implementations) or required?

**Should we design for hardware provider constraints now, or add listing operations later if needed?**
