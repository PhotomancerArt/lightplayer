# Implementation Questions: Engine Core and Model

## Context

We're implementing the engine core and model refactor with the goal of creating a comprehensive test that:
- Uses memory fs
- Creates an engine with a basic project (simple time-based shader)
- Sets up a client
- Uses the client to watch detail for a texture
- Ticks the engine
- Ensures changes were synced
- Updates a file (shader main.glsl)
- Ensures that the engine picked up the change and the client is updated

## Questions

1. **Incremental vs Big Bang**: Should we implement everything incrementally (model → engine → client → sync → test), or can we stub out parts (e.g., stub client API initially) to get the test working end-to-end faster?

2. **Filesystem watching**: For the hot-reload test, do we need full filesystem watching in the first phase, or can we manually trigger reloads for now?

3. **Client API implementation**: Should the client API be a real trait with a mock implementation for testing, or can we start with a simpler direct-call approach for the test?

4. **Shader compilation**: Do we need full GLSL compilation working, or can we stub shader execution initially to focus on the sync/hot-reload flow?

5. **Test structure**: Should the comprehensive test be in `lp-engine` tests, or a separate integration test? Should it be a single large test or broken into smaller test functions?

6. **Dependencies**: Should we implement `lp-model` first completely, then `lp-engine`, then `lp-engine-client`, or can we work across them incrementally?
