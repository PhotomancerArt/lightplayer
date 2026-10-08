//! The networks a board can hear.
//!
//! A virtual access point is a name, a password (or none, for an open
//! network), a signal strength and whether it hides its name. Every access
//! point on one [`super::VirtualLan`] bridges to the same segment, so several
//! of them stand for a house with more than one network in range, or a mesh
//! with one name heard at two strengths (the "strongest heard" rule a board's
//! join policy follows).
//!
//! **The signal is a configured number**, never a model: it does not fade,
//! it does not depend on distance, and nothing is lost because of it.
//!
//! Security is a password match and nothing else: no WPA handshake, no
//! authentication mode threshold. An open network joins whatever password
//! the board offers, since an emulator that refused one would be modelling
//! a driver's threshold setting, not a network.

/// One network in range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VirtualAccessPoint {
    /// The SSID.
    pub name: String,
    /// `None`: an open network.
    pub password: Option<String>,
    /// The strength a scan reports, in dBm (−30 near, −90 at the edge).
    pub signal_dbm: i8,
    /// Left out of scans; still joinable by name.
    pub hidden: bool,
}

/// One network as a scan reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanRecord {
    pub name: String,
    pub signal_dbm: i8,
    /// It asks for a password.
    pub secure: bool,
}

/// What joining a network by name and password comes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JoinOutcome {
    /// In range and the password is right: associated with this access point
    /// (an index into the list joined against).
    Associated(usize),
    /// A network by that name is in range and refused the password.
    AuthFailed,
    /// Nothing by that name is in range.
    NotFound,
}

impl VirtualAccessPoint {
    /// An open network.
    pub fn open(name: &str, signal_dbm: i8) -> Self {
        Self {
            name: name.to_string(),
            password: None,
            signal_dbm,
            hidden: false,
        }
    }

    /// A network that asks for `password`.
    pub fn secured(name: &str, password: &str, signal_dbm: i8) -> Self {
        Self {
            password: Some(password.to_string()),
            ..Self::open(name, signal_dbm)
        }
    }

    /// The same network, its name hidden from scans.
    pub fn hidden(self) -> Self {
        Self {
            hidden: true,
            ..self
        }
    }

    pub fn is_secure(&self) -> bool {
        self.password.is_some()
    }

    fn accepts(&self, password: &[u8]) -> bool {
        match &self.password {
            None => true,
            Some(p) => p.as_bytes() == password,
        }
    }
}

/// What a scan hears: every network not hidden, strongest first; equal
/// strengths by name, so a scan reads the same on every run.
pub fn scan(access_points: &[VirtualAccessPoint]) -> Vec<ScanRecord> {
    let mut heard: Vec<ScanRecord> = access_points
        .iter()
        .filter(|ap| !ap.hidden)
        .map(|ap| ScanRecord {
            name: ap.name.clone(),
            signal_dbm: ap.signal_dbm,
            secure: ap.is_secure(),
        })
        .collect();
    heard.sort_by(|a, b| {
        b.signal_dbm
            .cmp(&a.signal_dbm)
            .then_with(|| a.name.cmp(&b.name))
    });
    heard
}

/// Join by name and password. Of several access points with the name, the
/// strongest that takes the password (the first listed among equals).
pub fn join(access_points: &[VirtualAccessPoint], name: &[u8], password: &[u8]) -> JoinOutcome {
    let mut named = access_points
        .iter()
        .enumerate()
        .filter(|(_, ap)| ap.name.as_bytes() == name)
        .peekable();
    if named.peek().is_none() {
        return JoinOutcome::NotFound;
    }
    named
        .filter(|(_, ap)| ap.accepts(password))
        .fold(None::<(usize, i8)>, |best, (i, ap)| match best {
            Some((_, s)) if s >= ap.signal_dbm => best,
            _ => Some((i, ap.signal_dbm)),
        })
        .map_or(JoinOutcome::AuthFailed, |(i, _)| JoinOutcome::Associated(i))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn a_right_password_associates_a_wrong_one_fails_and_an_unknown_name_is_not_found() {
        let aps = fixture_access_points();
        let home = aps.iter().position(|a| a.name == "home").unwrap();
        assert_eq!(
            join(&aps, b"home", b"test-password-1"),
            JoinOutcome::Associated(home)
        );
        assert_eq!(join(&aps, b"home", b"wrong"), JoinOutcome::AuthFailed);
        assert_eq!(join(&aps, b"home", b""), JoinOutcome::AuthFailed);
        assert_eq!(join(&aps, b"nowhere", b"x"), JoinOutcome::NotFound);
    }

    #[test]
    fn an_open_network_joins_with_any_password() {
        let aps = fixture_access_points();
        let cafe = aps.iter().position(|a| a.name == "cafe").unwrap();
        assert_eq!(join(&aps, b"cafe", b""), JoinOutcome::Associated(cafe));
        assert_eq!(
            join(&aps, b"cafe", b"anything"),
            JoinOutcome::Associated(cafe)
        );
    }

    #[test]
    fn two_access_points_are_heard_strongest_first_and_hidden_ones_are_left_out() {
        let aps = fixture_access_points();
        let heard = scan(&aps);
        let names: Vec<(&str, i8, bool)> = heard
            .iter()
            .map(|r| (r.name.as_str(), r.signal_dbm, r.secure))
            .collect();
        assert_eq!(
            names,
            [
                ("home", -45, true),
                ("home", -70, true),
                ("cafe", -80, false)
            ]
        );
        assert!(!heard.iter().any(|r| r.name == "attic"));
    }

    #[test]
    fn a_mesh_joins_its_strongest_access_point_and_a_hidden_one_by_name() {
        let aps = fixture_access_points();
        let strongest = aps
            .iter()
            .position(|a| a.name == "home" && a.signal_dbm == -45)
            .unwrap();
        assert_eq!(
            join(&aps, b"home", b"test-password-1"),
            JoinOutcome::Associated(strongest)
        );
        let attic = aps.iter().position(|a| a.name == "attic").unwrap();
        assert_eq!(
            join(&aps, b"attic", b"test-password-2"),
            JoinOutcome::Associated(attic)
        );
    }

    /// The test networks, from `testdata/virtual_lan.toml`: test values,
    /// never real credentials.
    pub(crate) fn fixture_access_points() -> Vec<VirtualAccessPoint> {
        let text = include_str!("../../../testdata/virtual_lan.toml");
        let doc: toml::Table = text.parse().expect("the fixture is TOML");
        doc["access_point"]
            .as_array()
            .expect("[[access_point]]")
            .iter()
            .map(|ap| {
                let name = ap["name"].as_str().expect("name");
                let signal = i8::try_from(ap["signal_dbm"].as_integer().expect("signal_dbm"))
                    .expect("signal_dbm fits an i8");
                let ap_value = match ap.get("password").and_then(|p| p.as_str()) {
                    Some(p) => VirtualAccessPoint::secured(name, p, signal),
                    None => VirtualAccessPoint::open(name, signal),
                };
                if ap.get("hidden").and_then(|h| h.as_bool()) == Some(true) {
                    ap_value.hidden()
                } else {
                    ap_value
                }
            })
            .collect()
    }
}
