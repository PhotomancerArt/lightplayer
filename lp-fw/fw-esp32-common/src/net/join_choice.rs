//! Which saved network the station tries next.

use alloc::string::String;
use lpc_wire::HeardNetwork;

use crate::net::station_settings::StationSettings;

/// The network to try, and how.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinChoice {
    /// A saved network the radio heard, at `rssi` dBm.
    Heard { ssid: String, rssi: i8 },
    /// A hidden saved network, tried by name: the radio cannot hear it by
    /// listening.
    Hidden { ssid: String },
}

impl JoinChoice {
    /// The network's name.
    #[must_use]
    pub fn ssid(&self) -> &str {
        match self {
            Self::Heard { ssid, .. } | Self::Hidden { ssid } => ssid,
        }
    }
}

/// The join rule (settled before planning; Yona: "fine for now"): **the
/// strongest saved network heard**, with no priority order among saved
/// networks, skipping any `skip` says no to (one whose password was refused
/// until it changes, or one already tried this round). When none of the
/// heard ones is left, the hidden saved networks are tried by name, in the
/// file's order.
pub fn choose(
    settings: &StationSettings,
    heard: &[HeardNetwork],
    skip: impl Fn(&str) -> bool,
) -> Option<JoinChoice> {
    let strongest = heard
        .iter()
        .filter(|network| settings.network(&network.ssid).is_some())
        .filter(|network| !skip(&network.ssid))
        .max_by_key(|network| network.rssi);
    if let Some(network) = strongest {
        return Some(JoinChoice::Heard {
            ssid: network.ssid.clone(),
            rssi: network.rssi,
        });
    }
    settings
        .networks
        .iter()
        .filter(|network| network.hidden && !skip(&network.ssid))
        .map(|network| JoinChoice::Hidden {
            ssid: network.ssid.clone(),
        })
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::net::station_settings::SavedNetwork;
    use alloc::vec::Vec;

    fn saved(names: &[(&str, bool)]) -> StationSettings {
        StationSettings {
            wifi: true,
            networks: names
                .iter()
                .map(|(ssid, hidden)| SavedNetwork {
                    ssid: String::from(*ssid),
                    hidden: *hidden,
                    secret_tag: 1,
                })
                .collect(),
        }
    }

    fn heard(list: &[(&str, i8)]) -> Vec<HeardNetwork> {
        list.iter()
            .map(|(ssid, rssi)| HeardNetwork {
                ssid: String::from(*ssid),
                rssi: *rssi,
                secure: true,
            })
            .collect()
    }

    #[test]
    fn the_strongest_saved_network_heard_wins_whatever_the_file_order() {
        let settings = saved(&[("lp-back-office", false), ("lp-walk-net", false)]);
        let choice = choose(
            &settings,
            &heard(&[
                ("lp-cafe", -30),
                ("lp-back-office", -70),
                ("lp-walk-net", -50),
            ]),
            |_| false,
        );
        assert_eq!(
            choice,
            Some(JoinChoice::Heard {
                ssid: String::from("lp-walk-net"),
                rssi: -50
            })
        );
    }

    #[test]
    fn a_skipped_network_gives_way_to_the_next_strongest() {
        let settings = saved(&[("lp-back-office", false), ("lp-walk-net", false)]);
        let choice = choose(
            &settings,
            &heard(&[("lp-back-office", -70), ("lp-walk-net", -50)]),
            |ssid| ssid == "lp-walk-net",
        );
        assert_eq!(
            choice.as_ref().map(JoinChoice::ssid),
            Some("lp-back-office")
        );
    }

    #[test]
    fn hidden_networks_are_tried_by_name_after_the_heard_ones() {
        let settings = saved(&[("lp-attic", true), ("lp-walk-net", false)]);
        let list = heard(&[("lp-walk-net", -50)]);
        assert_eq!(
            choose(&settings, &list, |_| false)
                .as_ref()
                .map(JoinChoice::ssid),
            Some("lp-walk-net")
        );
        assert_eq!(
            choose(&settings, &list, |ssid| ssid == "lp-walk-net"),
            Some(JoinChoice::Hidden {
                ssid: String::from("lp-attic")
            })
        );
        assert_eq!(choose(&settings, &list, |_| true), None);
    }

    #[test]
    fn nothing_saved_heard_and_nothing_hidden_is_no_choice() {
        let settings = saved(&[("lp-walk-net", false)]);
        assert_eq!(
            choose(&settings, &heard(&[("lp-cafe", -30)]), |_| false),
            None
        );
        assert_eq!(choose(&settings, &[], |_| false), None);
    }
}
