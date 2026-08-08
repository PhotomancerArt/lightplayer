//! Format 4 → 5: `bus:time` carries a time product instead of `f32` seconds.
//!
//! The break (`f9d6981dc`, "bus:time carries the TimeProduct") retyped three
//! things at once:
//!
//! - `PlaylistState::time` and `FluidDef::time` went from `f32` to a time
//!   product, so an authored numeric `"time"` line is now a type error.
//! - The clock's `bus:time` publication moved from its `seconds` field to a
//!   new `product` field.
//! - Shader uniforms that used to read `f32` seconds off `bus:time` now
//!   declare a `seconds` (or `phasor`) slot kind, evaluated against the
//!   scope's time product instead of resolved through a binding.
//!
//! This step is **behavior-preserving**: an f32 seconds uniform becomes a
//! `seconds` slot, which feeds the same number into the same GLSL. The hand
//! migration of the gallery went further and converted several of those into
//! phasors — that needed period constants mined out of the shader source and
//! new slot names, information not present in the v4 bytes. Phasor-ization
//! is authoring polish; it is not something an upgrader may invent.
//!
//! ## Rule R10, in the negative
//!
//! Nothing here keys off a slot being *named* `time`.
//! `projects/test/fyeah-sign/blast.json` has a `time` slot bound to
//! `node:..#entry_time`, and it must pass through byte-identical. The signal
//! is always a `bus:time` reference, never a name.

use crate::json::JsonNode;
use crate::json_file_edit::edit_json_files;
use crate::project_files::{ProjectFiles, is_manifest_path};
use crate::upgrade_error::UpgradeError;
use crate::upgrade_report::UpgradeReport;

const FROM: u32 = 4;
const TO: u32 = 5;

/// The channel whose payload type changed.
const TIME_BUS: &str = "bus:time";

/// Node artifact kinds as of format 4. Hard-coded rather than read from
/// `lpc-model`: a step describes the world at *its* version, and the live
/// variant list is free to grow without changing what v4 could contain.
const V4_NODE_KINDS: &[&str] = &[
    "Module",
    "Button",
    "Clock",
    "Texture",
    "Shader",
    "ComputeShader",
    "Fluid",
    "Playlist",
    "ControlRadio",
    "Output",
    "Fixture",
];

pub(crate) fn apply(
    files: &mut ProjectFiles,
    report: &mut UpgradeReport,
) -> Result<(), UpgradeError> {
    let mut seconds_slots = 0usize;
    edit_json_files(files, report, |path, document, report| {
        if is_manifest_path(path) {
            bump_manifest_format(path, document, report);
        }
        migrate_artifacts(path, document, report, &mut seconds_slots)?;
        refuse_unhandled_time_refs(path, document, None, "")
    })?;

    if seconds_slots > 0 {
        report.warn(format!(
            "{seconds_slots} shader uniform(s) now read the scope's time product as `seconds`, \
             which preserves what they did before. Unbounded seconds lose precision in \
             fixed-point over long runs — consider converting the long-running ones to phasors \
             the next time you edit those shaders."
        ));
    }
    Ok(())
}

/// R1: the manifest's own version stamp.
fn bump_manifest_format(path: &str, document: &mut JsonNode, report: &mut UpgradeReport) {
    if document.get("format").and_then(JsonNode::as_u32) == Some(FROM) {
        document.set("format", JsonNode::u32(TO));
        report.note(format!("{path}: format {FROM} → {TO}"));
    }
}

/// Walk every object in the document, applying the artifact rules to each one
/// that carries a v4 node `kind`. Walking (rather than only looking at the
/// file root) is what covers a node inlined inside another artifact.
fn migrate_artifacts(
    path: &str,
    node: &mut JsonNode,
    report: &mut UpgradeReport,
    seconds_slots: &mut usize,
) -> Result<(), UpgradeError> {
    match node {
        JsonNode::Object(_) => {
            match node_kind(node).as_deref() {
                Some("Playlist") => remove_authored_time_value(path, node, "Playlist", report),
                Some("Fluid") => remove_authored_time_value(path, node, "Fluid", report),
                Some("Shader" | "ComputeShader") => {
                    migrate_shader_slots(path, node, report, seconds_slots)?;
                }
                Some("Clock") => rename_clock_time_binding(path, node, report),
                _ => {}
            }
            let members = node.object_mut().expect("object");
            for (_, child) in members.iter_mut() {
                migrate_artifacts(path, child, report, seconds_slots)?;
            }
        }
        JsonNode::Array(items) => {
            for item in items.iter_mut() {
                migrate_artifacts(path, item, report, seconds_slots)?;
            }
        }
        JsonNode::Scalar(_) => {}
    }
    Ok(())
}

/// R2: `PlaylistState::time` / `FluidDef::time` are time products now, so an
/// authored numeric value line no longer parses.
///
/// R3 lives here too, in the negative: the `bindings.time → bus:time` entry
/// on these same nodes is left verbatim. Binding registration is name-driven
/// and the slot still consumes `bus:time`; only its payload type changed.
fn remove_authored_time_value(
    path: &str,
    artifact: &mut JsonNode,
    kind: &str,
    report: &mut UpgradeReport,
) {
    if artifact.get("time").is_some_and(JsonNode::is_number) {
        artifact.remove("time");
        report.note(format!(
            "{path}: removed the authored `time` value ({kind} time is the scope's time product now)"
        ));
    }
}

/// R5: an `f32` uniform fed by `bus:time` becomes a `seconds` slot.
///
/// Two authored spellings reach the same place, and the hand migration
/// treated them identically:
///
/// - a declarative `"default_bind": "bus:time"` on the slot
///   (`schemas/history/v4/fixtures/fyeah-sign/idle.json`), and
/// - an explicit `bindings` entry sourcing `bus:time`
///   (`examples/meteor/sim.json`, `examples/events/event_a.json` at
///   `f9d6981dc^`).
///
/// Both lose the `bus:time` reference: a `seconds` slot reads the scope's
/// time product directly and never resolves through a binding.
fn migrate_shader_slots(
    path: &str,
    artifact: &mut JsonNode,
    report: &mut UpgradeReport,
    seconds_slots: &mut usize,
) -> Result<(), UpgradeError> {
    let authored = artifact
        .get("bindings")
        .and_then(JsonNode::object)
        .cloned()
        .unwrap_or_default();
    let time_bound: Vec<String> = authored
        .iter()
        .filter(|(_, binding)| binding.has_string("source", TIME_BUS))
        .map(|(name, _)| name.clone())
        .collect();
    let bound: Vec<String> = authored.iter().map(|(name, _)| name.clone()).collect();

    let Some(consumed) = artifact.get_mut("consumed") else {
        return Ok(());
    };
    let Some(slots) = consumed.object_mut() else {
        return Ok(());
    };

    let mut retyped = Vec::new();
    for (name, slot) in slots.iter_mut() {
        let default_bound = slot.has_string("default_bind", TIME_BUS);
        let explicitly_bound = time_bound.contains(name);
        if !default_bound && !explicitly_bound {
            continue;
        }
        // A `default_bind` only materializes when no authored binding names
        // the slot (ADR 2026-07-09). One that is overridden by a binding to
        // some *other* endpoint is dead text: the uniform does not read the
        // clock, so retyping it would change what the shader sees. Which of
        // the two the author meant is not recoverable from the bytes.
        if default_bound && !explicitly_bound && bound.contains(name) {
            return Err(UpgradeError::Refused {
                file: String::from(path),
                reason: format!(
                    "consumed slot `{name}` declares `default_bind: {TIME_BUS}` but an authored \
                     binding overrides it, so the default never applied — remove one of the two \
                     by hand and re-open the project"
                ),
            });
        }
        // A missing `kind` is the model's default, `value`.
        let kind = slot
            .get("kind")
            .and_then(JsonNode::as_str)
            .unwrap_or_else(|| String::from("value"));
        if kind != "value" {
            return Err(UpgradeError::Refused {
                file: String::from(path),
                reason: format!(
                    "consumed slot `{name}` reads {TIME_BUS} but is a `{kind}` slot, not a \
                     `value` slot — only an f32 seconds uniform has a behavior-preserving \
                     answer here"
                ),
            });
        }
        slot.set("kind", JsonNode::string("seconds"));
        if default_bound {
            slot.remove("default_bind");
        }
        retyped.push(name.clone());
        *seconds_slots += 1;
        report.note(format!(
            "{path}: consumed slot `{name}` is a `seconds` slot now (was an f32 `value` fed by {TIME_BUS})"
        ));
    }

    let dropped: Vec<String> = retyped
        .into_iter()
        .filter(|name| time_bound.contains(name))
        .collect();
    if dropped.is_empty() {
        return Ok(());
    }
    let bindings = artifact.get_mut("bindings").expect("bindings");
    for name in &dropped {
        bindings.remove(name);
        report.note(format!(
            "{path}: dropped the {TIME_BUS} binding for `{name}` (a `seconds` slot reads the \
             scope's time product directly)"
        ));
    }
    // Leaving `"bindings": {}` behind would be authored noise; an absent
    // bindings map means the same thing.
    if bindings.object().is_some_and(Vec::is_empty) {
        artifact.remove("bindings");
    }
    Ok(())
}

/// R8: the clock publishes the time product on `bus:time` from its new
/// `product` field; `seconds` stays, produced but unbound.
///
/// A `seconds` binding to any other channel is left alone — the field still
/// exists and still carries f32 seconds.
fn rename_clock_time_binding(path: &str, artifact: &mut JsonNode, report: &mut UpgradeReport) {
    let Some(bindings) = artifact.get_mut("bindings") else {
        return;
    };
    let publishes_time = bindings
        .get("seconds")
        .is_some_and(|binding| binding.has_string("target", TIME_BUS));
    if !publishes_time || bindings.get("product").is_some() {
        return;
    }
    bindings.rename_key("seconds", "product");
    report.note(format!(
        "{path}: Clock binding `seconds` → `product` ({TIME_BUS} carries the time product now)"
    ));
}

/// The refusal valve: after the rules have run, any surviving `bus:time`
/// reference is a shape this step does not understand.
///
/// Refusing loudly beats guessing. A wrong guess silently changes what a
/// project looks like, and the person who would notice is the one who no
/// longer has the old bytes.
fn refuse_unhandled_time_refs(
    file: &str,
    node: &JsonNode,
    kind: Option<&str>,
    pointer: &str,
) -> Result<(), UpgradeError> {
    match node {
        JsonNode::Scalar(_) => {
            if node.as_str().as_deref() != Some(TIME_BUS) {
                return Ok(());
            }
            if is_allowed_time_ref(kind, pointer) {
                return Ok(());
            }
            let owner = kind.unwrap_or("this file");
            Err(UpgradeError::Refused {
                file: String::from(file),
                reason: format!(
                    "`{TIME_BUS}` at `{pointer}` on {owner} is a shape this upgrade does not cover"
                ),
            })
        }
        JsonNode::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                refuse_unhandled_time_refs(file, item, kind, &join(pointer, &index.to_string()))?;
            }
            Ok(())
        }
        JsonNode::Object(members) => {
            // A nested node artifact starts its own pointer namespace, so an
            // inline node is checked against its own kind's rules.
            let (kind, pointer) = match node_kind(node) {
                Some(nested) if V4_NODE_KINDS.contains(&nested.as_str()) => (Some(nested), None),
                _ => (kind.map(String::from), Some(pointer)),
            };
            let base = pointer.unwrap_or("");
            for (name, child) in members {
                refuse_unhandled_time_refs(file, child, kind.as_deref(), &join(base, name))?;
            }
            Ok(())
        }
    }
}

/// The `bus:time` references that are correct at format 5.
fn is_allowed_time_ref(kind: Option<&str>, pointer: &str) -> bool {
    let mut segments = pointer.split('/');
    match (kind, segments.next(), segments.next(), segments.next()) {
        // R3: Playlist and Fluid consume the time product itself.
        (Some("Playlist" | "Fluid"), Some("bindings"), Some(_), Some("source")) => {
            segments.next().is_none()
        }
        // R8: the clock publishes it.
        (Some("Clock"), Some("bindings"), Some("product"), Some("target")) => {
            segments.next().is_none()
        }
        _ => false,
    }
}

fn node_kind(node: &JsonNode) -> Option<String> {
    node.get("kind").and_then(JsonNode::as_str)
}

fn join(pointer: &str, segment: &str) -> String {
    if pointer.is_empty() {
        String::from(segment)
    } else {
        format!("{pointer}/{segment}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_default_bound_time_uniform_becomes_a_seconds_slot() {
        let migrated = migrate_one(
            "idle.json",
            r#"{
  "kind": "Shader",
  "source": "idle.glsl",
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0,
      "label": "",
      "description": "",
      "default_bind": "bus:time"
    }
  }
}"#,
        )
        .expect("migrates");
        assert_eq!(
            migrated,
            r#"{
  "kind": "Shader",
  "source": "idle.glsl",
  "consumed": {
    "time": {
      "kind": "seconds",
      "value": "f32",
      "default": 0,
      "label": "",
      "description": ""
    }
  }
}
"#
        );
    }

    #[test]
    fn an_explicitly_bound_time_uniform_loses_its_binding() {
        let migrated = migrate_one(
            "sim.json",
            r#"{
  "kind": "ComputeShader",
  "bindings": {
    "speed": {
      "source": "bus:speed"
    },
    "time": {
      "source": "bus:time"
    }
  },
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0,
      "label": "Time",
      "description": "Project clock time in seconds"
    }
  }
}"#,
        )
        .expect("migrates");
        assert_eq!(
            migrated,
            r#"{
  "kind": "ComputeShader",
  "bindings": {
    "speed": {
      "source": "bus:speed"
    }
  },
  "consumed": {
    "time": {
      "kind": "seconds",
      "value": "f32",
      "default": 0,
      "label": "Time",
      "description": "Project clock time in seconds"
    }
  }
}
"#
        );
    }

    #[test]
    fn a_bindings_map_emptied_by_the_step_is_removed() {
        let migrated = migrate_one(
            "only.json",
            r#"{"kind":"Shader","bindings":{"time":{"source":"bus:time"}},"consumed":{"time":{"kind":"value"}}}"#,
        )
        .expect("migrates");
        assert_eq!(
            migrated,
            "{\n  \"kind\": \"Shader\",\n  \"consumed\": {\n    \"time\": {\n      \"kind\": \"seconds\"\n    }\n  }\n}\n"
        );
    }

    #[test]
    fn a_time_slot_bound_to_a_node_is_untouched() {
        // R10: `blast.json`'s `time` uniform reads the playlist entry's
        // elapsed time, not the clock. Keying off the name would break it.
        let source = r#"{
  "kind": "Shader",
  "bindings": {
    "time": {
      "source": "node:..#entry_time"
    }
  },
  "consumed": {
    "time": {
      "kind": "value",
      "value": "f32",
      "default": 0
    }
  }
}"#;
        assert_eq!(migrate_one("blast.json", source), None);
    }

    #[test]
    fn a_playlist_keeps_its_time_binding_and_loses_its_time_value() {
        let migrated = migrate_one(
            "playlist.json",
            r#"{
  "kind": "Playlist",
  "bindings": {
    "time": {
      "source": "bus:time"
    }
  },
  "time": 0,
  "idle_entry": 1
}"#,
        )
        .expect("migrates");
        assert_eq!(
            migrated,
            r#"{
  "kind": "Playlist",
  "bindings": {
    "time": {
      "source": "bus:time"
    }
  },
  "idle_entry": 1
}
"#
        );
    }

    #[test]
    fn a_clock_publishing_time_renames_its_binding() {
        let migrated = migrate_one(
            "clock.json",
            r#"{"kind":"Clock","bindings":{"seconds":{"target":"bus:time"},"delta_seconds":{"target":"bus:dt"}}}"#,
        )
        .expect("migrates");
        assert_eq!(
            migrated,
            "{\n  \"kind\": \"Clock\",\n  \"bindings\": {\n    \"product\": {\n      \"target\": \"bus:time\"\n    },\n    \"delta_seconds\": {\n      \"target\": \"bus:dt\"\n    }\n  }\n}\n"
        );
    }

    #[test]
    fn a_clock_seconds_binding_to_another_channel_stays() {
        assert_eq!(
            migrate_one(
                "clock.json",
                r#"{"kind":"Clock","bindings":{"seconds":{"target":"bus:elapsed"}}}"#
            ),
            None
        );
    }

    #[test]
    fn an_inline_node_is_migrated_in_place() {
        let migrated = migrate_one(
            "playlist.json",
            r#"{"kind":"Playlist","entries":{"1":{"node":{"kind":"Shader","consumed":{"t":{"kind":"value","default_bind":"bus:time"}}}}}}"#,
        )
        .expect("migrates");
        assert!(migrated.contains("\"kind\": \"seconds\""));
        assert!(!migrated.contains("bus:time"));
    }

    #[test]
    fn an_unknown_time_shape_is_refused_rather_than_guessed_at() {
        let refused = apply_to(
            "texture.json",
            r#"{"kind":"Texture","bindings":{"t":{"source":"bus:time"}}}"#,
        )
        .expect_err("refuses");
        let UpgradeError::Refused { file, reason } = refused else {
            panic!("expected a refusal, got {refused:?}");
        };
        assert_eq!(file, "texture.json");
        assert!(reason.contains("bindings/t/source"), "{reason}");

        let refused = apply_to(
            "shader.json",
            r#"{"kind":"Shader","consumed":{"m":{"kind":"map","default_bind":"bus:time"}}}"#,
        )
        .expect_err("refuses");
        assert!(
            matches!(&refused, UpgradeError::Refused { reason, .. } if reason.contains("`map` slot")),
            "{refused:?}"
        );
    }

    #[test]
    fn a_dead_default_bind_is_refused_rather_than_retyped() {
        // The binding wins, so this uniform never read the clock. Retyping it
        // to `seconds` would silently point it at the timebase instead.
        let refused = apply_to(
            "shader.json",
            r#"{"kind":"Shader","bindings":{"t":{"source":"bus:speed"}},"consumed":{"t":{"kind":"value","default_bind":"bus:time"}}}"#,
        )
        .expect_err("refuses");
        assert!(
            matches!(&refused, UpgradeError::Refused { reason, .. } if reason.contains("overrides it")),
            "{refused:?}"
        );
    }

    #[test]
    fn only_the_manifest_format_is_bumped() {
        // `*.map2d.json` carries its own unrelated `format` key.
        assert_eq!(
            migrate_one("fyeah.map2d.json", r#"{"format":1,"objects":[]}"#),
            None
        );
        assert_eq!(
            migrate_one("project.json", "{\n  \"format\": 4,\n  \"name\": \"x\"\n}"),
            Some(String::from("{\n  \"format\": 5,\n  \"name\": \"x\"\n}\n"))
        );
    }

    /// Runs the step over a one-file package; `None` means byte-identical.
    fn migrate_one(path: &str, source: &str) -> Option<String> {
        let (files, report) = apply_to(path, source).expect("migrates");
        if report.changed_files.is_empty() {
            assert_eq!(files.get(path), Some(source.as_bytes()));
            return None;
        }
        assert_eq!(report.changed_files, vec![String::from(path)]);
        Some(String::from_utf8(files.get(path).unwrap().to_vec()).unwrap())
    }

    fn apply_to(path: &str, source: &str) -> Result<(ProjectFiles, UpgradeReport), UpgradeError> {
        let mut files: ProjectFiles = [(path, source.as_bytes().to_vec())].into_iter().collect();
        let mut report = UpgradeReport::new(FROM);
        apply(&mut files, &mut report)?;
        report.to = TO;
        Ok((files, report))
    }
}
