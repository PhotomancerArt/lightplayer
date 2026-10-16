//! [`UiBluetoothSwitch`]: the board's Bluetooth switch, as the connection
//! bar's details draw it (moved here from the web's Connections group, Q39).
//!
//! Bluetooth is on by default, so most people never touch the switch. The
//! board reads it once, at boot: flipped over USB, Studio restarts the board
//! to apply it, and the switch says so until the board is back. Over
//! Bluetooth the switch is locked — you cannot turn off the radio you are
//! talking over — and it says how instead. The toggle itself still sends
//! the access command from the web.

use crate::app::access::UiDeviceAccess;

/// What the Bluetooth switch shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiBluetoothSwitch {
    pub on: bool,
    /// The switch cannot be flipped now.
    pub locked: bool,
    /// Under the name: why it is locked.
    pub sub: Option<String>,
    /// Under the row: the restart that applies a change.
    pub restart_note: Option<String>,
}

/// The switch for a board with these access facts.
pub fn bluetooth_switch(access: &UiDeviceAccess) -> UiBluetoothSwitch {
    if access.over_bluetooth {
        return UiBluetoothSwitch {
            on: true,
            locked: true,
            sub: Some("connected this way — turn off by USB".to_string()),
            restart_note: None,
        };
    }
    let Some(panel) = access.panel.as_ref() else {
        return UiBluetoothSwitch {
            on: false,
            locked: true,
            sub: None,
            restart_note: None,
        };
    };
    let on = panel.ble_enabled.unwrap_or(false);
    let word = if on { "on" } else { "off" };
    let restart_note = panel.restart_pending.then(|| match panel.can_restart {
        true => format!("Restarting to turn Bluetooth {word}…"),
        false => format!("Bluetooth turns {word} when the device restarts."),
    });
    UiBluetoothSwitch {
        on,
        locked: panel.ble_enabled.is_none() || panel.writing || panel.restart_pending,
        sub: None,
        restart_note,
    }
}

#[cfg(test)]
mod tests {
    use lpa_devices::DeviceId;

    use super::*;
    use crate::app::access::UiAccessPanel;

    #[test]
    fn over_bluetooth_the_switch_is_locked_on_and_says_how() {
        let switch = bluetooth_switch(&UiDeviceAccess {
            over_bluetooth: true,
            ..UiDeviceAccess::default()
        });
        assert!(switch.on && switch.locked);
        assert_eq!(
            switch.sub.as_deref(),
            Some("connected this way — turn off by USB")
        );
    }

    #[test]
    fn a_switch_over_usb_says_the_restart_until_the_device_is_back() {
        let mut access = usb(Some(true));
        assert_eq!(bluetooth_switch(&access).restart_note, None);
        assert!(!bluetooth_switch(&access).locked);
        access.panel.as_mut().unwrap().restart_pending = true;
        let switch = bluetooth_switch(&access);
        assert_eq!(
            switch.restart_note.as_deref(),
            Some("Restarting to turn Bluetooth on…")
        );
        assert!(switch.locked, "no second flip while the first applies");
    }

    #[test]
    fn before_the_list_arrives_the_switch_waits() {
        assert!(bluetooth_switch(&usb(None)).locked);
    }

    fn usb(ble_enabled: Option<bool>) -> UiDeviceAccess {
        UiDeviceAccess {
            panel: Some(UiAccessPanel {
                ble_enabled,
                ..UiAccessPanel::reading(DeviceId(1))
            }),
            ..UiDeviceAccess::default()
        }
    }
}
