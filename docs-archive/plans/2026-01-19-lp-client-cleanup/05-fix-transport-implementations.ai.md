# Phase 5: Fix Transport Implementations

## Description

Update existing transport implementations to work with the new `ClientTransport` trait and `LpClient`.

## Tasks

1. Update `lp-app/apps/lp-cli/src/client/transport_ws.rs`:
   - Implement new `ClientTransport` trait
   - Ensure async methods work correctly
   - Update to work with `Arc<dyn ClientTransport>`

2. Update `lp-app/apps/lp-cli/src/client/local.rs`:
   - Check if `AsyncLocalClientTransport` needs to implement new trait
   - Update or create wrapper if needed

3. Update `lp-app/apps/lp-cli/src/client/client_connect.rs`:
   - Ensure it returns a transport that implements `ClientTransport`
   - Update return type if needed

4. Check `lp-app/apps/lp-cli/src/client/async_transport.rs`:
   - If file exists, update to work with new trait
   - If file doesn't exist, may need to create wrapper or remove references

5. Fix any compilation errors related to transport:
   - Ensure all transports implement `ClientTransport`
   - Ensure `Send` bound is satisfied
   - Fix any async/await issues

## Success Criteria

- All transport implementations work with `ClientTransport` trait
- `client_connect` returns compatible transport
- Code compiles without transport-related errors
- `LpClient` can be created with transports
