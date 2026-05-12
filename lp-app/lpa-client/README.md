# lpa-client

Application client layer for LightPlayer.

This crate owns client-side transports and request helpers for talking to a
LightPlayer server or firmware target. It is app-facing: websocket, serial,
emulator, and local-server client plumbing belong here.

It uses `lpc-wire` for messages and may use `lpc-view` in tests or callers to
maintain a local view of one engine, but it should not own core engine state or
source/wire type definitions.
