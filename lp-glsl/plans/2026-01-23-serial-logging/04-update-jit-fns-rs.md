# Phase 4: Update jit_fns.rs

## Description

Update `jit_fns.rs` to replace defmt macros with esp_println for host functions called by JIT-compiled code.

## Changes

### Imports
- **REMOVE:** `use defmt::debug;` (if present)
- **REMOVE:** `use defmt::info;` (if present)
- **ADD:** `use esp_println::println;`

### Function Updates
- **CHANGE:** `defmt::debug!(...)` → `println!(...)` in `lp_jit_host_debug`
- **CHANGE:** `defmt::info!(...)` → `println!(...)` in `lp_jit_host_println`

## Success Criteria

- Code compiles without errors
- All defmt references removed
- Host functions use esp_println

## Code Organization

- Place helper utility functions at the bottom of files
- Keep related functionality grouped together

## Formatting

- Run `cargo +nightly fmt` on all changes before committing

## Language and Tone

- Keep language professional and restrained
- Avoid overly optimistic language
- Use measured, factual descriptions
