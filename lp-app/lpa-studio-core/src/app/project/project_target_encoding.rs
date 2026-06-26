//! Readable `ControllerId` encoding for project editor action targets.

use lpc_model::{NodeId, SlotPath, TreePath};

use crate::{
    ControllerId, ProjectNodeAddress, ProjectNodeTarget, ProjectSlotAddress, ProjectSlotRoot,
    UiError,
};

/// Typed project target decoded from a controller id tail.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DecodedProjectTarget {
    Node(ProjectNodeTarget),
    Slot {
        node: ProjectNodeTarget,
        slot: ProjectSlotAddress,
    },
}

/// Build a controller id for a typed node target.
pub fn node_target_id(root: &ControllerId, target: &ProjectNodeTarget) -> ControllerId {
    root.child("node")
        .child("nid")
        .child(target.node_id.to_string())
        .child("path")
        .child(encode_payload(&target.address.path().to_string()))
}

/// Build a controller id for a typed slot target.
pub fn slot_target_id(
    root: &ControllerId,
    target: &ProjectNodeTarget,
    slot: &ProjectSlotAddress,
) -> ControllerId {
    debug_assert_eq!(&target.address, &slot.node);
    let id = node_target_id(root, target)
        .child("slot")
        .child(encode_slot_root(&slot.root));
    if slot.path.is_root() {
        id.child("root")
    } else {
        id.child("path")
            .child(encode_payload(&slot.path.to_string()))
    }
}

/// Decode a typed project target from segments after `studio|project`.
pub fn decode_typed_project_target(
    segments: &[&str],
) -> Result<Option<DecodedProjectTarget>, UiError> {
    if !matches!(segments, ["node", "nid", ..]) {
        return Ok(None);
    }

    let (node, consumed) = decode_node_prefix(segments)?;
    match &segments[consumed..] {
        [] => Ok(Some(DecodedProjectTarget::Node(node))),
        ["slot", root, "root"] => Ok(Some(DecodedProjectTarget::Slot {
            slot: ProjectSlotAddress::root(node.address.clone(), decode_slot_root(root)?),
            node,
        })),
        ["slot", root, "path", path] => {
            let path = decode_payload(path)?;
            let path = SlotPath::parse(&path).map_err(|error| {
                project_target_error(format!("invalid project slot path `{path}`: {error}"))
            })?;
            Ok(Some(DecodedProjectTarget::Slot {
                slot: ProjectSlotAddress::new(node.address.clone(), decode_slot_root(root)?, path),
                node,
            }))
        }
        _ => Err(project_target_error("malformed typed project target")),
    }
}

fn decode_node_prefix(segments: &[&str]) -> Result<(ProjectNodeTarget, usize), UiError> {
    let ["node", "nid", node_id, "path", path, ..] = segments else {
        return Err(project_target_error("malformed typed project node target"));
    };
    let node_id = node_id.parse::<u32>().map_err(|error| {
        project_target_error(format!("invalid project node id `{node_id}`: {error}"))
    })?;
    let path = decode_payload(path)?;
    let path = TreePath::parse(&path).map_err(|error| {
        project_target_error(format!("invalid project node path `{path}`: {error}"))
    })?;
    Ok((
        ProjectNodeTarget::new(ProjectNodeAddress::new(path), NodeId::new(node_id)),
        5,
    ))
}

fn encode_slot_root(root: &ProjectSlotRoot) -> String {
    match root {
        ProjectSlotRoot::Def => "def".to_string(),
        ProjectSlotRoot::State => "state".to_string(),
        ProjectSlotRoot::Other(name) => format!("other:{}", encode_payload(name)),
    }
}

fn decode_slot_root(value: &str) -> Result<ProjectSlotRoot, UiError> {
    match value {
        "def" => Ok(ProjectSlotRoot::Def),
        "state" => Ok(ProjectSlotRoot::State),
        value if value.starts_with("other:") => {
            let encoded = value.trim_start_matches("other:");
            Ok(ProjectSlotRoot::Other(decode_payload(encoded)?))
        }
        value => Err(project_target_error(format!(
            "unknown project slot root `{value}`"
        ))),
    }
}

fn encode_payload(value: &str) -> String {
    let mut output = String::new();
    for byte in value.as_bytes() {
        let byte = *byte;
        if byte == b'|' || byte == b'%' || !byte.is_ascii() || byte < 0x20 {
            output.push('%');
            output.push(HEX[(byte >> 4) as usize] as char);
            output.push(HEX[(byte & 0x0f) as usize] as char);
        } else {
            output.push(byte as char);
        }
    }
    output
}

fn decode_payload(value: &str) -> Result<String, UiError> {
    let mut bytes = Vec::new();
    let input = value.as_bytes();
    let mut index = 0;
    while index < input.len() {
        if input[index] != b'%' {
            bytes.push(input[index]);
            index += 1;
            continue;
        }
        if index + 2 >= input.len() {
            return Err(project_target_error(format!(
                "invalid percent escape in `{value}`"
            )));
        }
        let high = decode_hex(input[index + 1])
            .ok_or_else(|| project_target_error(format!("invalid percent escape in `{value}`")))?;
        let low = decode_hex(input[index + 2])
            .ok_or_else(|| project_target_error(format!("invalid percent escape in `{value}`")))?;
        bytes.push((high << 4) | low);
        index += 3;
    }
    String::from_utf8(bytes)
        .map_err(|error| project_target_error(format!("invalid utf-8 target payload: {error}")))
}

fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn project_target_error(message: impl Into<String>) -> UiError {
    UiError::UnsupportedAction(message.into())
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

#[cfg(test)]
mod tests {
    use lpc_model::{NodeId, SlotMapKey, SlotPath, SlotPathSegment};

    use super::*;

    #[test]
    fn node_target_round_trips_with_node_id_and_path() {
        let root = ControllerId::new("studio|project");
        let target = node_target(3, "/demo.project/orbit.shader");
        let id = node_target_id(&root, &target);

        assert_eq!(
            id.as_str(),
            "studio|project|node|nid|3|path|/demo.project/orbit.shader"
        );
        assert_eq!(
            decode_typed_project_target(&tail(&id)).unwrap(),
            Some(DecodedProjectTarget::Node(target))
        );
    }

    #[test]
    fn root_slot_target_round_trips() {
        let root = ControllerId::new("studio|project");
        let target = node_target(3, "/demo.project/orbit.shader");
        let slot = ProjectSlotAddress::root(target.address.clone(), ProjectSlotRoot::def());
        let id = slot_target_id(&root, &target, &slot);

        assert_eq!(
            id.as_str(),
            "studio|project|node|nid|3|path|/demo.project/orbit.shader|slot|def|root"
        );
        assert_eq!(
            decode_typed_project_target(&tail(&id)).unwrap(),
            Some(DecodedProjectTarget::Slot { node: target, slot })
        );
    }

    #[test]
    fn field_slot_path_target_round_trips() {
        let root = ControllerId::new("studio|project");
        let target = node_target(3, "/demo.project/orbit.shader");
        let slot = ProjectSlotAddress::new(
            target.address.clone(),
            ProjectSlotRoot::def(),
            SlotPath::parse("config.brightness").unwrap(),
        );
        let id = slot_target_id(&root, &target, &slot);

        assert_eq!(
            id.as_str(),
            "studio|project|node|nid|3|path|/demo.project/orbit.shader|slot|def|path|config.brightness"
        );
        assert_eq!(
            decode_typed_project_target(&tail(&id)).unwrap(),
            Some(DecodedProjectTarget::Slot { node: target, slot })
        );
    }

    #[test]
    fn string_map_key_with_dots_round_trips() {
        let root = ControllerId::new("studio|project");
        let target = node_target(3, "/demo.project/orbit.shader");
        let path = SlotPath::parse(r#"params["phase.offset"].label"#).unwrap();
        let slot = ProjectSlotAddress::new(target.address.clone(), ProjectSlotRoot::def(), path);
        let id = slot_target_id(&root, &target, &slot);

        assert_eq!(
            id.as_str(),
            r#"studio|project|node|nid|3|path|/demo.project/orbit.shader|slot|def|path|params["phase.offset"].label"#
        );
        assert_eq!(
            decode_typed_project_target(&tail(&id)).unwrap(),
            Some(DecodedProjectTarget::Slot { node: target, slot })
        );
    }

    #[test]
    fn payload_escaping_round_trips_pipe_and_percent() {
        let root = ControllerId::new("studio|project");
        let target = node_target(3, "/demo.project/orbit.shader");
        let path = SlotPath::parse("params")
            .unwrap()
            .child_segment(SlotPathSegment::Key(SlotMapKey::String("a|b%".to_string())));
        let slot = ProjectSlotAddress::new(target.address.clone(), ProjectSlotRoot::def(), path);
        let id = slot_target_id(&root, &target, &slot);

        assert_eq!(
            id.as_str(),
            "studio|project|node|nid|3|path|/demo.project/orbit.shader|slot|def|path|params[a%7Cb%25]"
        );
        assert_eq!(
            decode_typed_project_target(&tail(&id)).unwrap(),
            Some(DecodedProjectTarget::Slot { node: target, slot })
        );
    }

    #[test]
    fn numeric_map_key_round_trips() {
        let root = ControllerId::new("studio|project");
        let target = node_target(3, "/demo.project/orbit.shader");
        let path = SlotPath::parse("touches")
            .unwrap()
            .child_segment(SlotPathSegment::Key(SlotMapKey::U32(2)));
        let slot = ProjectSlotAddress::new(target.address.clone(), ProjectSlotRoot::state(), path);
        let id = slot_target_id(&root, &target, &slot);

        assert_eq!(
            decode_typed_project_target(&tail(&id)).unwrap(),
            Some(DecodedProjectTarget::Slot { node: target, slot })
        );
    }

    #[test]
    fn other_slot_root_round_trips() {
        let root = ControllerId::new("studio|project");
        let target = node_target(3, "/demo.project/orbit.shader");
        let slot = ProjectSlotAddress::root(
            target.address.clone(),
            ProjectSlotRoot::Other("runtime|debug%root".to_string()),
        );
        let id = slot_target_id(&root, &target, &slot);

        assert_eq!(
            id.as_str(),
            "studio|project|node|nid|3|path|/demo.project/orbit.shader|slot|other:runtime%7Cdebug%25root|root"
        );
        assert_eq!(
            decode_typed_project_target(&tail(&id)).unwrap(),
            Some(DecodedProjectTarget::Slot { node: target, slot })
        );
    }

    #[test]
    fn malformed_target_is_rejected() {
        let error =
            decode_typed_project_target(&["node", "nid", "3", "path"]).expect_err("invalid");

        assert!(matches!(error, UiError::UnsupportedAction(_)));
    }

    fn tail(id: &ControllerId) -> Vec<&str> {
        id.strip_prefix(&ControllerId::new("studio|project"))
            .unwrap()
            .iter()
            .collect()
    }

    fn node_target(id: u32, path: &str) -> ProjectNodeTarget {
        ProjectNodeTarget::new(ProjectNodeAddress::parse(path).unwrap(), NodeId::new(id))
    }
}
