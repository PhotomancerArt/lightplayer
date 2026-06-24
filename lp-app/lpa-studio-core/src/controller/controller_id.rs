use core::fmt;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ControllerId {
    value: String,
    segments: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UxNodePath<'a> {
    segments: &'a [String],
}

impl ControllerId {
    pub fn new(value: impl Into<String>) -> Self {
        let value = value.into();
        let segments = parse_dotted_path(&value);
        Self { value, segments }
    }

    pub fn from_segments(segments: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let segments = segments.into_iter().map(Into::into).collect::<Vec<_>>();
        validate_segments(&segments);
        let value = segments.join(".");
        Self { value, segments }
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    pub fn segments(&self) -> UxNodePath<'_> {
        UxNodePath {
            segments: &self.segments,
        }
    }

    pub fn child(&self, segment: impl Into<String>) -> Self {
        let segment = segment.into();
        validate_segment(&segment);
        let mut segments = self.segments.clone();
        segments.push(segment);
        Self::from_segments(segments)
    }

    pub fn is_descendant_of(&self, parent: &ControllerId) -> bool {
        self.segments.len() > parent.segments.len() && self.segments.starts_with(&parent.segments)
    }

    pub fn is_self_or_descendant_of(&self, parent: &ControllerId) -> bool {
        self.segments.starts_with(&parent.segments)
    }

    pub fn strip_prefix<'a>(&'a self, parent: &ControllerId) -> Option<UxNodePath<'a>> {
        self.is_self_or_descendant_of(parent).then(|| UxNodePath {
            segments: &self.segments[parent.segments.len()..],
        })
    }
}

impl fmt::Display for ControllerId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl From<String> for ControllerId {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for ControllerId {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

impl<'a> UxNodePath<'a> {
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn get(&self, index: usize) -> Option<&'a str> {
        self.segments.get(index).map(String::as_str)
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = &'a str> {
        self.segments.iter().map(String::as_str)
    }

    pub fn as_slice(&self) -> &'a [String] {
        self.segments
    }
}

fn parse_dotted_path(value: &str) -> Vec<String> {
    let segments = value
        .split('.')
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    validate_segments(&segments);
    segments
}

fn validate_segments(segments: &[String]) {
    assert!(
        !segments.is_empty(),
        "UX node id must have at least one segment"
    );
    for segment in segments {
        validate_segment(segment);
    }
}

fn validate_segment(segment: &str) {
    assert!(!segment.is_empty(), "UX node id segments must not be empty");
    assert!(
        !segment.contains('.'),
        "UX node id segments must not contain dots"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_static_id_keeps_display_and_segments() {
        let node_id = ControllerId::new("studio.project");

        assert_eq!(node_id.as_str(), "studio.project");
        assert_eq!(node_id.to_string(), "studio.project");
        assert_eq!(
            node_id.segments().iter().collect::<Vec<_>>(),
            vec!["studio", "project"]
        );
    }

    #[test]
    fn from_segments_builds_dotted_display() {
        let node_id = ControllerId::from_segments(["studio", "project", "node_tree"]);

        assert_eq!(node_id.as_str(), "studio.project.node_tree");
    }

    #[test]
    fn child_appends_one_segment() {
        let node_id = ControllerId::new("studio.project").child("node_tree");

        assert_eq!(node_id.as_str(), "studio.project.node_tree");
    }

    #[test]
    fn descendant_checks_are_strict() {
        let project = ControllerId::new("studio.project");
        let node_tree = project.child("node_tree");
        let device = ControllerId::new("studio.device");

        assert!(node_tree.is_descendant_of(&project));
        assert!(node_tree.is_self_or_descendant_of(&project));
        assert!(project.is_self_or_descendant_of(&project));
        assert!(!project.is_descendant_of(&project));
        assert!(!device.is_descendant_of(&project));
    }

    #[test]
    fn strip_prefix_returns_tail_segments() {
        let project = ControllerId::new("studio.project");
        let slot = project
            .child("node")
            .child("4")
            .child("slot")
            .child("brightness");

        let tail = slot.strip_prefix(&project).unwrap();

        assert_eq!(
            tail.iter().collect::<Vec<_>>(),
            vec!["node", "4", "slot", "brightness"]
        );
    }

    #[test]
    #[should_panic(expected = "UX node id segments must not contain dots")]
    fn child_rejects_dotted_segment() {
        let _ = ControllerId::new("studio.project").child("node.tree");
    }
}
