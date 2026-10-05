//! The Wi‑Fi verbs, as offers under `devices/<board>/wifi/…`.
//!
//! | path | params | level | binds to |
//! |---|---|---|---|
//! | `wifi/add` | `network` text (≤ 32 bytes), `password` **secret** text, `hidden` toggle | Routine | save a network (a saved name takes the new password: Change password); blank password = an open network |
//! | `wifi/forget/<slug>` | — | **Lasting** | the board forgets that network and its password — one per saved network ([`super::wifi_network_slug`]) |
//! | `wifi/enabled` | `enabled` toggle | Routine | the board's one Wi‑Fi switch |
//! | `wifi/cloud-relay` | `enabled` toggle | Routine | let lightplayer.app reach the board through the cloud (on by default) |
//!
//! Published only while the link holds edit and the board has answered its
//! status ([`UiDeviceWifi::can_edit`], [`UiDeviceWifi::status`]), on every
//! LightPlayer board whatever its station says (plan Q6). Drawn disabled,
//! never hidden, while a read or a change is in flight. The board's
//! validation is the authority; the binder mirrors it for early words, and
//! the board's own sentence is shown when it refuses. The add verb reads
//! **Connect** on a board that can connect and **Save** on one that cannot
//! (every M5 image).

use lpa_devices::DeviceId;
use lpc_access::{NetworkFile, validate_password, validate_ssid};

use super::network_op::{NetworkChange, NetworkOp};
use super::ui_device_wifi::UiDeviceWifi;
use super::wifi_network_slug::wifi_network_slugs;
use super::wifi_password_change::PasswordChange;
use super::wifi_words::{CLOUD_RELAY_HELP, FORGET_HELP};
use crate::{
    ActionConfirmation, OfferArgError, OfferArgs, OfferBinder, OfferParam, OfferPath, UiAction,
    UiOffer,
};

/// The add offer's network-name parameter.
pub const WIFI_NETWORK_PARAM: &str = "network";
/// The add offer's password parameter (a secret).
pub const WIFI_PASSWORD_PARAM: &str = "password";
/// The add offer's hidden-network parameter.
pub const WIFI_HIDDEN_PARAM: &str = "hidden";
/// The two switch offers' parameter.
pub const WIFI_ENABLED_PARAM: &str = "enabled";

/// What the cloud-relay switch does: its offer's summary, drawn under it.
pub const WIFI_CLOUD_RELAY_SUMMARY: &str = CLOUD_RELAY_HELP;

/// The Wi‑Fi namespace segment under a board's prefix.
pub const WIFI_SEGMENT: &str = "wifi";

/// The forget verbs' namespace under it: `wifi/forget/<slug>`.
pub const WIFI_FORGET_SEGMENT: &str = "forget";

/// Why the Wi‑Fi verbs wait while the board is answering.
pub const WIFI_BUSY: &str = "Studio is talking to the board — a moment";

/// Why a secure network's password may not be left blank.
pub const PASSWORD_NEEDED: &str = "A Wi‑Fi password is at least 8 characters.";

/// Every Wi‑Fi offer `wifi` makes, under the board prefix `prefix`.
pub fn wifi_offers(prefix: &OfferPath, wifi: &UiDeviceWifi) -> Vec<UiOffer> {
    let Some(status) = wifi.status.as_ref().filter(|_| wifi.can_edit) else {
        return Vec::new();
    };
    let at = |verb: &str| prefix.clone().child(WIFI_SEGMENT).child(verb);
    let busy = wifi.reading || wifi.writing;
    let device = wifi.device;
    let saved: Vec<String> = status
        .networks
        .iter()
        .map(|network| network.ssid.clone())
        .collect();
    let secure: Vec<String> = wifi
        .heard
        .iter()
        .flatten()
        .filter(|heard| heard.secure)
        .map(|heard| heard.ssid.clone())
        .collect();
    let mut offers = vec![add_offer(
        at("add"),
        device,
        AddRules {
            saved: saved.clone(),
            secure,
            label: if wifi.can_connect() {
                "Connect"
            } else {
                "Save"
            },
        },
        busy,
    )];
    let slugs = wifi_network_slugs(saved.iter().map(String::as_str));
    for (ssid, slug) in saved.into_iter().zip(slugs) {
        let forget = NetworkOp::action_for(device, NetworkChange::Forget { ssid: ssid.clone() })
            .lasting(ActionConfirmation::new(
                format!("Forget {ssid}?"),
                FORGET_HELP,
                "Forget",
            ))
            .with_summary(format!("Forget {ssid} and its password."));
        offers.push(UiOffer::new(
            at(WIFI_FORGET_SEGMENT).child(slug),
            "remove",
            disabled_if(forget, busy),
        ));
    }
    offers.push(toggle_offer(
        at("enabled"),
        device,
        "Wi‑Fi",
        None,
        status.wifi,
        busy,
        |wifi| NetworkChange::Switches {
            wifi: Some(wifi),
            cloud_relay: None,
        },
    ));
    offers.push(toggle_offer(
        at("cloud-relay"),
        device,
        "cloud relay",
        Some(WIFI_CLOUD_RELAY_SUMMARY),
        status.cloud_relay,
        busy,
        |cloud_relay| NetworkChange::Switches {
            wifi: None,
            cloud_relay: Some(cloud_relay),
        },
    ));
    offers
}

/// What the add binder checks against: the names saved (a new one past
/// eight is refused; a saved one is a password change), the names the
/// radio heard as secure (no blank password for those), and the verb's
/// word.
#[derive(Clone)]
struct AddRules {
    saved: Vec<String>,
    secure: Vec<String>,
    label: &'static str,
}

/// `wifi/add`: the network's name, its password, and whether it hides.
fn add_offer(path: OfferPath, device: DeviceId, rules: AddRules, busy: bool) -> UiOffer {
    let network = OfferParam::text(WIFI_NETWORK_PARAM, "network name", "Network name").max_len(32);
    let password = OfferParam::text(WIFI_PASSWORD_PARAM, "password", "empty if open")
        .optional()
        .secret();
    let hidden = OfferParam::toggle(WIFI_HIDDEN_PARAM, "hidden network", false);
    let unbound = NetworkOp::action_for(
        device,
        NetworkChange::Add {
            ssid: String::new(),
            password: PasswordChange::Open,
            hidden: None,
        },
    )
    .with_label(rules.label);
    UiOffer::with_params(
        path,
        "wifi",
        vec![network, password, hidden],
        OfferBinder::new(move |args: &OfferArgs| {
            let action = bind_add(device, &rules, args)?;
            Ok(disabled_if(action.with_label(rules.label), busy))
        }),
        disabled_if(unbound, busy),
    )
}

/// An add from the form's values; see [`add_offer`].
fn bind_add(
    device: DeviceId,
    rules: &AddRules,
    args: &OfferArgs,
) -> Result<UiAction, OfferArgError> {
    let Some(ssid) = args.text(WIFI_NETWORK_PARAM) else {
        return Err(OfferArgError::Missing {
            name: WIFI_NETWORK_PARAM.to_string(),
            label: "network name".to_string(),
        });
    };
    validate_ssid(ssid).map_err(|rule| OfferArgError::Invalid {
        name: WIFI_NETWORK_PARAM.to_string(),
        reason: rule.to_string(),
    })?;
    if !rules.saved.iter().any(|saved| saved == ssid)
        && rules.saved.len() >= NetworkFile::MAX_NETWORKS
    {
        return Err(OfferArgError::Invalid {
            name: WIFI_NETWORK_PARAM.to_string(),
            reason: lpc_access::NetworkFileError::TooManyNetworks {
                max: NetworkFile::MAX_NETWORKS,
            }
            .to_string(),
        });
    }
    // A password is taken as typed (spaces count), never trimmed.
    let password = match args
        .get(WIFI_PASSWORD_PARAM)
        .filter(|password| !password.is_empty())
    {
        Some(password) => {
            // The rule's words name the length, never the text.
            validate_password(password).map_err(|rule| OfferArgError::Invalid {
                name: WIFI_PASSWORD_PARAM.to_string(),
                reason: rule.to_string(),
            })?;
            PasswordChange::Set(password.to_string())
        }
        None if rules.secure.iter().any(|secure| secure == ssid) => {
            return Err(OfferArgError::Invalid {
                name: WIFI_PASSWORD_PARAM.to_string(),
                reason: PASSWORD_NEEDED.to_string(),
            });
        }
        None => PasswordChange::Open,
    };
    // Off leaves a saved network's `hidden` as it is.
    let hidden = (args.toggle(WIFI_HIDDEN_PARAM) == Some(true)).then_some(true);
    Ok(NetworkOp::action_for(
        device,
        NetworkChange::Add {
            ssid: ssid.to_string(),
            password,
            hidden,
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
    use lpc_wire::server::{HeardNetwork, NetworkStatus, SavedNetworkInfo, StationState};

    const PASSWORD: &str = "correct-horse-42";

    #[test]
    fn a_board_with_no_network_offers_add_and_the_two_switches() {
        let offers = wifi_offers(&prefix(), &wifi(&[]));
        assert_eq!(verbs(&offers), ["add", "enabled", "cloud-relay"]);
        let add = &offers[0];
        assert!(add.takes_a_secret());
        assert!(!add.is_enabled(), "a new network needs its name");
        assert_eq!(
            add.label(),
            "Save",
            "today's firmware saves, it can't connect"
        );
        assert!(add.consequence().is_routine());

        let pressed = add
            .press(
                &OfferArgs::new()
                    .with(WIFI_NETWORK_PARAM, "lp-walk-net")
                    .with(WIFI_PASSWORD_PARAM, PASSWORD),
            )
            .unwrap();
        assert_eq!(
            op(&pressed).change,
            NetworkChange::Add {
                ssid: "lp-walk-net".to_string(),
                password: PasswordChange::Set(PASSWORD.to_string()),
                hidden: None,
            }
        );
        // Nothing the agent or a log reads holds it.
        assert!(!format!("{pressed:?}").contains(PASSWORD));
        assert_eq!(
            pressed.offer_press().unwrap().args.get(WIFI_PASSWORD_PARAM),
            Some(crate::SECRET_MARKER)
        );

        // No password: an open network; hidden only when asked.
        let open = add
            .press(
                &OfferArgs::new()
                    .with(WIFI_NETWORK_PARAM, "lp-cafe")
                    .with(WIFI_HIDDEN_PARAM, "true"),
            )
            .unwrap();
        assert_eq!(
            op(&open).change,
            NetworkChange::Add {
                ssid: "lp-cafe".to_string(),
                password: PasswordChange::Open,
                hidden: Some(true),
            }
        );
    }

    #[test]
    fn each_saved_network_has_its_own_lasting_forget() {
        let offers = wifi_offers(&prefix(), &wifi(&["Starlink Home", "cafe.guest"]));
        assert_eq!(
            verbs(&offers),
            [
                "add",
                "forget/starlink-home",
                "forget/cafe-guest",
                "enabled",
                "cloud-relay"
            ]
        );
        let forget = &offers[2];
        assert!(forget.consequence().arms());
        let copy = forget.consequence().copy().unwrap();
        assert_eq!(copy.title, "Forget cafe.guest?");
        assert_eq!(copy.message, FORGET_HELP);
        assert_eq!(
            op(&forget.action).change,
            NetworkChange::Forget {
                ssid: "cafe.guest".to_string()
            }
        );
    }

    #[test]
    fn the_binder_mirrors_the_boards_rules_without_quoting_the_password() {
        let add = wifi_offers(&prefix(), &wifi(&[])).remove(0);
        let short = add
            .press(
                &OfferArgs::new()
                    .with(WIFI_NETWORK_PARAM, "lp-walk-net")
                    .with(WIFI_PASSWORD_PARAM, "seven77"),
            )
            .unwrap_err()
            .to_string();
        assert!(short.contains("7 characters"), "{short}");
        assert!(!short.contains("seven77"), "{short}");
        let long = add
            .press(&OfferArgs::new().with(WIFI_NETWORK_PARAM, "x".repeat(33)))
            .unwrap_err()
            .to_string();
        assert!(long.contains("32"), "{long}");
        // Multibyte: 11 characters, 33 bytes — the board counts bytes.
        let wide = add
            .press(&OfferArgs::new().with(WIFI_NETWORK_PARAM, "ネットワークの名前です"))
            .unwrap_err()
            .to_string();
        assert!(wide.contains("bytes"), "{wide}");
    }

    #[test]
    fn a_ninth_network_is_refused_but_a_saved_one_still_changes() {
        let names: Vec<String> = (0..8).map(|n| format!("lp-net-{n}")).collect();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        let add = wifi_offers(&prefix(), &wifi(&names)).remove(0);
        let refused = add
            .press(&OfferArgs::new().with(WIFI_NETWORK_PARAM, "lp-net-9"))
            .unwrap_err()
            .to_string();
        assert!(refused.contains("at most 8"), "{refused}");
        assert!(
            add.press(
                &OfferArgs::new()
                    .with(WIFI_NETWORK_PARAM, "lp-net-3")
                    .with(WIFI_PASSWORD_PARAM, PASSWORD)
            )
            .is_ok()
        );
    }

    #[test]
    fn a_board_that_can_connect_says_connect_and_wants_a_secure_networks_password() {
        let mut board = wifi(&[]);
        board.status.as_mut().unwrap().station = StationState::NotConnected;
        board.heard = Some(vec![HeardNetwork {
            ssid: "NETGEAR42".to_string(),
            rssi: -71,
            secure: true,
        }]);
        let add = wifi_offers(&prefix(), &board).remove(0);
        assert_eq!(add.label(), "Connect");
        let refused = add
            .press(&OfferArgs::new().with(WIFI_NETWORK_PARAM, "NETGEAR42"))
            .unwrap_err()
            .to_string();
        assert!(refused.contains(PASSWORD_NEEDED), "{refused}");
    }

    #[test]
    fn the_switches_bind_their_switch_and_nothing_else() {
        let offers = wifi_offers(&prefix(), &wifi(&["lp-walk-net"]));
        let enabled = offers
            .iter()
            .find(|o| o.path.last() == Some("enabled"))
            .unwrap();
        assert_eq!(enabled.params()[0].label, "Wi‑Fi");
        let off = enabled
            .press(&OfferArgs::new().with(WIFI_ENABLED_PARAM, "false"))
            .unwrap();
        assert_eq!(
            op(&off).change,
            NetworkChange::Switches {
                wifi: Some(false),
                cloud_relay: None,
            }
        );
        let relay = offers.last().unwrap();
        assert_eq!(relay.params()[0].label, "cloud relay");
        assert_eq!(relay.summary(), WIFI_CLOUD_RELAY_SUMMARY);
        let relay_off = relay
            .press(&OfferArgs::new().with(WIFI_ENABLED_PARAM, "false"))
            .unwrap();
        assert_eq!(
            op(&relay_off).change,
            NetworkChange::Switches {
                wifi: None,
                cloud_relay: Some(false),
            }
        );
    }

    #[test]
    fn busy_draws_every_verb_disabled_and_no_author_offers_nothing() {
        let mut busy = wifi(&["lp-walk-net"]);
        busy.writing = true;
        let offers = wifi_offers(&prefix(), &busy);
        assert_eq!(offers.len(), 4, "drawn, not hidden");
        assert!(offers.iter().all(|offer| !offer.is_enabled()));

        let mut play = wifi(&["lp-walk-net"]);
        play.can_edit = false;
        assert!(wifi_offers(&prefix(), &play).is_empty());
        let mut unread = wifi(&[]);
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

    fn wifi(saved: &[&str]) -> UiDeviceWifi {
        UiDeviceWifi {
            status: Some(NetworkStatus {
                wifi: true,
                cloud_relay: true,
                networks: saved
                    .iter()
                    .map(|ssid| SavedNetworkInfo {
                        ssid: ssid.to_string(),
                        has_password: true,
                        hidden: false,
                        last: None,
                    })
                    .collect(),
                station: StationState::Unsupported,
            }),
            ..UiDeviceWifi::new(DeviceId(7), true)
        }
    }
}
