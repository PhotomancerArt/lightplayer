# Phase 2: `#![no_std]` + `alloc` implementation

## Scope of phase

Convert **`pp-rs`** to compile on **`no_std`** targets that provide **`alloc`** (global allocator on ESP32 / firmware).

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. **`src/lib.rs`**
   - Add **`#![no_std]`**.
   - Add **`extern crate alloc;`**.
   - Keep **`unicode_xid`** as today.

2. **`Cargo.toml`**
   - Add **`hashbrown`** with **`default-features = false`**, features aligned with **naga** (e.g. **`default-hasher`**, **`inline-more`** if needed for API parity with **`std::collections::HashMap`**).
   - Bump **version** patch (e.g. **`0.2.2`**) so patched resolution is unambiguous.
   - Set **`edition`** compatible with **lp2025** workspace (**2021** or **2024** — match upstream unless you intentionally align to **2024**).

3. **Replace `std` imports (mechanical map)**

   | Was | Becomes |
   |-----|---------|
   | **`std::collections::HashMap`** | **`hashbrown::HashMap`** |
   | **`std::collections::HashSet`** | **`hashbrown::HashSet`** |
   | **`std::rc::Rc`** | **`alloc::rc::Rc`** |
   | **`std::vec::Vec` / `vec::`** | **`alloc::vec::Vec`** / **`alloc::vec`** |
   | **`std::cmp::Ordering`** | **`core::cmp::Ordering`** |
   | **`std::convert::{TryFrom, TryInto}`** | **`core::convert::{TryFrom, TryInto}`** |
   | **`std::str::Chars`** | **`core::str::Chars`** |
   | **`std::usize::MAX`** | **`core::usize::MAX`** |

4. **Files to touch:** **`src/lexer.rs`**, **`src/pp.rs`**, **`src/pp/if_parser.rs`**, **`src/token.rs`** (if any **`String`** — use **`alloc::string::String`** where needed; **`token.rs`** may already be **`Copy`**-heavy).

5. **Tests:** **`lexer_tests.rs`**, **`pp_tests.rs`** are **`#[cfg(test)]`** — they run on **host** with **`std`**. Either:
   - gate tests with **`#[cfg(test)]`** and add **`extern crate std`** only under test, or
   - use **`alloc`** + **`std`** in tests via **`#[cfg(test)] mod ...`**. Simplest: **`#![cfg_attr(test, allow(...))]`** and **`extern crate std`** only for test module if required.

6. **`String` / `format!`:** If any **`format!`** appears, use **`alloc::format!`** or **`alloc::string::String`** + manual concat.

## Tests to write

- No new tests required if existing **`pp-rs`** tests still pass on host after conversion.
- Optional: **`cargo test`** in **`pp-rs`** on CI.

## Validate

```bash
cd ../pp-rs
cargo check
cargo test
```
