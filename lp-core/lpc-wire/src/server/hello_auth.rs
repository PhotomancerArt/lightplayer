//! What the link that receives a hello may do.

use lpc_access::Tier;
use serde::{Deserialize, Serialize};

/// The access half of a [`crate::ServerHello`], computed for the ONE link
/// the hello is sent on — two links on the same device can read different
/// answers.
///
/// - `required` — this link's tier comes from logging in (an untrusted,
///   radio link). `false` on a trusted link (USB, host, browser worker),
///   which holds edit by what it physically is.
/// - `granted` — the tier the link holds right now: edit on a trusted link;
///   on an untrusted one, what its login earned, else play when the device
///   is `open`, else nothing (`None`: only `hello` and `login*` are answered).
///
/// A client reads it to decide whether to log in before anything else, and
/// what to offer once it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HelloAuth {
    pub required: bool,
    pub granted: Option<Tier>,
}

impl HelloAuth {
    /// A trusted link: no login, edit.
    pub const TRUSTED: HelloAuth = HelloAuth {
        required: false,
        granted: Some(Tier::Edit),
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serializes_camel_case_with_the_tier_by_name() {
        assert_eq!(
            crate::json::to_string(&HelloAuth::TRUSTED).unwrap(),
            "{\"required\":false,\"granted\":\"edit\"}"
        );
        let locked = HelloAuth {
            required: true,
            granted: None,
        };
        let json = crate::json::to_string(&locked).unwrap();
        assert_eq!(json, "{\"required\":true,\"granted\":null}");
        assert_eq!(crate::json::from_str::<HelloAuth>(&json).unwrap(), locked);
    }
}
