# Plan: lp-client Cleanup and Refactor

## Overview

Delete the `lp-client` crate entirely and rebuild `LpClient` as a standalone client in `lp-cli`. Create a new async `ClientTransport` trait in `lp-cli` built for our needs.

## Phases

1. Remove lp-client dependency and delete crate
2. Create ClientTransport trait
3. Rebuild LpClient from scratch
4. Update all call sites (rename AsyncLpClient -> LpClient)
5. Fix compilation errors and get building
6. Cleanup and finalization

## Success Criteria

- `lp-client` crate completely deleted
- `lp-cli` compiles without errors
- `LpClient` works with existing code (handler.rs, push_project.rs, etc.)
- All references to `lp-client` removed
- Code is clean and ready for use
