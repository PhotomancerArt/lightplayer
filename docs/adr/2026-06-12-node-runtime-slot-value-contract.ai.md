# ADR 2026-06-12: Node Runtime Slot Value Contract

## Status

Accepted

## Context

Project registry change summaries can report that a node definition body
changed, but that fact is too coarse to decide whether an engine runtime node
must be rebuilt.

A `NodeDef` is authored state. A runtime node consumes effective values through
the slot resolver. Those effective values may come from authored defaults,
overlay edits, bindings, upstream produced values, or other resolver behavior.
Therefore an authored body change may not change the effective value observed
by a runtime node at all.

For example, a slot value in a node definition can change while the runtime node
continues to read a bound value. Rebuilding the runtime node just because the
definition body changed would waste CPU, churn runtime resources, and create
unnecessary work on embedded targets.

Some values feel structural or construction-oriented today, such as shader
source, fixture source/mapping assets, parameter definitions, or buffer layout
inputs. The current model does not yet provide a general, explicit distinction
between ordinary values and these structural inputs.

## Decision

Runtime nodes are expected to fully support changes to every value they
consume.

The slot resolver is the source of truth for runtime node inputs. Runtime nodes
must read consumed values through resolver-backed APIs and use revisions or
equivalent change detection to refresh any cached internal state that depends on
those values.

A same-kind `NodeDef` body change is not a runtime lifecycle event by default.
The engine must not destroy, recreate, or reattach runtime nodes solely because
the authored body changed.

The engine remains responsible for lifecycle and topology changes:

- node uses are added or removed from the effective project tree;
- a node use changes to a different definition or placement;
- a definition changes kind;
- a definition enters or leaves a load error state such that a runtime node can
  no longer be constructed or can now be constructed.

Runtime node implementations are responsible for their own consumed-value
semantics:

- ordinary scalar, enum, record, and collection values;
- values overridden through bindings;
- source text and source-like asset contents;
- fixture mappings or other loaded asset-derived inputs;
- node-specific structural details until the model grows explicit metadata for
  them.

If a node needs to recompile, rebuild a mapping, invalidate a cache, resize a
buffer, or enter a failed state because a consumed value or asset changed, that
logic belongs in the node implementation or in node-specific helper code. It
should not be implemented as a generic engine reaction to `NodeDef` body
changes.

Future work may add explicit model metadata, structural notifications, or
node-specific reload hooks. Those mechanisms should refine this contract rather
than replace it: runtime nodes still observe effective values, and the engine
still handles topology and lifecycle.

## Consequences

Incremental runtime apply can be conservative without being wasteful. It can
apply node-use additions/removals and kind/error transitions while leaving
same-kind body changes to runtime node value observation.

Tests for project editing should distinguish lifecycle from value observation:

- a simple authored value edit should not destroy or recreate the runtime node;
- the runtime node should still observe the changed effective value when that
  value is not masked by a binding;
- a binding that masks the authored value means the runtime node should keep
  seeing the bound value;
- shader source, fixture mapping, and similar asset-backed inputs should be
  tested through the consuming node's refresh behavior, not by asserting a
  generic engine reattach.

When a node fails to observe a consumed value change correctly, that is a bug in
the node or resolver path. It should be fixed with a focused test for that node
instead of broadening engine-level rebuild policy.

This contract is especially important for ESP32. Avoiding unnecessary runtime
node churn preserves CPU, memory, buffers, output sinks, and compiled shader
state on the embedded target.
