use crate::{ActionConfirmation, ActionMeta, ActionPriority, UiError, UxNodeId, UxOp};

#[derive(Clone, Debug)]
pub struct UiAction {
    node_id: UxNodeId,
    op: Box<dyn UxOp>,
    meta: ActionMeta,
}

impl PartialEq for UiAction {
    fn eq(&self, other: &Self) -> bool {
        self.node_id == other.node_id && self.meta == other.meta && self.op.eq_op(other.op.as_ref())
    }
}

impl Eq for UiAction {}

impl UiAction {
    pub fn from_op(node_id: impl Into<UxNodeId>, op: impl UxOp) -> Self {
        let meta = op.default_action_meta();
        Self {
            node_id: node_id.into(),
            op: Box::new(op),
            meta,
        }
    }

    pub fn node_id(&self) -> &UxNodeId {
        &self.node_id
    }

    pub fn meta(&self) -> &ActionMeta {
        &self.meta
    }

    pub fn is_for_node(&self, node_id: &str) -> bool {
        self.node_id.as_str() == node_id
    }

    pub fn op_as<T>(&self) -> Option<&T>
    where
        T: UxOp,
    {
        self.op.as_any().downcast_ref::<T>()
    }

    pub fn into_op<T>(self) -> Result<T, UiError>
    where
        T: UxOp,
    {
        let node_id = self.node_id;
        self.op
            .into_any()
            .downcast::<T>()
            .map(|op| *op)
            .map_err(|_| {
                UiError::UnsupportedAction(format!(
                    "action for node {node_id} did not contain operation {}",
                    core::any::type_name::<T>()
                ))
            })
    }

    pub async fn execute(self, ctx: &mut impl crate::UxContext) -> crate::UxResult {
        ctx.dispatch(self).await
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.meta = self.meta.with_label(label);
        self
    }

    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.meta = self.meta.with_summary(summary);
        self
    }

    pub fn with_short_label(mut self, short_label: impl Into<String>) -> Self {
        self.meta = self.meta.with_short_label(short_label);
        self
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.meta = self.meta.with_icon(icon);
        self
    }

    pub fn with_priority(mut self, priority: ActionPriority) -> Self {
        self.meta.priority = priority;
        self
    }

    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.meta = self.meta.disabled(reason);
        self
    }

    pub fn with_confirmation(mut self, confirmation: ActionConfirmation) -> Self {
        self.meta = self.meta.with_confirmation(confirmation);
        self
    }

    pub fn with_meta(mut self, meta: ActionMeta) -> Self {
        self.meta = meta;
        self
    }
}

#[cfg(test)]
mod tests {
    use core::any::Any;

    use crate::{ActionMeta, ActionPriority, UiAction, UxNodeId, UxOp};

    #[test]
    fn cloned_action_clones_boxed_op() {
        let action = UiAction::from_op(UxNodeId::new("test.node"), TestOp::Run);

        let cloned = action.clone();

        assert!(matches!(cloned.op_as::<TestOp>(), Some(TestOp::Run)));
    }

    #[test]
    fn into_op_downcasts_matching_type() {
        let action = UiAction::from_op(UxNodeId::new("test.node"), TestOp::Run);

        let op = action.into_op::<TestOp>().unwrap();

        assert_eq!(op, TestOp::Run);
    }

    #[test]
    fn into_op_rejects_wrong_type() {
        let action = UiAction::from_op(UxNodeId::new("test.node"), TestOp::Run);

        assert!(action.into_op::<OtherOp>().is_err());
    }

    #[test]
    fn metadata_overrides_change_only_metadata() {
        let action = UiAction::from_op(UxNodeId::new("test.node"), TestOp::Run)
            .with_label("Go")
            .with_summary("Run it")
            .with_short_label("Go")
            .with_icon("play");

        assert_eq!(action.meta().label, "Go");
        assert_eq!(action.meta().summary, "Run it");
        assert_eq!(action.meta().short_label.as_deref(), Some("Go"));
        assert_eq!(action.meta().icon.as_deref(), Some("play"));
        assert!(matches!(action.op_as::<TestOp>(), Some(TestOp::Run)));
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum TestOp {
        Run,
    }

    impl UxOp for TestOp {
        fn default_action_meta(&self) -> ActionMeta {
            ActionMeta::new("Run", "Run the test operation.", ActionPriority::Primary)
        }

        fn clone_box(&self) -> Box<dyn UxOp> {
            Box::new(self.clone())
        }

        fn eq_op(&self, other: &dyn UxOp) -> bool {
            other.as_any().downcast_ref::<Self>() == Some(self)
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn into_any(self: Box<Self>) -> Box<dyn Any> {
            self
        }
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct OtherOp;

    impl UxOp for OtherOp {
        fn default_action_meta(&self) -> ActionMeta {
            ActionMeta::new("Other", "Run the other operation.", ActionPriority::Primary)
        }

        fn clone_box(&self) -> Box<dyn UxOp> {
            Box::new(self.clone())
        }

        fn eq_op(&self, other: &dyn UxOp) -> bool {
            other.as_any().downcast_ref::<Self>() == Some(self)
        }

        fn as_any(&self) -> &dyn Any {
            self
        }

        fn into_any(self: Box<Self>) -> Box<dyn Any> {
            self
        }
    }
}
