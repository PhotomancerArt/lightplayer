//! Top-level slot-tree namespace addressed by a `PropPath`.
//!
//! Every node's slots are partitioned across four namespaces:
//! `params` and `inputs` are *consumed* (bound from outside);
//! `outputs` and `state` are *produced* (written by the node).
//! See [`design/05-slots-and-props.md`](../../docs/roadmaps/2026-04-28-node-runtime/design/05-slots-and-props.md).

use crate::prop::prop_path::{PropPath, Segment};

/// Top-level slot-tree namespace addressed by a `PropPath`.
///
/// Every node's slots are partitioned across four namespaces:
/// `params` and `inputs` are *consumed* (bound from outside);
/// `outputs` and `state` are *produced* (written by the node).
/// See [`design/05-slots-and-props.md`](../../docs/roadmaps/2026-04-28-node-runtime/design/05-slots-and-props.md).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PropNamespace {
    Params,
    Inputs,
    Outputs,
    State,
}

impl PropNamespace {
    /// The leading `PropPath` segment that identifies this namespace.
    pub fn segment_name(self) -> &'static str {
        match self {
            PropNamespace::Params => "params",
            PropNamespace::Inputs => "inputs",
            PropNamespace::Outputs => "outputs",
            PropNamespace::State => "state",
        }
    }

    /// Project a `PropPath`'s top-level segment to a `PropNamespace`,
    /// or `None` if the head segment isn't a namespace name.
    pub fn from_prop_path(path: &PropPath) -> Option<Self> {
        match path.first()? {
            Segment::Field(name) => match name.as_str() {
                "params" => Some(PropNamespace::Params),
                "inputs" => Some(PropNamespace::Inputs),
                "outputs" => Some(PropNamespace::Outputs),
                "state" => Some(PropNamespace::State),
                _ => None,
            },
            Segment::Index(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prop::parse_path;

    #[test]
    fn params_namespace_from_prop_path() {
        let path = parse_path("params.speed").unwrap();
        assert_eq!(
            PropNamespace::from_prop_path(&path),
            Some(PropNamespace::Params)
        );
    }

    #[test]
    fn inputs_namespace_from_prop_path() {
        let path = parse_path("inputs[0]").unwrap();
        assert_eq!(
            PropNamespace::from_prop_path(&path),
            Some(PropNamespace::Inputs)
        );
    }

    #[test]
    fn outputs_namespace_from_prop_path() {
        let path = parse_path("outputs.color").unwrap();
        assert_eq!(
            PropNamespace::from_prop_path(&path),
            Some(PropNamespace::Outputs)
        );
    }

    #[test]
    fn state_namespace_from_prop_path() {
        let path = parse_path("state.value").unwrap();
        assert_eq!(
            PropNamespace::from_prop_path(&path),
            Some(PropNamespace::State)
        );
    }

    #[test]
    fn unknown_namespace_returns_none() {
        let path = parse_path("weird.field").unwrap();
        assert_eq!(PropNamespace::from_prop_path(&path), None);
    }

    #[test]
    fn index_head_returns_none() {
        let path = parse_path("[0]").unwrap();
        assert_eq!(PropNamespace::from_prop_path(&path), None);
    }

    #[test]
    fn empty_path_returns_none() {
        let path: PropPath = PropPath::default();
        assert_eq!(PropNamespace::from_prop_path(&path), None);
    }

    #[test]
    fn segment_name_returns_correct_strings() {
        assert_eq!(PropNamespace::Params.segment_name(), "params");
        assert_eq!(PropNamespace::Inputs.segment_name(), "inputs");
        assert_eq!(PropNamespace::Outputs.segment_name(), "outputs");
        assert_eq!(PropNamespace::State.segment_name(), "state");
    }
}
