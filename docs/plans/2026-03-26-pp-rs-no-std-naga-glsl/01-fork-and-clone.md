# Phase 1: Fork and clone `pp-rs`

## Scope of phase

Create **`https://github.com/light-player/pp-rs`** from upstream **`pp-rs`** (version **0.2.1**, matching **naga 29**’s dependency). Clone it as **`../pp-rs`** next to **`lp2025`**.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

1. **Upstream source:** Use crates.io **`pp-rs` 0.2.1** tarball or the copy under **`~/.cargo/registry/src/.../pp-rs-0.2.1/`** as the baseline (MIT — verify **`LICENSE`** in crate root).
2. **GitHub:** Create empty **`light-player/pp-rs`**, push initial commit mirroring **0.2.1** source (exclude **`target/`**).
3. **Local:** From parent of **`lp2025`**:
   ```bash
   cd ..
   git clone https://github.com/light-player/pp-rs.git
   ```
4. **README:** Add a short note: fork for **`#![no_std]`** / **naga `glsl-in`** on bare-metal; link to this plan or **`lp2025`** roadmap.

## Validate

- **`cd ../pp-rs && cargo check`** (host) passes on clean tree **before** `no_std` edits (baseline).

## Tests to write

- None this phase (infra only).
