//! [`BluetoothSwitchOp`]: turn a board's Bluetooth on or off, because
//! someone asked — the offer at `devices/<board>/bluetooth/enabled`
//! ([`super::bluetooth_offer`]).
//!
//! Built only by that offer's binder; the web never builds one (it used to
//! send the access command from the Bluetooth switch itself). The studio
//! controller runs it by handing the access controller the change it stands
//! for, which refuses "off" over Bluetooth and, over USB, restarts the
//! board to apply it (the board reads the switch at boot).

use core::any::Any;

use lpa_devices::DeviceId;

use super::access_command::AccessCommand;
use crate::DeviceAccessChange;
use crate::{ActionClass, ActionMeta, ActionPriority, ControllerId, ControllerOp, UiAction};

/// Set `device`'s Bluetooth switch to `on`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BluetoothSwitchOp {
    pub device: DeviceId,
    pub on: bool,
}

impl BluetoothSwitchOp {
    /// Routed by `StudioController` to its access controller.
    pub const NODE_ID: &'static str = "studio|bluetooth";

    /// This change as a dispatchable [`UiAction`].
    pub fn action_for(op: Self) -> UiAction {
        UiAction::from_op(ControllerId::new(Self::NODE_ID), op)
    }

    /// The access command this op stands for.
    pub(crate) fn into_access_command(self) -> AccessCommand {
        AccessCommand::Change {
            device: self.device,
            change: DeviceAccessChange::SetBluetooth(self.on),
        }
    }
}

impl ControllerOp for BluetoothSwitchOp {
    fn default_action_meta(&self) -> ActionMeta {
        let meta = match self.on {
            true => ActionMeta::new(
                ENABLE_BLUETOOTH,
                "Turn the board's Bluetooth on. It restarts to apply it.",
                ActionPriority::Secondary,
            ),
            // A phone on its Bluetooth loses the board until it is on again.
            false => ActionMeta::new(
                DISABLE_BLUETOOTH,
                "Turn the board's Bluetooth off: nothing nearby can connect by it. It restarts to apply it.",
                ActionPriority::Secondary,
            )
            .undoable(),
        };
        meta.with_icon("bluetooth")
    }

    fn action_class(&self) -> ActionClass {
        ActionClass::Foreground {
            deadline: crate::PROJECT_ACTION_DEADLINE,
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

/// The verb's words, off.
pub const DISABLE_BLUETOOTH: &str = "Disable Bluetooth";
/// …and on.
pub const ENABLE_BLUETOOTH: &str = "Enable Bluetooth";
