# lpc-model

Shared LightPlayer core model concepts used by source files, wire messages,
the engine, and the engine client.

This crate owns stable shared vocabulary such as node identity, tree paths,
property paths, frame ids, semantic kinds, and portable value/type shapes.

It should not contain authored source-file formats, engine-client wire messages,
or engine runtime behavior. Those live in `lpc-source`, `lpc-wire`, and
`lpc-engine` respectively.

`no_std`, designed for running on embedded devices. It should not depend on
`lps-shared`; shader/runtime value conversion belongs at the `lpc-engine`
boundary.