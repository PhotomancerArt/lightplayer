# Probes

Probes are request-scoped diagnostic reads.

They let a client ask the runtime to inspect or materialize something without
adding authored graph state, creating a persistent resource, or registering a
subscription. The mental model is an oscilloscope probe: the project keeps
running normally, and the client asks for an observation at one point in time.

## Rules

- Probes are stateless on the server.
- Probes are explicit because they may be expensive.
- Probe results are not part of the normal project mirror.
- Probes do not mutate node definitions, bindings, resources, or runtime state.
- Probes do not imply a persisted resource unless a future operation explicitly
  promotes one.

## First Probe Families

`RenderProduct` probes a `VisualProduct` by asking it to render into an
inspection format. The engine may use a scratch buffer internally, but the
response is still a probe result, not texture resource sync.

`ExplainSlot` explains how a node slot resolves. For consumed slots this can
eventually re-run resolution with tracing enabled so the UI can show where the
effective value came from.

## Future Probe Families

Shader probes are a major future use of LightPlayer's CPU shader engine. A
client should eventually be able to render or sample a shader, select a pixel or
sample, and ask the runtime for detailed debug information about how that value
was produced.

Other likely probe families include control-buffer probes, filesystem probes,
and IO probes.
