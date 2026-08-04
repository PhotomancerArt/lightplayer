//! Panel write/clear operations (panel.md P8): the `(scope, channel)`
//! command-channel path for panel controls.
//!
//! A runtime poke, not an edit: nothing is staged in the overlay, nothing
//! shows in the Save panel, the project stays clean, and the effect lands
//! on the engine's next frame — engaged state follows through ordinary
//! probe pulls (which is also what makes multiple clients converge,
//! panel.md P9).

use core::any::Any;

use crate::{
    ActionClass, ActionMeta, ActionPriority, ControllerOp, PROJECT_EDITOR_ACTION_DEADLINE,
};

/// Engage (or update) the panel writer for `(scope, channel)`.
///
/// Knob drags are a stream of these; the actor's batch planner COALESCES
/// per `(scope, channel)` — a newer write replaces a queued unsent one
/// (see `push_action_coalesced` in `studio_actor`), so a drag never
/// floods the wire.
#[derive(Clone, Debug, PartialEq)]
pub struct PanelWriteOp {
    /// The scope the write lands in (the control's home scope).
    pub scope: lpc_wire::WireScopeRef,
    /// The channel the control drives.
    pub channel: String,
    /// The value to hold.
    pub value: lpc_model::LpValue,
    /// Momentary renewal deadline (panel.md P14); `None` = latching.
    pub ttl_ms: Option<u32>,
}

impl ControllerOp for PanelWriteOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Set panel control",
            "Hold this value on the channel until reset.",
            ActionPriority::Primary,
        )
    }

    fn action_class(&self) -> ActionClass {
        ActionClass::Foreground {
            deadline: PROJECT_EDITOR_ACTION_DEADLINE,
        }
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

/// Clear engaged panel writers: one control, one scope, or everything
/// (settled P-Q4: clear-all reaches sink scopes too).
#[derive(Clone, Debug, PartialEq)]
pub struct PanelClearOp {
    pub request: lpc_wire::WirePanelClearRequest,
}

impl ControllerOp for PanelClearOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Reset panel control",
            "Release the held value; authored wiring returns.",
            ActionPriority::Primary,
        )
    }

    fn action_class(&self) -> ActionClass {
        ActionClass::Foreground {
            deadline: PROJECT_EDITOR_ACTION_DEADLINE,
        }
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

/// Flip panel-state auto-save for the whole project (panel.md P11).
///
/// Project-level, not scoped: panel state persists per project folder, so
/// the switch has no `(scope, channel)` identity — which is also why the
/// root module is the only face that presents it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PanelAutoSaveOp {
    /// Whether panel state keeps being written.
    pub enabled: bool,
}

impl ControllerOp for PanelAutoSaveOp {
    fn default_action_meta(&self) -> ActionMeta {
        ActionMeta::new(
            "Panel auto-save",
            "Keep this project's held panel values across restarts.",
            ActionPriority::Primary,
        )
    }

    fn action_class(&self) -> ActionClass {
        ActionClass::Foreground {
            deadline: PROJECT_EDITOR_ACTION_DEADLINE,
        }
    }

    fn clone_box(&self) -> Box<dyn ControllerOp> {
        Box::new(*self)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_auto_save_op_is_editor_foreground_class() {
        let op = PanelAutoSaveOp { enabled: false };
        assert_eq!(
            op.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        assert_eq!(op.default_action_meta().label, "Panel auto-save");
    }

    #[test]
    fn panel_ops_are_editor_foreground_class() {
        let write = PanelWriteOp {
            scope: lpc_wire::WireScopeRef::Module {
                owner: lpc_model::NodeId::new(0),
            },
            channel: "speed".to_string(),
            value: lpc_model::LpValue::F32(0.5),
            ttl_ms: None,
        };
        assert_eq!(
            write.action_class(),
            ActionClass::Foreground {
                deadline: PROJECT_EDITOR_ACTION_DEADLINE,
            }
        );
        let clear = PanelClearOp {
            request: lpc_wire::WirePanelClearRequest::All,
        };
        assert_eq!(clear.default_action_meta().label, "Reset panel control");
    }
}
