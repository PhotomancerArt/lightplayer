//! Bus channels: how a fixture reaches an output in the product.
//!
//! A fixture writes its lamps to a bus channel (`bindings.output.target`,
//! `bus:control.out` when it says nothing), and an output reads one
//! (`bindings.input.source`, the same default). Every fixture on a channel
//! auto-flows onto the channel's **default output**: the first output, in
//! attach order, that reads it (the engine's `is_default_fragments_consumer`).
//! That is the node model, followed as it is (D52).
//!
//! The lab's auto rule names an output; on disk it is "the fixture writes
//! the channel that output reads" (fit-check §6).
//!
//! Scope: a channel belongs to a module. A module that exports a channel
//! (`"exports": {"control.out": "bus:control.out"}`) publishes it to its
//! parent under the export's name, so an output there reads it too.

use serde_json::Value;

/// The channel a binding names: `bus:control.out` → `control.out`.
pub fn channel_of(binding: Option<&Value>, key: &str) -> String {
    binding
        .and_then(|b| b.get(key))
        .and_then(Value::as_str)
        .and_then(|s| s.strip_prefix("bus:"))
        .unwrap_or("control.out")
        .to_string()
}

/// The channel a fixture writes.
pub fn fixture_writes(fixture: &Value) -> String {
    channel_of(
        fixture.get("bindings").and_then(|b| b.get("output")),
        "target",
    )
}

/// The channel an output reads.
pub fn output_reads(output: &Value) -> String {
    channel_of(
        output.get("bindings").and_then(|b| b.get("input")),
        "source",
    )
}

/// What a module publishes to its parent: (name outside, channel inside).
pub fn exports(module: &Value) -> Vec<(String, String)> {
    module
        .get("exports")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter_map(|(outside, inside)| {
            let inside = inside.as_str()?.strip_prefix("bus:")?;
            Some((outside.clone(), inside.to_string()))
        })
        .collect()
}
