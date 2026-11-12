# Plan: Update LpFs to use LpPath consistently (with Path/PathBuf split)

## Context

We're splitting `LpPathBuf` (owned) and creating `LpPath` (slice type) to match Rust's `Path`/`PathBuf` pattern. This allows proper `AsRef<LpPath>` implementations and better type safety.

Current state: `LpPathBuf(String)` exists and is used throughout the codebase.

Goal: Create `LpPath(str)` slice type and update `LpFs` to use `P: AsRef<LpPath>`.

## Questions

### Q1: How should we implement the `LpPath` slice type?

**Context**: `LpPath` needs to be an unsized type (like `str` or `Path`). In Rust, this is done with `#[repr(transparent)]` and a wrapper around the unsized type.

**Options**:
- Option A: `#[repr(transparent)] pub struct LpPath(str);` - matches Rust's `Path` exactly
- Option B: Use a newtype wrapper with different internal representation

**Answer**: Option A - Use `#[repr(transparent)] pub struct LpPath(str);` to match Rust's pattern exactly. This allows `LpPath` to be a thin wrapper around `str` with the same memory layout.

### Q2: Where should methods live - `LpPath` or `LpPathBuf`?

**Context**: Methods like `is_absolute()`, `file_name()`, `parent()`, etc. can work on both borrowed (`&LpPath`) and owned (`LpPathBuf`) paths. In Rust, these methods are typically on `Path` (the slice type), and `PathBuf` gets them via `Deref`.

**Options**:
- Option A: Put all read-only methods on `LpPath`, mutation methods on `LpPathBuf`, `LpPathBuf` derefs to `LpPath`
- Option B: Duplicate methods on both types
- Option C: Put methods on `LpPathBuf` only

**Answer**: Option A - Put read-only methods (`is_absolute`, `file_name`, `parent`, `components`, etc.) on `LpPath`. Put mutation methods (`join`, `push`, etc.) on `LpPathBuf`. Implement `Deref<Target = LpPath>` for `LpPathBuf` so it can use `LpPath` methods.

### Q3: Should `LpPath` methods return `&LpPath` or `LpPathBuf`?

**Context**: Methods like `parent()` currently return `Option<LpPathBuf>`. With the split, we could return `Option<&LpPath>` (borrowed) or `Option<LpPathBuf>` (owned).

**Options**:
- Option A: Return `&LpPath` when possible (e.g., `parent()` returns `Option<&LpPath>`), `LpPathBuf` when building new paths
- Option B: Always return `LpPathBuf` for consistency
- Option C: Return `&LpPath` for methods that return views into the original path

**Answer**: Option A - Return `&LpPath` when we can create a view into the original path (like `parent()` can return a slice of the original string). Return `LpPathBuf` when we need to build/combine paths (like `join()`). This matches Rust's pattern.

### Q4: How should normalization work with `LpPath`?

**Context**: Currently `LpPathBuf` normalizes on construction. `LpPath` is a slice type, so it doesn't "construct" - it's created from existing `str` data.

**Options**:
- Option A: `LpPath::new()` normalizes the input `&str` and creates `LpPath` (requires allocation for normalization)
- Option B: `LpPath` doesn't normalize - only `LpPathBuf` normalizes (via `From` implementations)
- Option C: `LpPath` can be created from normalized or unnormalized strings, normalization happens when converting to `LpPathBuf`

**Answer**: Option B - `LpPath` is just a view - it doesn't normalize. Normalization happens when creating `LpPathBuf` via `From<&str>` or `From<String>`. `LpPath::new()` can take a `&str` directly without normalization. This matches Rust's pattern where `Path::new()` doesn't normalize.

### Q5: How to implement `AsRef<LpPath>` for `&str` and `String`?

**Context**: We need `&str` and `String` to implement `AsRef<LpPath>` so they can be passed to `LpFs` methods. But `AsRef` returns a reference, and we can't return a reference to a temporary.

**Options**:
- Option A: `impl AsRef<LpPath> for &str` that creates `LpPath` via `LpPath::new()` - but this requires storing the normalized string somewhere (not possible)
- Option B: `impl AsRef<LpPath> for &str` that doesn't normalize - just casts `&str` to `&LpPath` (unsafe but matches Rust's `Path::new()`)
- Option C: Use `P: AsRef<str>` in trait, convert to `LpPathBuf` internally, then get `&LpPath` from that

**Answer**: Option B - `impl AsRef<LpPath> for &str` uses unsafe to cast `&str` to `&LpPath` (since they have the same memory layout with `#[repr(transparent)]`). This matches how Rust's `Path::new()` works - it doesn't normalize, just creates a view. Normalization happens when converting to `LpPathBuf`. Confirmed: Rust's `Path::new()` doesn't normalize (just unsafe cast), and `PathBuf::from()` also doesn't normalize. Our `LpPathBuf` normalizing on construction is custom behavior (fine).

### Q6: Should `LpPathBuf` implement `Deref<Target = LpPath>`?

**Context**: This allows `LpPathBuf` to use all `LpPath` methods automatically, matching Rust's `PathBuf` → `Path` relationship.

**Options**:
- Option A: Yes, implement `Deref<Target = LpPath>` so `LpPathBuf` can use `LpPath` methods
- Option B: No, keep them separate

**Answer**: Option A - Implement `Deref<Target = LpPath>` for `LpPathBuf`. This allows `let buf: LpPathBuf = ...; buf.is_absolute()` to work automatically, matching Rust's pattern.

### Q7: What about serialization (Serialize/Deserialize)?

**Context**: `LpPathBuf` currently has `#[derive(Serialize, Deserialize)]`. `LpPath` is unsized, so it can't be serialized directly.

**Options**:
- Option A: Keep `Serialize/Deserialize` on `LpPathBuf` only, serialize as string
- Option B: Custom serialization for `LpPath` that serializes as string
- Option C: Only `LpPathBuf` can be serialized (matches Rust - `PathBuf` can be serialized, `Path` cannot)

**Answer**: Option C - Only `LpPathBuf` implements `Serialize/Deserialize`. `LpPath` is a slice type and can't be serialized directly (just like Rust's `Path`). When serializing, convert to `LpPathBuf` first. This matches Rust's pattern where only `PathBuf` can be serialized.

### Q8: Should `list_dir()` return `Vec<&LpPath>` or `Vec<LpPathBuf>`?

**Context**: Currently `list_dir()` returns `Vec<String>`. With the split, we need to decide what to return.

**Options**:
- Option A: Return `Vec<LpPathBuf>` (owned) - callers own the paths
- Option B: Return `Vec<&LpPath>` (borrowed) - but this requires lifetime management
- Option C: Return `Vec<LpPathBuf>` for simplicity and ownership clarity

**Answer**: Option A - Return `Vec<LpPathBuf>`. The filesystem builds new paths, so callers should own them. Borrowed references would require complex lifetime management and don't make sense here. This matches Rust's pattern where functions that create/build paths return `PathBuf`.

### Q9: Should we update all existing code to use `LpPath` where appropriate?

**Context**: With the split, we should use `&LpPath` in function parameters and `LpPathBuf` for storage/returns.

**Options**:
- Option A: Update gradually - change `LpFs` first, then update call sites as needed
- Option B: Update everything systematically - function params use `&LpPath` or `P: AsRef<LpPath>`, storage uses `LpPathBuf`
- Option C: Keep using `LpPathBuf` everywhere for now, add `LpPath` for `LpFs` API only

**Answer**: Option B - Update everything systematically. Function parameters should use `&LpPath` or `P: AsRef<LpPath>`. Struct fields and return values should use `LpPathBuf`. This provides the full benefit of the split and makes the codebase consistent. We'll do it comprehensively to avoid tech debt - the sooner we do it, the less tech debt we have.
