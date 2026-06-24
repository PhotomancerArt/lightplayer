use crate::{ActionConfirmation, ActionMeta, ActionPriority, ControllerId, ControllerOp, UiError};

/// A user-invokable controller operation with render metadata.
///
/// `UiAction` is the bridge between controller state and UI controls. The
/// operation remains typed behind `ControllerOp`, while `ActionMeta` carries the
/// label, summary, icon, priority, enablement, and confirmation data that a
/// component needs to render the button.
#[derive(Clone, Debug)]
pub struct UiAction {
    node_id: ControllerId,
    op: Box<dyn ControllerOp>,
    meta: ActionMeta,
}

impl PartialEq for UiAction {
    fn eq(&self, other: &Self) -> bool {
        self.node_id == other.node_id && self.meta == other.meta && self.op.eq_op(other.op.as_ref())
    }
}

impl Eq for UiAction {}

impl UiAction {
    /// Create an action from a controller id and operation.
    ///
    /// The action metadata starts with `ControllerOp::default_action_meta` and
    /// can be refined with the builder-style methods below.
    pub fn from_op(node_id: impl Into<ControllerId>, op: impl ControllerOp) -> Self {
        let meta = op.default_action_meta();
        Self {
            node_id: node_id.into(),
            op: Box::new(op),
            meta,
        }
    }

    /// Return the controller id this action targets.
    pub fn node_id(&self) -> &ControllerId {
        &self.node_id
    }

    /// Return the render metadata for this action.
    pub fn meta(&self) -> &ActionMeta {
        &self.meta
    }

    /// Return whether this action targets the given controller id string.
    pub fn is_for_node(&self, node_id: &str) -> bool {
        self.node_id.as_str() == node_id
    }

    /// Borrow the operation as a concrete operation type.
    pub fn op_as<T>(&self) -> Option<&T>
    where
        T: ControllerOp,
    {
        self.op.as_any().downcast_ref::<T>()
    }

    /// Consume the action and recover a concrete operation type.
    ///
    /// Controllers use this when dispatch has already routed the action to the
    /// expected controller. A type mismatch means the action was routed
    /// incorrectly or constructed with the wrong operation type.
    pub fn into_op<T>(self) -> Result<T, UiError>
    where
        T: ControllerOp,
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

    /// Dispatch this action through a controller context.
    pub async fn execute(self, ctx: &mut impl crate::ControllerContext) -> crate::UiResult {
        ctx.dispatch(self).await
    }

    /// Override the visible action label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.meta = self.meta.with_label(label);
        self
    }

    /// Override the action summary, usually used as tooltip/help text.
    pub fn with_summary(mut self, summary: impl Into<String>) -> Self {
        self.meta = self.meta.with_summary(summary);
        self
    }

    /// Add a shorter label for compact layouts.
    pub fn with_short_label(mut self, short_label: impl Into<String>) -> Self {
        self.meta = self.meta.with_short_label(short_label);
        self
    }

    /// Attach an icon token understood by the renderer.
    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.meta = self.meta.with_icon(icon);
        self
    }

    /// Override the visual/action priority.
    pub fn with_priority(mut self, priority: ActionPriority) -> Self {
        self.meta.priority = priority;
        self
    }

    /// Disable the action with a user-facing reason.
    pub fn disabled(mut self, reason: impl Into<String>) -> Self {
        self.meta = self.meta.disabled(reason);
        self
    }

    /// Require confirmation before the action is dispatched.
    pub fn with_confirmation(mut self, confirmation: ActionConfirmation) -> Self {
        self.meta = self.meta.with_confirmation(confirmation);
        self
    }

    /// Replace all render metadata for the action.
    pub fn with_meta(mut self, meta: ActionMeta) -> Self {
        self.meta = meta;
        self
    }
}

#[cfg(test)]
mod tests {
    use core::any::Any;

    use crate::{ActionMeta, ActionPriority, ControllerId, ControllerOp, UiAction};

    #[test]
    fn cloned_action_clones_boxed_op() {
        let action = UiAction::from_op(ControllerId::new("test.node"), TestOp::Run);

        let cloned = action.clone();

        assert!(matches!(cloned.op_as::<TestOp>(), Some(TestOp::Run)));
    }

    #[test]
    fn into_op_downcasts_matching_type() {
        let action = UiAction::from_op(ControllerId::new("test.node"), TestOp::Run);

        let op = action.into_op::<TestOp>().unwrap();

        assert_eq!(op, TestOp::Run);
    }

    #[test]
    fn into_op_rejects_wrong_type() {
        let action = UiAction::from_op(ControllerId::new("test.node"), TestOp::Run);

        assert!(action.into_op::<OtherOp>().is_err());
    }

    #[test]
    fn metadata_overrides_change_only_metadata() {
        let action = UiAction::from_op(ControllerId::new("test.node"), TestOp::Run)
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

    impl ControllerOp for TestOp {
        fn default_action_meta(&self) -> ActionMeta {
            ActionMeta::new("Run", "Run the test operation.", ActionPriority::Primary)
        }

        fn clone_box(&self) -> Box<dyn ControllerOp> {
            Box::new(self.clone())
        }

        fn eq_op(&self, other: &dyn ControllerOp) -> bool {
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

    impl ControllerOp for OtherOp {
        fn default_action_meta(&self) -> ActionMeta {
            ActionMeta::new("Other", "Run the other operation.", ActionPriority::Primary)
        }

        fn clone_box(&self) -> Box<dyn ControllerOp> {
            Box::new(self.clone())
        }

        fn eq_op(&self, other: &dyn ControllerOp) -> bool {
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
