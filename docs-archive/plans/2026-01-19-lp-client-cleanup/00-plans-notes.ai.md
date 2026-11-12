# Plan Notes: lp-client Cleanup and Refactor

## Context

We attempted to create a `no_std` `lp-client` crate that could be reused, but it's proven too difficult. We're abandoning that approach and consolidating everything into `lp-cli`.

## Current State

### lp-client crate
- Located at `lp-app/crates/lp-client/`
- Has `#![no_std]` with `extern crate alloc`
- Contains:
  - `LpClient` struct - async client with background receiver task
  - `ClientTransport` trait - async transport interface
  - `LocalTransport` - in-memory transport for testing
  - `channel` module - no_std channels (unbounded, oneshot)
  - `error` module - `ClientError` type
  - Tests using tokio

### lp-cli usage
- Code references `AsyncLpClient` (which was deleted)
- Calls methods like `client.fs_read()`, `client.fs_write()`, etc.
- Uses `AsyncClientTransport` for async request/response correlation
- Needs to be rebuilt as `LpClient`

### Issues
- `lp-client` **doesn't compile** - has compilation errors (Wake trait, oneshot function visibility, conflicting Future impls)
- `LpClient` in `lp-client` doesn't have the methods `AsyncLpClient` expects (like `fs_read()`, `fs_write()`, etc.)
- `lp-cli` imports `lp_shared::transport::ClientTransport` but that trait doesn't exist in `lp-shared` (only `ServerTransport` exists)
- `AsyncLpClient` wraps `LpClient` in a `Mutex` and calls methods that don't exist
- The no_std approach is too complex and not working well
- Code is broken - there's a mismatch between what's expected and what exists

## Goals

1. Move `LpClient` functionality into `lp-cli`, rebuilding as standalone `LpClient`
2. Move anything else from `lp-client` that we still need into `lp-cli`
3. Delete the `lp-client` crate totally
4. Get `lp-cli` building again

## Questions

### Question 1: What functionality from lp-client do we actually need?

**Context**: We need to understand what parts of `lp-client` are actually being used or needed.

**Current usage**:
- `AsyncLpClient` wraps `LpClient` in a `Mutex` and calls methods like `fs_read()`, `fs_write()`, etc.
- Integration tests use `LpClient::new()` and call similar methods
- Tests also use `LocalTransport` from `lp-client`

**Answer**: 
- We can delete lp-client totally
- Rebuild `AsyncLpClient` to be standalone (not depending on `LpClient`)
- Extract functions weren't that useful - we'll create our own as needed
- No need to preserve anything from lp-client

### Question 2: Should we merge LpClient into AsyncLpClient or keep them separate?

**Answer**: 
- User deleted the old `async_client.rs` - we're starting over making the client from scratch
- Create a `ClientTransport` trait in `lp-cli` (async, built for our needs)
- Rebuild as `LpClient` (not `AsyncLpClient`) - standalone, no dependency on `lp-client` crate

### Question 3: What about ClientTransport trait and LocalTransport?

**Context**: 
- `lp-cli` imports `lp_shared::transport::ClientTransport` but it doesn't exist
- `lp-client` has an async `ClientTransport` trait
- `lp-cli` already has `AsyncLocalClientTransport` in `local.rs` that implements a sync `ClientTransport` trait
- `lp-client` has `LocalTransport` for testing

**Findings**:
- `lp-cli/src/client/local.rs` already has local transport functionality
- `lp-shared` only has `ServerTransport`, not `ClientTransport`
- `AsyncClientTransport` wraps a `ClientTransport` trait

**Answer**: 
- Create a `ClientTransport` trait in `lp-cli` (async, built for our needs there)
- User deleted the old `async_client.rs` - we should start over making the client from scratch

### Question 4: What about the channel module?

**Context**: `lp-client` has a `channel` module with no_std channels.

**Suggested answer**: 
- Not needed if we're using std - can use `tokio::sync` channels
- Delete it

### Question 5: What about error types?

**Context**: `lp-client` has `ClientError` type.

**Suggested answer**: 
- Check if we need a separate error type or can use `anyhow::Error`
- Probably can use `anyhow::Error` or create a simple error type in `lp-cli`

### Question 6: What about tests in lp-client?

**Answer**: 
- Delete them - we'll build new ones for the rebuilt client
