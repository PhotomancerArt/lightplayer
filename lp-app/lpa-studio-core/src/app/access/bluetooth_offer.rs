//! `devices/<board>/bluetooth/enabled`: the board's Bluetooth switch as a
//! core offer, pressed from the card's Bluetooth row ("Disable Bluetooth",
//! "Enable Bluetooth") and by the app agent the same way.
//!
//! Offered where the board's access list says its switch: over USB, or on
//! a network or Bluetooth link unlocked at edit (the access panel's own
//! rule). One Toggle parameter, `enabled`, at the switch's other position:
//! a press with no value flips it, and the offer reads "Disable Bluetooth"
//! while it is on. Off over Bluetooth is refused — it is the
//! link you are on — and a change that is still applying (the board reads
//! the switch at boot, and Studio restarts it over USB) waits with the
//! restart's words. Off is Undoable (a phone on its Bluetooth loses the
//! board until it is on again); on is Routine.

use lpa_devices::DeviceId;

use super::bluetooth_switch_op::BluetoothSwitchOp;
use super::ui_access_view::UiDeviceAccess;
use crate::{OfferArgs, OfferBinder, OfferParam, OfferPath, UiAction, UiOffer};

/// The verb's path under `devices/<board>`: `bluetooth/enabled`.
pub const BLUETOOTH_SEGMENT: &str = "bluetooth";
/// …its leaf.
pub const BLUETOOTH_ENABLED_VERB: &str = "enabled";
/// The one parameter: the switch's position.
pub const BLUETOOTH_ENABLED_PARAM: &str = "enabled";

/// Why off waits over Bluetooth.
pub const OFF_OVER_BLUETOOTH: &str =
    "Bluetooth can only be turned off by USB \u{2014} it is the link you are on.";

/// The offer for `device` under `prefix`, from its access facts; `None`
/// where its list does not say the switch. `over_bluetooth` is the link's
/// own kind, read off the endpoint.
pub fn bluetooth_offer(
    prefix: &OfferPath,
    device: DeviceId,
    access: &UiDeviceAccess,
    over_bluetooth: bool,
) -> Option<UiOffer> {
    let panel = access.panel.as_ref()?;
    let value = panel.ble_enabled?;
    let applying = (panel.writing || panel.restart_pending)
        .then(|| bluetooth_restart_words(value, panel.can_restart));
    let set = move |on: bool| -> UiAction {
        let action = BluetoothSwitchOp::action_for(BluetoothSwitchOp { device, on });
        match (&applying, on, over_bluetooth) {
            (Some(words), _, _) => action.disabled(words.clone()),
            (None, false, true) => action.disabled(OFF_OVER_BLUETOOTH),
            (None, _, _) => action,
        }
    };
    let unbound = set(!value);
    // (`with_params` binds the unbound action again, from the default.)
    Some(UiOffer::with_params(
        bluetooth_path(prefix),
        "bluetooth",
        // The value a press sets, the switch's other position: the offer
        // reads "Disable Bluetooth" while it is on.
        vec![OfferParam::toggle(
            BLUETOOTH_ENABLED_PARAM,
            "Bluetooth",
            !value,
        )],
        OfferBinder::new(move |args: &OfferArgs| {
            Ok(set(args.toggle(BLUETOOTH_ENABLED_PARAM).unwrap_or(!value)))
        }),
        unbound,
    ))
}

/// `devices/<board>/bluetooth/enabled`.
pub fn bluetooth_path(prefix: &OfferPath) -> OfferPath {
    prefix
        .clone()
        .child(BLUETOOTH_SEGMENT)
        .child(BLUETOOTH_ENABLED_VERB)
}

/// What a change to the switch says while it applies — `on` is the value
/// the board's list now holds: Studio restarts the board to turn it so
/// (over USB), or it turns at the board's next restart.
pub fn bluetooth_restart_words(on: bool, can_restart: bool) -> String {
    let word = if on { "on" } else { "off" };
    match can_restart {
        true => format!("Restarting to turn Bluetooth {word}\u{2026}"),
        false => format!("Bluetooth turns {word} when the device restarts."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::access::UiAccessPanel;
    use crate::{ActionConsequence, OfferArgError};

    #[test]
    fn it_is_offered_where_the_list_says_the_switch() {
        let prefix = OfferPath::devices().child("mac-a0f26287b48c");
        assert!(bluetooth_offer(&prefix, DeviceId(7), &access(None), false).is_none());
        let offer = bluetooth_offer(&prefix, DeviceId(7), &access(Some(true)), false)
            .expect("its list says the switch");
        assert_eq!(
            offer.path.to_string(),
            "devices/mac-a0f26287b48c/bluetooth/enabled"
        );
        assert_eq!(offer.label(), "Disable Bluetooth", "a press flips it");
    }

    /// Off is Undoable, on Routine; off over Bluetooth is refused.
    #[test]
    fn off_is_undoable_and_refused_over_bluetooth() {
        let prefix = OfferPath::devices().child("mac-a0f26287b48c");
        let off = OfferArgs::new().with(BLUETOOTH_ENABLED_PARAM, "false");
        let on = OfferArgs::new().with(BLUETOOTH_ENABLED_PARAM, "true");
        let usb = bluetooth_offer(&prefix, DeviceId(7), &access(Some(true)), false).unwrap();
        let pressed = usb.press(&off).expect("off over USB");
        assert_eq!(pressed.meta().consequence, ActionConsequence::Undoable);
        let usb_off = bluetooth_offer(&prefix, DeviceId(7), &access(Some(false)), false).unwrap();
        let pressed = usb_off.press(&on).expect("on over USB");
        assert!(pressed.meta().consequence.is_routine());
        let ble = bluetooth_offer(&prefix, DeviceId(7), &access(Some(true)), true).unwrap();
        assert_eq!(
            ble.press(&off),
            Err(OfferArgError::Unavailable {
                reason: OFF_OVER_BLUETOOTH.to_string()
            })
        );
    }

    /// A change still applying waits with the restart's words.
    #[test]
    fn a_change_applying_waits_with_the_restarts_words() {
        let prefix = OfferPath::devices().child("mac-a0f26287b48c");
        let mut access = access(Some(true));
        access.panel.as_mut().unwrap().restart_pending = true;
        let offer = bluetooth_offer(&prefix, DeviceId(7), &access, false).unwrap();
        assert_eq!(
            offer.press(&OfferArgs::new()),
            Err(OfferArgError::Unavailable {
                reason: "Restarting to turn Bluetooth on\u{2026}".to_string()
            })
        );
    }

    fn access(ble_enabled: Option<bool>) -> UiDeviceAccess {
        UiDeviceAccess {
            panel: Some(UiAccessPanel {
                ble_enabled,
                can_restart: true,
                ..UiAccessPanel::reading(DeviceId(7))
            }),
            ..UiDeviceAccess::default()
        }
    }
}
