# Phase 3: Implement Async Project Methods

## Description

Implement async version of project loading operation (`project_load`) on `AsyncLpClient`.

## Tasks

1. Implement `project_load(&self, path: String) -> Result<ProjectHandle>`:
   - Create request via `LpClient::project_load()`
   - Send message via transport
   - Call `wait_for_response()` to get response
   - Extract response using `LpClient::extract_load_project_response()`
   - Return project handle

## Success Criteria

- `project_load()` method compiles and returns project handle
- Method handles timeouts correctly
- Method handles transport and client errors correctly
- Code compiles without errors

## Implementation Notes

- Method should be `async fn`
- Use `wait_for_response()` helper for consistency
- Wrap `ClientError` in `anyhow::Error` with context
- Ensure proper error messages for debugging
