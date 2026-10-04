//! The Wi‑Fi verbs, as offers under `devices/<board>/wifi/…`.
//!
//! | path | params | level | binds to |
//! |---|---|---|---|
//! | `wifi/set` | `network` text (≤ 32), `password` **secret** text | Routine | a set: same name + blank password keeps it; a new name + blank is an open network |
//! | `wifi/enabled` | `enabled` toggle | Routine | switch the saved network on or off — only when one is saved |
//! | `wifi/cloud-relay` | `enabled` toggle | Routine | let lightplayer.app reach the board through the cloud (on by default) |
//! | `wifi/forget` | — | **Lasting** | the board forgets the network and its password — only when one is saved |
//!
//! Published only while the link holds edit and the board has answered its
//! status ([`UiDeviceWifi::can_edit`], [`UiDeviceWifi::status`]), on every
//! LightPlayer board whatever its station says (plan Q6). Drawn disabled,
//! never hidden, while a read or a change is in flight. The board's
//! validation is the authority; the binder mirrors it for early words, and
//! the board's own sentence is shown when it refuses.

use lpa_devices::DeviceId;
use lpc_access::{validate_password, validate_ssid};

use super::network_op::{NetworkChange, NetworkOp};
use super::ui_device_wifi::UiDeviceWifi;
use super::wifi_password_change::PasswordChange;
use crate::{
    ActionConfirmation, OfferArgError, OfferArgs, OfferBinder, OfferParam, OfferPath, UiAction,
    UiOffer,
};

/// The set offer's network-name parameter.
pub const WIFI_NETWORK_PARAM: &str = "network";
/// The set offer's password parameter (a secret).
pub const WIFI_PASSWORD_PARAM: &str = "password";
/// The two toggle offers' parameter.
pub const WIFI_ENABLED_PARAM: &str = "enabled";

/// What the cloud-relay switch does: its offer's summary, drawn under it.
pub const WIFI_CLOUD_RELAY_SUMMARY: &str =
    "Lets lightplayer.app reach this board through the cloud.";

/// The Wi‑Fi namespace segment under a board's prefix.
pub const WIFI_SEGMENT: &str = "wifi";

/// Why the Wi‑Fi verbs wait while the board is answering.
pub const WIFI_BUSY: &str = "Studio is talking to the board — a moment";

/// Every Wi‑Fi offer `wifi` makes, under the board prefix `prefix`.
pub fn wifi_offers(prefix: &OfferPath, wifi: &UiDeviceWifi) -> Vec<UiOffer> {
    let Some(status) = wifi.status.as_ref().filter(|_| wifi.can_edit) else {
        return Vec::new();
    };
    let at = |verb: &str| prefix.clone().child(WIFI_SEGMENT).child(verb);
    let busy = wifi.reading || wifi.writing;
    let device = wifi.device;
    let saved = status.wifi.as_ref();
    let mut offers = vec![set_offer(
        at("set"),
        device,
        saved.map(|wifi| wifi.ssid.clone()),
        busy,
    )];
    if let Some(saved) = saved {
        offers.push(toggle_offer(
            at("enabled"),
            device,
            "join this network",
            None,
            saved.enabled,
            busy,
            |enabled| NetworkChange::Set {
                ssid: None,
                password: PasswordChange::Keep,
                enabled: Some(enabled),
                cloud_relay: None,
            },
        ));
    }
    offers.push(toggle_offer(
        at("cloud-relay"),
        device,
        "cloud relay",
        Some(WIFI_CLOUD_RELAY_SUMMARY),
        status.cloud_relay,
        busy,
        |cloud_relay| NetworkChange::Set {
            ssid: None,
            password: PasswordChange::Keep,
            enabled: None,
            cloud_relay: Some(cloud_relay),
        },
    ));
    if let Some(saved) = saved {
        let forget =
            NetworkOp::action_for(device, NetworkChange::Forget).lasting(ActionConfirmation::new(
                format!("Forget {}?", saved.ssid),
                format!(
                    "The board forgets {} and its password. You'll need the password to set \
                     it again.",
                    saved.ssid
                ),
                "Forget",
            ));
        offers.push(UiOffer::new(
            at("forget"),
            "remove",
            disabled_if(forget, busy),
        ));
    }
    offers
}

/// `wifi/set`: the network's name and its password.
///
/// With a network saved, the name may be left blank (it means the saved
/// one) and so may the password (it stays as it is). A new name with a
/// blank password is an open network — the field's placeholder says so.
fn set_offer(path: OfferPath, device: DeviceId, saved: Option<String>, busy: bool) -> UiOffer {
    let network = OfferParam::text(
        WIFI_NETWORK_PARAM,
        "network name",
        saved.clone().unwrap_or_else(|| "network name".to_string()),
    )
    .max_len(32);
    let network = match saved {
        Some(_) => network.optional(),
        None => network,
    };
    let password = OfferParam::text(
        WIFI_PASSWORD_PARAM,
        "password",
        match saved {
            Some(_) => "unchanged",
            None => "none — open network",
        },
    )
    .optional()
    .secret();
    let unbound = NetworkOp::action_for(
        device,
        NetworkChange::Set {
            ssid: None,
            password: PasswordChange::Keep,
            enabled: None,
            cloud_relay: None,
        },
    );
    UiOffer::with_params(
        path,
        "wifi",
        vec![network, password],
        OfferBinder::new(move |args: &OfferArgs| {
            let action = bind_set(device, saved.as_deref(), args)?;
            Ok(disabled_if(action, busy))
        }),
        disabled_if(unbound, busy),
    )
}

/// A set from the form's values; see [`set_offer`].
fn bind_set(
    device: DeviceId,
    saved: Option<&str>,
    args: &OfferArgs,
) -> Result<UiAction, OfferArgError> {
    let ssid = match args.text(WIFI_NETWORK_PARAM).or(saved) {
        Some(ssid) => ssid.to_string(),
        None => {
            return Err(OfferArgError::Missing {
                name: WIFI_NETWORK_PARAM.to_string(),
                label: "network name".to_string(),
            });
        }
    };
    validate_ssid(&ssid).map_err(|rule| OfferArgError::Invalid {
        name: WIFI_NETWORK_PARAM.to_string(),
        reason: rule.to_string(),
    })?;
    // A password is taken as typed (spaces count), never trimmed.
    let typed = args
        .get(WIFI_PASSWORD_PARAM)
        .filter(|password| !password.is_empty());
    let same_network = saved == Some(ssid.as_str());
    let password = match (typed, same_network) {
        (Some(password), _) => {
            // The rule's words name the length, never the text.
            validate_password(password).map_err(|rule| OfferArgError::Invalid {
                name: WIFI_PASSWORD_PARAM.to_string(),
                reason: rule.to_string(),
            })?;
            PasswordChange::Set(password.to_string())
        }
        (None, true) => PasswordChange::Keep,
        (None, false) => PasswordChange::Open,
    };
    Ok(NetworkOp::action_for(
        device,
        NetworkChange::Set {
            ssid: Some(ssid),
            password,
            enabled: None,
            cloud_relay: None,
        },
    ))
}

/// A one-switch offer: `enabled`, currently `value`, with `summary` as
/// its help line when it has one.
fn toggle_offer(
    path: OfferPath,
    device: DeviceId,
    label: &str,
    summary: Option<&'static str>,
    value: bool,
    busy: bool,
    change: fn(bool) -> NetworkChange,
) -> UiOffer {
    let set = move |on: bool| {
        let action = NetworkOp::action_for(device, change(on));
        let action = match summary {
            Some(summary) => action.with_summary(summary),
            None => action,
        };
        disabled_if(action, busy)
    };
    UiOffer::with_params(
        path,
        "wifi",
        vec![OfferParam::toggle(WIFI_ENABLED_PARAM, label, value)],
        OfferBinder::new(move |args: &OfferArgs| {
            Ok(set(args.toggle(WIFI_ENABLED_PARAM).unwrap_or(value)))
        }),
        set(value),
    )
}

fn disabled_if(action: UiAction, busy: bool) -> UiAction {
    if busy {
        action.disabled(WIFI_BUSY)
    } else {
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_wire::server::{NetworkStatus, StationState, WifiInfo};

    const PASSWORD: &str = "correct-horse-42";

    #[test]
    fn a_board_with_no_network_offers_set_and_cloud_relay() {
        let offers = wifi_offers(&prefix(), &wifi(None));
        assert_eq!(verbs(&offers), ["set", "cloud-relay"]);
        let set = &offers[0];
        assert!(set.takes_a_secret());
        assert!(!set.is_enabled(), "a new network needs its name");
        assert!(set.consequence().is_routine());

        // A name and a password: Set, which the binder saw.
        let pressed = set
            .press(
                &OfferArgs::new()
                    .with(WIFI_NETWORK_PARAM, "lp-walk-net")
                    .with(WIFI_PASSWORD_PARAM, PASSWORD),
            )
            .unwrap();
        assert_eq!(
            op(&pressed).change,
            NetworkChange::Set {
                ssid: Some("lp-walk-net".to_string()),
                password: PasswordChange::Set(PASSWORD.to_string()),
                enabled: None,
                cloud_relay: None,
            }
        );
        // Nothing the agent or a log reads holds it.
        assert!(!format!("{pressed:?}").contains(PASSWORD));
        assert_eq!(
            pressed.offer_press().unwrap().args.get(WIFI_PASSWORD_PARAM),
            Some(crate::SECRET_MARKER)
        );

        // A new name with no password: an open network.
        let open = set
            .press(&OfferArgs::new().with(WIFI_NETWORK_PARAM, "cafe"))
            .unwrap();
        assert!(matches!(
            op(&open).change,
            NetworkChange::Set {
                password: PasswordChange::Open,
                ..
            }
        ));
    }

    #[test]
    fn a_saved_network_keeps_its_password_unless_one_is_typed() {
        let offers = wifi_offers(&prefix(), &wifi(Some("lp-walk-net")));
        assert_eq!(verbs(&offers), ["set", "enabled", "cloud-relay", "forget"]);
        let set = &offers[0];
        assert!(set.is_enabled(), "the saved name stands in for a blank one");
        let keep = set.press(&OfferArgs::new()).unwrap();
        assert_eq!(
            op(&keep).change,
            NetworkChange::Set {
                ssid: Some("lp-walk-net".to_string()),
                password: PasswordChange::Keep,
                enabled: None,
                cloud_relay: None,
            }
        );
        // A different name never takes the old password with it.
        let renamed = set
            .press(&OfferArgs::new().with(WIFI_NETWORK_PARAM, "other"))
            .unwrap();
        assert!(matches!(
            op(&renamed).change,
            NetworkChange::Set {
                password: PasswordChange::Open,
                ..
            }
        ));
    }

    #[test]
    fn the_binder_mirrors_the_boards_rules_without_quoting_the_password() {
        let set = wifi_offers(&prefix(), &wifi(None)).remove(0);
        let short = set
            .press(
                &OfferArgs::new()
                    .with(WIFI_NETWORK_PARAM, "lp-walk-net")
                    .with(WIFI_PASSWORD_PARAM, "seven77"),
            )
            .unwrap_err()
            .to_string();
        assert!(short.contains("7 characters"), "{short}");
        assert!(!short.contains("seven77"), "{short}");
        let long = set
            .press(&OfferArgs::new().with(WIFI_NETWORK_PARAM, "x".repeat(33)))
            .unwrap_err()
            .to_string();
        assert!(long.contains("32"), "{long}");
        // Multibyte: 11 characters, 33 bytes — the board counts bytes.
        let wide = set
            .press(&OfferArgs::new().with(WIFI_NETWORK_PARAM, "ネットワークの名前です"))
            .unwrap_err()
            .to_string();
        assert!(wide.contains("bytes"), "{wide}");
    }

    #[test]
    fn forget_is_lasting_and_names_the_network() {
        let offers = wifi_offers(&prefix(), &wifi(Some("lp-walk-net")));
        let forget = offers.last().unwrap();
        assert!(forget.consequence().arms());
        let copy = forget.consequence().copy().unwrap();
        assert_eq!(copy.title, "Forget lp-walk-net?");
        assert!(copy.message.contains("You'll need the password"));
    }

    #[test]
    fn the_toggles_bind_their_switch_and_nothing_else() {
        let offers = wifi_offers(&prefix(), &wifi(Some("lp-walk-net")));
        let enabled = &offers[1];
        let off = enabled
            .press(&OfferArgs::new().with(WIFI_ENABLED_PARAM, "false"))
            .unwrap();
        assert_eq!(
            op(&off).change,
            NetworkChange::Set {
                ssid: None,
                password: PasswordChange::Keep,
                enabled: Some(false),
                cloud_relay: None,
            }
        );
        let relay = &offers[2];
        assert_eq!(relay.params()[0].label, "cloud relay");
        assert_eq!(relay.summary(), WIFI_CLOUD_RELAY_SUMMARY);
        let relay_off = relay
            .press(&OfferArgs::new().with(WIFI_ENABLED_PARAM, "false"))
            .unwrap();
        assert!(matches!(
            op(&relay_off).change,
            NetworkChange::Set {
                cloud_relay: Some(false),
                enabled: None,
                ..
            }
        ));
    }

    #[test]
    fn busy_draws_every_verb_disabled_and_no_author_offers_nothing() {
        let mut busy = wifi(Some("lp-walk-net"));
        busy.writing = true;
        let offers = wifi_offers(&prefix(), &busy);
        assert_eq!(offers.len(), 4, "drawn, not hidden");
        assert!(offers.iter().all(|offer| !offer.is_enabled()));

        let mut play = wifi(Some("lp-walk-net"));
        play.can_edit = false;
        assert!(wifi_offers(&prefix(), &play).is_empty());
        let mut unread = wifi(None);
        unread.status = None;
        assert!(wifi_offers(&prefix(), &unread).is_empty());
    }

    fn op(action: &UiAction) -> &NetworkOp {
        action.op_as::<NetworkOp>().unwrap()
    }

    fn verbs(offers: &[UiOffer]) -> Vec<String> {
        offers
            .iter()
            .map(|offer| {
                let path = offer.path.to_string();
                let prefix = "devices/mac-a0f26287b48c/wifi/";
                assert!(path.starts_with(prefix), "{path}");
                path[prefix.len()..].to_string()
            })
            .collect()
    }

    fn prefix() -> OfferPath {
        OfferPath::board(&crate::BoardRef::Mac(
            lpa_devices::BoardKey::parse("a0:f2:62:87:b4:8c").unwrap(),
        ))
    }

    fn wifi(saved: Option<&str>) -> UiDeviceWifi {
        UiDeviceWifi {
            device: DeviceId(7),
            can_edit: true,
            status: Some(NetworkStatus {
                wifi: saved.map(|ssid| WifiInfo {
                    ssid: ssid.to_string(),
                    has_password: true,
                    enabled: true,
                }),
                cloud_relay: true,
                station: StationState::Unsupported,
            }),
            reading: false,
            writing: false,
            error: None,
        }
    }
}
