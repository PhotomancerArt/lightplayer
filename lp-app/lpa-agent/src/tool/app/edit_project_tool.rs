//! The `edit_project` tool: the app agent's one content vocabulary (D14).
//!
//! One call carries an ordered list of edits — node create/import/remove
//! and slot/asset edits — that the host applies through the same ops a
//! user's clicks take, into the same unsaved overlay. Each edit gets its
//! own result, in order, so a model sees exactly which of its edits landed
//! and why the others did not.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::provider::model_provider::ToolDef;
use crate::tool::app::app_agent_host::AppAgentHost;
use crate::tool::app::app_tool_schema::app_tool_schema;
use crate::toolset::ToolOutcome;

pub const EDIT_PROJECT_TOOL_NAME: &str = "edit_project";

/// The tool input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct EditProjectInput {
    /// One line: why these edits (shown to the user beside the edit row).
    #[serde(default)]
    pub note: Option<String>,
    /// The edits, applied in order. A later edit may name a node an earlier
    /// one in the same call created.
    pub edits: Vec<ProjectEdit>,
    /// Save the project after the edits (the user's Save: the unsaved edits
    /// become the saved project). Use it once the project is complete.
    #[serde(default)]
    pub save: bool,
}

/// One edit. Exactly one key per edit object.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProjectEdit {
    /// Add a new node of `kind` with the kind's starter contents. Committed
    /// immediately (not an unsaved edit), like the add-node menu.
    CreateNode(CreateNodeEdit),
    /// Copy a catalog pattern into the project as a module node, at the
    /// project root or as the next entry of a playlist.
    ImportPattern(ImportPatternEdit),
    /// Remove a node (an unsaved edit until Save).
    RemoveNode(NodeRef),
    /// Set one value in a node's definition.
    Set(SetEdit),
    /// Make an optional value present with its defaults (e.g. a playlist's
    /// `cycle`), so its fields can then be `set`.
    Ensure(SlotRef),
    /// Remove one value (an optional field, or a map entry) from a node's
    /// definition.
    Remove(SlotRef),
    /// Replace the whole text of a file a node references (e.g. a fixture's
    /// `.map2d.json` mapping).
    SetAsset(SetAssetEdit),
    /// Set which board the project is for (the project's `target`).
    SetTarget(SetTargetEdit),
}

/// Add a node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct CreateNodeEdit {
    /// The node kind: Clock, Playlist, Fixture, Output, Shader, Module, ….
    pub kind: String,
    /// Where to attach: omitted = the project root; a playlist node's name
    /// = that playlist's next entry.
    #[serde(default)]
    pub in_playlist: Option<String>,
}

/// Import a catalog pattern.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportPatternEdit {
    /// The catalog pattern's slug (`palette-waves`).
    pub pattern: String,
    /// Omitted = the project root; a playlist node's name = that
    /// playlist's next entry.
    #[serde(default)]
    pub in_playlist: Option<String>,
}

/// A node, by name (`fixture`) or by path of names
/// (`playlist/palette_waves/shader`) when a name is ambiguous.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NodeRef {
    pub node: String,
}

/// A value inside a node's definition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SlotRef {
    pub node: String,
    /// Slot path in the node's definition: `render_size`,
    /// `ports[0].endpoint`, `cycle`, `bindings[time]`, `entries[2]`.
    pub path: String,
}

/// Set one value.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetEdit {
    pub node: String,
    /// Slot path in the node's definition.
    pub path: String,
    /// The value, as it appears in the node's JSON file.
    pub value: Value,
}

/// Replace a referenced file's text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetAssetEdit {
    pub node: String,
    /// The file as the node's definition references it
    /// (`fixture.map2d.json`).
    pub file: String,
    /// The file's complete new text.
    pub text: String,
}

/// Set the project's board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetTargetEdit {
    /// A board id from the board list (`seeed/xiao-esp32-c6`).
    pub board: String,
}

impl ProjectEdit {
    /// The edit's key, for results.
    pub fn verb(&self) -> &'static str {
        match self {
            Self::CreateNode(_) => "create_node",
            Self::ImportPattern(_) => "import_pattern",
            Self::RemoveNode(_) => "remove_node",
            Self::Set(_) => "set",
            Self::Ensure(_) => "ensure",
            Self::Remove(_) => "remove",
            Self::SetAsset(_) => "set_asset",
            Self::SetTarget(_) => "set_target",
        }
    }
}

/// What the host did with one edit.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EditStatus {
    /// Applied. `detail` names what changed (for a create: the new node's
    /// name, which later edits use).
    Applied { detail: String },
    /// Refused, with the reason — the edit changed nothing.
    Rejected { reason: String },
    /// Not attempted: an earlier node create/import failed, so this edit
    /// could address a node that does not exist.
    Skipped { reason: String },
}

/// The host's answer to one `edit_project` call.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ProjectEditsOutcome {
    /// One per input edit, in order.
    pub results: Vec<EditStatus>,
    /// The save's outcome when `save` was asked (`Err` = why not).
    pub saved: Option<Result<(), String>>,
    /// The project after the edits, compact (statuses, outputs, unsaved),
    /// when the host can report it.
    pub project: Option<Value>,
    /// Where each edit landed, one per input edit when the host knows it:
    /// the node's tree path (`/demo.module/fixture.fixture`), or `None`
    /// for an edit that touched no node (the board) or did not land. It
    /// rides the tool row's summary so the chat can link each edit to its
    /// node; the model never sees it (it addresses nodes by name).
    #[serde(skip)]
    pub nodes: Vec<Option<String>>,
}

pub fn edit_project_tool_def() -> ToolDef {
    ToolDef {
        name: EDIT_PROJECT_TOOL_NAME.into(),
        description: DESCRIPTION.into(),
        input_schema: app_tool_schema::<EditProjectInput>(),
    }
}

const DESCRIPTION: &str = "\
Change the open project: add, import or remove nodes, set values in their \
definitions, replace a referenced file, set the board. Pass many edits in \
one call, in order; a later edit can name a node an earlier edit created \
(each create's result says the new node's name). Every edit is the same \
change the user could make in the editor and lands as an unsaved edit they \
can see and revert (node creation is committed immediately, like the \
add-node menu). Each edit gets its own result: `applied`, `rejected` with \
the reason, or `skipped` (an earlier create failed). Set `save: true` to \
save once the project is complete.";

/// Run one `edit_project` call against `host`.
pub async fn run_edit_project(input_json: &Value, host: &mut dyn AppAgentHost) -> ToolOutcome {
    let input: EditProjectInput = match serde_json::from_value(input_json.clone()) {
        Ok(input) => input,
        Err(error) => {
            return ToolOutcome {
                content: json!({
                    "error": format!("invalid edit_project input: {error}"),
                    "hint": "pass {\"edits\": [{\"<verb>\": {…}}, …]} with one key per edit"
                })
                .to_string(),
                is_error: false,
                summary: json!({ "input_error": true }),
            };
        }
    };
    let note = input.note.clone();
    match host.apply_project_edits(&input).await {
        Ok(outcome) => {
            let applied = outcome
                .results
                .iter()
                .filter(|status| matches!(status, EditStatus::Applied { .. }))
                .count();
            let results: Vec<Value> = input
                .edits
                .iter()
                .zip(&outcome.results)
                .enumerate()
                .map(|(index, (edit, status))| {
                    let mut row = json!({ "index": index, "edit": edit.verb() });
                    match status {
                        EditStatus::Applied { detail } => {
                            row["ok"] = true.into();
                            row["detail"] = detail.clone().into();
                        }
                        EditStatus::Rejected { reason } => {
                            row["ok"] = false.into();
                            row["reason"] = reason.clone().into();
                        }
                        EditStatus::Skipped { reason } => {
                            row["ok"] = false.into();
                            row["skipped"] = true.into();
                            row["reason"] = reason.clone().into();
                        }
                    }
                    row
                })
                .collect();
            let mut content = json!({ "results": results });
            if let Some(saved) = &outcome.saved {
                content["saved"] = match saved {
                    Ok(()) => true.into(),
                    Err(reason) => json!({ "ok": false, "reason": reason }),
                };
            }
            if let Some(project) = outcome.project {
                content["project"] = project;
            }
            let rows: Vec<Value> = input
                .edits
                .iter()
                .zip(&outcome.results)
                .enumerate()
                .map(|(index, (edit, status))| {
                    let node = outcome.nodes.get(index).cloned().flatten();
                    edit_summary_row(edit, status, node)
                })
                .collect();
            ToolOutcome {
                content: content.to_string(),
                is_error: false,
                summary: json!({
                    "note": note,
                    "edits": input.edits.len(),
                    "applied": applied,
                    "saved": matches!(outcome.saved, Some(Ok(()))),
                    "save_error": match &outcome.saved {
                        Some(Err(reason)) => Some(reason.clone()),
                        _ => None,
                    },
                    "rows": rows,
                }),
            }
        }
        Err(error) => ToolOutcome {
            content: json!({ "error": error.message }).to_string(),
            is_error: true,
            summary: json!({ "note": note, "error": "host error" }),
        },
    }
}

/// One edit, compact, for the tool row's expandable list: what kind of
/// edit, what it was about, how it went, and — when the host said — the
/// node it landed on (`node`, a tree path), so the row can link to it. The
/// UI words it; this only carries the facts (the edit's own fields, never
/// the whole asset text).
fn edit_summary_row(edit: &ProjectEdit, status: &EditStatus, node: Option<String>) -> Value {
    let mut row = json!({ "edit": edit.verb() });
    if let Some(node) = node {
        row["node"] = node.into();
    }
    match edit {
        ProjectEdit::CreateNode(create) => {
            row["target"] = create.kind.clone().into();
            row["in"] = create.in_playlist.clone().into();
        }
        ProjectEdit::ImportPattern(import) => {
            row["target"] = import.pattern.clone().into();
            row["in"] = import.in_playlist.clone().into();
        }
        ProjectEdit::RemoveNode(node) => row["target"] = node.node.clone().into(),
        ProjectEdit::Set(set) => {
            row["target"] = set.node.clone().into();
            row["path"] = set.path.clone().into();
            row["value"] = set.value.clone();
        }
        ProjectEdit::Ensure(slot) | ProjectEdit::Remove(slot) => {
            row["target"] = slot.node.clone().into();
            row["path"] = slot.path.clone().into();
        }
        ProjectEdit::SetAsset(asset) => {
            row["target"] = asset.node.clone().into();
            row["path"] = asset.file.clone().into();
        }
        ProjectEdit::SetTarget(target) => row["target"] = target.board.clone().into(),
    }
    match status {
        EditStatus::Applied { detail } => {
            row["ok"] = true.into();
            row["detail"] = detail.clone().into();
        }
        EditStatus::Rejected { reason } => {
            row["ok"] = false.into();
            row["reason"] = reason.clone().into();
        }
        EditStatus::Skipped { reason } => {
            row["ok"] = false.into();
            row["skipped"] = true.into();
            row["reason"] = reason.clone().into();
        }
    }
    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_schema_is_strict_and_inlined() {
        let schema = edit_project_tool_def().input_schema;
        let text = schema.to_string();
        assert!(!text.contains("$ref"), "{text}");
        assert_eq!(schema["additionalProperties"], false, "{schema:#}");
        for verb in [
            "create_node",
            "import_pattern",
            "remove_node",
            "set",
            "ensure",
            "remove",
            "set_asset",
            "set_target",
        ] {
            assert!(text.contains(&format!("\"{verb}\"")), "{verb}: {text}");
        }
    }

    #[test]
    fn edits_parse_and_unknown_fields_are_refused() {
        let input: EditProjectInput = serde_json::from_value(json!({
            "note": "sean",
            "edits": [
                { "create_node": { "kind": "Playlist" } },
                { "import_pattern": { "pattern": "spiral", "in_playlist": "playlist" } },
                { "set": { "node": "output", "path": "ports[0].endpoint", "value": "ws281x:local:D6" } },
                { "set_target": { "board": "seeed/xiao-esp32-c6" } }
            ],
            "save": true
        }))
        .expect("parses");
        assert_eq!(input.edits.len(), 4);
        assert!(input.save);
        assert!(
            serde_json::from_value::<EditProjectInput>(json!({
                "edits": [{ "set": { "node": "a", "path": "b", "value": 1, "extra": 2 } }]
            }))
            .is_err()
        );
        assert!(
            serde_json::from_value::<EditProjectInput>(json!({
                "edits": [{ "teleport": {} }]
            }))
            .is_err()
        );
    }

    #[test]
    fn the_summary_carries_one_row_per_edit_with_its_outcome() {
        let set = ProjectEdit::Set(SetEdit {
            node: "output".into(),
            path: "ports[0].endpoint".into(),
            value: json!("ws281x:local:D6"),
        });
        let row = edit_summary_row(
            &set,
            &EditStatus::Applied {
                detail: "set".into(),
            },
            Some("/demo.module/output.output".into()),
        );
        assert_eq!(row["edit"], "set");
        assert_eq!(row["target"], "output");
        assert_eq!(row["path"], "ports[0].endpoint");
        assert_eq!(row["value"], "ws281x:local:D6");
        assert_eq!(row["ok"], true);
        assert_eq!(row["node"], "/demo.module/output.output");

        let import = ProjectEdit::ImportPattern(ImportPatternEdit {
            pattern: "spiral".into(),
            in_playlist: Some("playlist".into()),
        });
        let row = edit_summary_row(
            &import,
            &EditStatus::Skipped {
                reason: "an earlier create failed".into(),
            },
            None,
        );
        assert_eq!(row["target"], "spiral");
        assert_eq!(row["in"], "playlist");
        assert_eq!(row["ok"], false);
        assert_eq!(row["skipped"], true);
        assert_eq!(row["reason"], "an earlier create failed");
    }
}
