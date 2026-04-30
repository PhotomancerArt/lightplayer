# lpa-server

The LightPlayer application server layer.

This crate hosts one or more core engines behind the `lpc-wire` API and handles
project management, request routing, and server-side integration points.

Used by apps and firmware to provide LightPlayer server functionality. All
communications are abstracted: serial, websocket, HTTP, or other concrete
transports are supplied by the embedding app.

`no_std`, designed for embedding.