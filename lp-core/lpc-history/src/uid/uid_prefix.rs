//! The kind prefix of a [`super::prefixed_uid::PrefixedUid`].

use core::fmt;
use core::str::FromStr;

use super::prefixed_uid::UidParseError;

/// Kind prefix of a prefixed uid: what sort of thing the uid names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UidPrefix {
    /// A project package (`prj_…`).
    Project,
    /// A module package (`mod_…`).
    Module,
    /// A physical or virtual device (`dev_…`).
    Device,
}

impl UidPrefix {
    /// All known prefixes.
    pub const ALL: [UidPrefix; 3] = [UidPrefix::Project, UidPrefix::Module, UidPrefix::Device];

    /// The canonical three-letter prefix string (without the `_` separator).
    pub fn as_str(&self) -> &'static str {
        match self {
            UidPrefix::Project => "prj",
            UidPrefix::Module => "mod",
            UidPrefix::Device => "dev",
        }
    }
}

impl fmt::Display for UidPrefix {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for UidPrefix {
    type Err = UidParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "prj" => Ok(UidPrefix::Project),
            "mod" => Ok(UidPrefix::Module),
            "dev" => Ok(UidPrefix::Device),
            _ => Err(UidParseError::UnknownPrefix),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_all_prefixes() {
        for prefix in UidPrefix::ALL {
            assert_eq!(prefix.as_str().parse::<UidPrefix>().unwrap(), prefix);
        }
    }

    #[test]
    fn rejects_unknown_prefix() {
        assert_eq!(
            "prx".parse::<UidPrefix>(),
            Err(UidParseError::UnknownPrefix)
        );
        assert_eq!("".parse::<UidPrefix>(), Err(UidParseError::UnknownPrefix));
    }
}
