//! **Runtime** graph nodes: the live counterpart to authored path and spec
//! types in [`crate::types`].
//!
//! A [`NodeProperties`] is an **object-safe** interface implemented by every concrete
//! on-graph object (see tests holding `Box<dyn NodeProperties>`). It combines a cheap
//! [`Uid`] with a stable [`NodePath`] and [`PropPath`]-keyed property access
//! over [`LpsValue`][`crate::LpsValue`] (`docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md` trait surface, `00-design` Node sketch).

use crate::LpsValue;
use crate::error::DomainError;
use crate::types::{NodePath, PropPath, Uid};

/// A **node instance** in the runtime graph: addressable, property-get/set.
pub trait NodeProperties {
    /// The process-local [`Uid`]; not the same as a [`NodePath`].
    fn uid(&self) -> Uid;
    /// Slash-joined path of `name.type` segments (see [`NodePath`]). Stable across sessions for authored content.
    fn path(&self) -> &NodePath;

    /// Read a property; paths use [`PropPath`] (dot fields and `[index]`).
    /// Errors map to [`DomainError`] (e.g. unknown key or type mismatch when setting).
    fn get_property(&self, prop: &PropPath) -> Result<LpsValue, DomainError>;
    /// Write a property, validating shape against the domain model where applicable.
    fn set_property(&mut self, prop: &PropPath, value: LpsValue) -> Result<(), DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use alloc::vec;

    struct DummyNode {
        uid: Uid,
        path: NodePath,
        speed: f32,
    }

    impl NodeProperties for DummyNode {
        fn uid(&self) -> Uid {
            self.uid
        }
        fn path(&self) -> &NodePath {
            &self.path
        }

        fn get_property(&self, prop: &PropPath) -> Result<LpsValue, DomainError> {
            match prop.first() {
                Some(crate::types::prop_path::Segment::Field(name)) if name == "speed" => {
                    Ok(LpsValue::F32(self.speed))
                }
                _ => Err(DomainError::UnknownProperty(prop_path_to_string(prop))),
            }
        }

        fn set_property(&mut self, prop: &PropPath, value: LpsValue) -> Result<(), DomainError> {
            match prop.first() {
                Some(crate::types::prop_path::Segment::Field(name)) if name == "speed" => {
                    match value {
                        LpsValue::F32(v) => {
                            self.speed = v;
                            Ok(())
                        }
                        other => Err(DomainError::PropertyTypeMismatch {
                            expected: "F32".to_string(),
                            actual: alloc::format!("{other:?}"),
                        }),
                    }
                }
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
                crate::types::prop_path::Segment::Field(n) => out.push_str(n),
                crate::types::prop_path::Segment::Index(idx) => {
                    out.push_str(&alloc::format!("[{idx}]"));
                }
            }
        }
        out
    }

    #[test]
    fn node_is_object_safe() {
        let node: alloc::boxed::Box<dyn NodeProperties> = alloc::boxed::Box::new(DummyNode {
            uid: Uid(1),
            path: NodePath::parse("/main.show").unwrap(),
            speed: 1.0,
        });
        assert_eq!(node.uid(), Uid(1));
        assert_eq!(node.path().to_string(), "/main.show");
    }

    #[test]
    fn dummy_node_round_trips_speed() {
        let mut node = DummyNode {
            uid: Uid(7),
            path: NodePath::parse("/main.show").unwrap(),
            speed: 1.0,
        };
        let prop = vec![crate::types::prop_path::Segment::Field("speed".into())];
        node.set_property(&prop, LpsValue::F32(3.5)).unwrap();
        match node.get_property(&prop).unwrap() {
            LpsValue::F32(v) => assert!((v - 3.5f32).abs() < 1e-5),
            other => panic!("expected F32, got {other:?}"),
        }
    }
}
