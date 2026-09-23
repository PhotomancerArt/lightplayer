# ser-write 0.3.1 — LightPlayer spike fork

Vendored from crates.io `ser-write` 0.3.1 (MIT OR Apache-2.0, licenses alongside).
Spike change (branch `spike/ion-wire`, 2026-09-23): `SerWrite::token` and the `Token` enum,
a provided method whose default declines, so all existing sinks behave identically. It lets a
sink take structural tokens from `ser-write-json`'s serializer instead of its JSON text,
through the same single serializer instantiation.
