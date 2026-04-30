use crate::prop::prop_path::PropPath;
use crate::tree::tree_path::TreePath;
use crate::{DomainError, NodeId, WireValue};

/// A **node instance** in the runtime graph: addressable, property-get/set.
pub trait NodeProps {
    /// The process-local [`NodeId`]; not the same as a [`TreePath`].
    fn uid(&self) -> NodeId;
    /// Slash-joined path of `name.type` segments (see [`TreePath`]). Stable across sessions for authored content.
    fn path(&self) -> &TreePath;

    /// Read a property; paths use [`PropPath`] (dot fields and `[index]`).
    /// Errors map to [`DomainError`] (e.g. unknown key or type mismatch when setting).
    fn get_property(&self, prop: &PropPath) -> Result<WireValue, DomainError>;
    /// Write a property, validating shape against the domain model where applicable.
    fn set_property(&mut self, prop: &PropPath, value: WireValue) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::NodeProps;
    use crate::tree::tree_path::TreePath;
    use crate::{DomainError, NodeId, PropPath, WireValue};
    use alloc::string::{String, ToString};
    use alloc::vec;

    struct DummyNode {
        uid: NodeId,
        path: TreePath,
        speed: f32,
    }

    impl NodeProps for DummyNode {
        fn uid(&self) -> NodeId {
            self.uid
        }
        fn path(&self) -> &TreePath {
            &self.path
        }

        fn get_property(&self, prop: &PropPath) -> Result<WireValue, DomainError> {
            match prop.first() {
                Some(crate::prop::Segment::Field(name)) if name == "speed" => {
                    Ok(WireValue::F32(self.speed))
                }
                _ => Err(DomainError::UnknownProperty(prop_path_to_string(prop))),
            }
        }

        fn set_property(&mut self, prop: &PropPath, value: WireValue) -> Result<(), DomainError> {
            match prop.first() {
                Some(crate::prop::Segment::Field(name)) if name == "speed" => match value {
                    WireValue::F32(v) => {
                        self.speed = v;
                        Ok(())
                    }
                    other => Err(DomainError::PropertyTypeMismatch {
                        expected: "F32".to_string(),
                        actual: alloc::format!("{other:?}"),
                    }),
                },
                _ => Err(DomainError::UnknownProperty(prop_path_to_string(prop))),
            }
        }
    }

    fn prop_path_to_string(p: &PropPath) -> String {
        let mut out = String::new();
        for (i, seg) in p.iter().enumerate() {
            if i > 0 {
                out.push('.');
            }
            match seg {
                crate::prop::Segment::Field(n) => out.push_str(n),
                crate::prop::Segment::Index(idx) => {
                    out.push_str(&alloc::format!("[{idx}]"));
                }
            }
        }
        out
    }

    #[test]
    fn node_is_object_safe() {
        let node: alloc::boxed::Box<dyn NodeProps> = alloc::boxed::Box::new(DummyNode {
            uid: NodeId(1),
            path: TreePath::parse("/main.show").unwrap(),
            speed: 1.0,
        });
        assert_eq!(node.uid(), NodeId(1));
        assert_eq!(node.path().to_string(), "/main.show");
    }

    #[test]
    fn dummy_node_round_trips_speed() {
        let mut node = DummyNode {
            uid: NodeId(7),
            path: TreePath::parse("/main.show").unwrap(),
            speed: 1.0,
        };
        let prop = vec![crate::prop::Segment::Field(String::from("speed"))];
        node.set_property(&prop, WireValue::F32(3.5)).unwrap();
        match node.get_property(&prop).unwrap() {
            WireValue::F32(v) => assert!((v - 3.5f32).abs() < 1e-5),
            other => panic!("expected F32, got {other:?}"),
        }
    }
}
