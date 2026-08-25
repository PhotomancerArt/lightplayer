//! Identity as a **chain of bindings**, each learned from evidence and each
//! revocable: port endpoint → chip MAC → provisioned uid → provisioned name.
//!
//! [`DeviceId`] is an app-side handle, deliberately NOT the uid: an
//! anonymous board (blank flash, no provisioned identity) still needs one
//! stable entry to render, name, and forget. Promotion (learning a stronger
//! binding) and conflict (a binding that contradicts what was assumed) are
//! journaled operations, never accidents of map keys.

use serde::{Deserialize, Serialize};

/// Stable app-side handle for one known device. Survives anonymity, link
/// churn, and identity promotion; minted by the roster, never by hardware.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct DeviceId(pub u64);

/// Provisioned LightPlayer device uid (the strongest binding).
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct DeviceUid(pub String);

/// Silicon MAC, as reported by the peer. Stronger than an endpoint (survives
/// re-plugging into another port), weaker than a uid (an unprovisioned board
/// has a MAC but no uid).
#[derive(Clone, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct MacAddress(pub String);

/// Fingerprint of the transport endpoint a link speaks through (a granted
/// serial port, in practice). The weakest binding: correct until someone
/// moves a cable.
#[derive(Clone, Debug, Default, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct EndpointKey(pub String);

/// What a peer said about itself on the wire. Hellos always carry this;
/// heartbeats carry it too once the firmware stamps identity into them
/// (vision R4), which is what lets a mid-stream attach resolve identity
/// passively.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct PeerIdentity {
    pub uid: Option<DeviceUid>,
    pub mac: Option<MacAddress>,
    /// Name the device reports for itself, if it was provisioned with one.
    pub name: Option<String>,
}

impl PeerIdentity {
    pub fn is_empty(&self) -> bool {
        self.uid.is_none() && self.mac.is_none() && self.name.is_none()
    }
}

/// The bindings known for one device, strongest last.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct IdentityChain {
    pub endpoint: Option<EndpointKey>,
    pub mac: Option<MacAddress>,
    pub uid: Option<DeviceUid>,
    /// The *provisioned* name (device-reported). The user's chosen name is
    /// intent, not identity — see [`crate::Intent::name`].
    pub name: Option<String>,
}

/// Which rung of the chain a promotion or conflict concerns.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum IdentityBinding {
    Endpoint,
    Mac,
    Uid,
    Name,
}

/// How strongly two chains agree. Ordering is the point: a uid match beats a
/// MAC match beats an endpoint match, so a link that arrives on a familiar
/// port but announces a different uid is re-routed rather than mis-joined.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub enum IdentityMatch {
    Endpoint,
    Mac,
    Uid,
}

/// What learning one [`PeerIdentity`] did to a chain.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IdentityLearned {
    /// Bindings that went from unknown to known, with their new values.
    pub promotions: Vec<(IdentityBinding, String)>,
    /// Bindings whose newly observed value CONTRADICTS the stored one. The
    /// roster treats a uid conflict as a re-route signal.
    pub conflicts: Vec<(IdentityBinding, String)>,
}

impl IdentityLearned {
    pub fn is_empty(&self) -> bool {
        self.promotions.is_empty() && self.conflicts.is_empty()
    }
}

impl IdentityChain {
    /// A chain bound only to where it is plugged in (or to nothing at all).
    /// Anonymous devices are exactly the ones the old system could never
    /// forget, so the model keeps the concept explicit.
    pub fn is_anonymous(&self) -> bool {
        self.uid.is_none() && self.mac.is_none()
    }

    /// The strongest binding held, as a display string.
    pub fn strongest_label(&self) -> Option<String> {
        if let Some(uid) = &self.uid {
            return Some(uid.0.clone());
        }
        if let Some(mac) = &self.mac {
            return Some(mac.0.clone());
        }
        self.endpoint.as_ref().map(|endpoint| endpoint.0.clone())
    }

    /// Fold one peer announcement into the chain, reporting promotions and
    /// contradictions. Called ONLY from the evidence fold.
    pub fn learn(&mut self, observed: &PeerIdentity) -> IdentityLearned {
        let mut learned = IdentityLearned::default();
        if let Some(uid) = &observed.uid {
            match &self.uid {
                Some(existing) if existing == uid => {}
                Some(_) => learned
                    .conflicts
                    .push((IdentityBinding::Uid, uid.0.clone())),
                None => {
                    self.uid = Some(uid.clone());
                    learned
                        .promotions
                        .push((IdentityBinding::Uid, uid.0.clone()));
                }
            }
        }
        if let Some(mac) = &observed.mac {
            match &self.mac {
                Some(existing) if existing == mac => {}
                Some(_) => learned
                    .conflicts
                    .push((IdentityBinding::Mac, mac.0.clone())),
                None => {
                    self.mac = Some(mac.clone());
                    learned
                        .promotions
                        .push((IdentityBinding::Mac, mac.0.clone()));
                }
            }
        }
        if let Some(name) = &observed.name {
            if self.name.as_deref() != Some(name.as_str()) {
                let promoted = self.name.is_none();
                self.name = Some(name.clone());
                let binding = IdentityBinding::Name;
                if promoted {
                    learned.promotions.push((binding, name.clone()));
                } else {
                    learned.conflicts.push((binding, name.clone()));
                }
            }
        }
        learned
    }

    /// Bind (or re-bind) the endpoint rung. Endpoints are the revocable
    /// rung: plugging the same board into another port replaces it silently.
    pub fn bind_endpoint(&mut self, endpoint: EndpointKey) -> Option<IdentityLearned> {
        if self.endpoint.as_ref() == Some(&endpoint) {
            return None;
        }
        let promoted = self.endpoint.is_none();
        let value = endpoint.0.clone();
        self.endpoint = Some(endpoint);
        let mut learned = IdentityLearned::default();
        if promoted {
            learned.promotions.push((IdentityBinding::Endpoint, value));
        } else {
            learned.conflicts.push((IdentityBinding::Endpoint, value));
        }
        Some(learned)
    }

    /// The strongest rung on which these two chains agree, if any.
    pub fn match_against(&self, other: &Self) -> Option<IdentityMatch> {
        if let (Some(left), Some(right)) = (&self.uid, &other.uid) {
            if left == right {
                return Some(IdentityMatch::Uid);
            }
            // A uid DISAGREEMENT is decisive: weaker rungs cannot rescue it.
            return None;
        }
        if let (Some(left), Some(right)) = (&self.mac, &other.mac) {
            if left == right {
                return Some(IdentityMatch::Mac);
            }
            return None;
        }
        if let (Some(left), Some(right)) = (&self.endpoint, &other.endpoint) {
            if left == right {
                return Some(IdentityMatch::Endpoint);
            }
        }
        None
    }

    /// Absorb another chain's bindings, keeping this chain's stronger ones.
    /// Used by the merge operation when an anonymous entry turns out to be a
    /// device that already had a record.
    pub fn absorb(&mut self, other: &Self) {
        if self.uid.is_none() {
            self.uid = other.uid.clone();
        }
        if self.mac.is_none() {
            self.mac = other.mac.clone();
        }
        if self.endpoint.is_none() {
            self.endpoint = other.endpoint.clone();
        }
        if self.name.is_none() {
            self.name = other.name.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn learning_a_uid_promotes_once_and_then_stays_quiet() {
        let mut chain = IdentityChain::default();
        let observed = PeerIdentity {
            uid: Some(DeviceUid("dev_abc".to_string())),
            ..Default::default()
        };

        let first = chain.learn(&observed);
        assert_eq!(
            first.promotions,
            vec![(IdentityBinding::Uid, "dev_abc".to_string())]
        );

        let second = chain.learn(&observed);
        assert!(second.is_empty(), "re-hearing the same uid is not news");
    }

    #[test]
    fn a_contradicting_uid_is_a_conflict_not_a_promotion() {
        let mut chain = IdentityChain {
            uid: Some(DeviceUid("dev_abc".to_string())),
            ..Default::default()
        };

        let learned = chain.learn(&PeerIdentity {
            uid: Some(DeviceUid("dev_xyz".to_string())),
            ..Default::default()
        });

        assert!(learned.promotions.is_empty());
        assert_eq!(
            learned.conflicts,
            vec![(IdentityBinding::Uid, "dev_xyz".to_string())]
        );
        assert_eq!(chain.uid, Some(DeviceUid("dev_abc".to_string())));
    }

    #[test]
    fn matching_prefers_the_strongest_rung_and_a_uid_clash_is_decisive() {
        let endpoint = EndpointKey("usb-1".to_string());
        let left = IdentityChain {
            endpoint: Some(endpoint.clone()),
            uid: Some(DeviceUid("dev_abc".to_string())),
            ..Default::default()
        };
        let same_uid = IdentityChain {
            uid: Some(DeviceUid("dev_abc".to_string())),
            ..Default::default()
        };
        let other_uid_same_port = IdentityChain {
            endpoint: Some(endpoint.clone()),
            uid: Some(DeviceUid("dev_xyz".to_string())),
            ..Default::default()
        };
        let anonymous_same_port = IdentityChain {
            endpoint: Some(endpoint),
            ..Default::default()
        };

        assert_eq!(left.match_against(&same_uid), Some(IdentityMatch::Uid));
        assert_eq!(left.match_against(&other_uid_same_port), None);
        assert_eq!(
            left.match_against(&anonymous_same_port),
            Some(IdentityMatch::Endpoint)
        );
        assert!(IdentityMatch::Uid > IdentityMatch::Mac);
        assert!(IdentityMatch::Mac > IdentityMatch::Endpoint);
    }

    #[test]
    fn a_board_with_only_a_port_is_anonymous() {
        let chain = IdentityChain {
            endpoint: Some(EndpointKey("usb-1".to_string())),
            ..Default::default()
        };

        assert!(chain.is_anonymous());
    }
}
