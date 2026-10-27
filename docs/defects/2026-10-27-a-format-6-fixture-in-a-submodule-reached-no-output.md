---
status: fixed
found: 2026-10-27      # how: ci (the format-12 parity test, mapping rebuild M3 P4)
fixed: 93e40ad18
area: lpc-engine — the port tables a format-6 fixture patches against (EngineResolveHost::control_patch_outputs)
class: untested-path   # module exports, which the format-11 path never had to cross, were never in a format-6 test
related:
  - lp-core/lpc-engine/src/engine/engine.rs (control_patch_outputs, bus_read_reaches, output_port_table)
  - lp-core/lpc-engine/src/engine/output_port_table_tests.rs (a_fixture_in_a_submodule_reaches_an_output_through_its_export)
  - lp-core/lpc-engine/tests/upgrade_landing.rs (the parity test that found it)
  - lp2025/2026-10-27-0114-mapping-rebuild (M2 P1, M3 P4)
---
# A format-6 fixture in a submodule reached no output

**Symptom** — After the repo's projects were upgraded to project format 12,
the parity test found peach-1d, peach-2d and small-dome dark: every lamp of
their fixtures landed nowhere, and each fixture said
`the fixture's start names output "1", but no named output reads bus:control.out`.
Every other project landed lamp for lamp as at format 11.

**Root cause** — Those projects keep each fixture in a submodule whose
`module.json` exports `control.out` to the root, where the output reads it
(R7). The question a format-6 fixture asks — which named outputs gather my
product, with which ports (M2 P1) — looked for outputs reading the
producer's own channel name, and accepted a read only when its provider was
the producer itself. In the root scope the provider is the submodule's
export binding, whose source is the inner channel, so every output was
hidden. The format-11 path never asked the question (an unpatched fixture
auto-flowed onto the default output, and the output gathered fragments
through the export), so no M2 test crossed a module boundary.

**Fix** — The question follows exports: the names the product is published
under are its channel plus every name an export republishes one of them as,
and a read reaches the producer when its provider is the producer or an
export whose inner read does (depth-bounded).

**Regression coverage** — `a_fixture_in_a_submodule_reaches_an_output_through_its_export`
(fails without the fix: `left: [], right: ["main"]`), and `upgrade_landing`
over peach-1d, peach-2d, small-dome and their v11 corpus copies.

**Lesson** — A new path that replaces an old one inherits every topology
the old one handled implicitly. The format-11 path never named outputs, so
it never had to see across a module boundary; the format-6 path names them,
and the projects that cross a boundary are exactly the shipped submodule
examples. A parity test over every real project, against goldens recorded
from the old path, is what found it — the per-feature tests written for the
new path could not.
