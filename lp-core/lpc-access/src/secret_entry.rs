//! One installed secret: a labelled, tiered login key.

use alloc::string::String;
use core::fmt;
use serde::{Deserialize, Serialize};

use crate::tier::Tier;

/// Salt length, in bytes, of every stored secret.
pub const SALT_BYTES: usize = 16;

/// Length, in bytes, of a derived login key `K`.
pub const KEY_BYTES: usize = 32;

/// A shared secret the board accepts, stored as the KDF's OUTPUT.
///
/// `k = PBKDF2-HMAC-SHA256(password, salt, iterations)` is computed by the
/// client that installs the secret; the password itself is never stored and
/// never crosses a link. `k` is login-equivalent — anyone holding it can
/// answer a challenge — which is accepted (the threat model is someone
/// cheeky nearby, not a cracker with the flash chip), and is why no link at
/// any tier can read an access file back.
///
/// `label` names the secret for people ("camp", "mine"). It is never
/// offered before login; a successful login names the label it matched.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretEntry {
    /// Human name for this secret.
    pub label: String,
    /// What a login with this secret grants.
    pub tier: Tier,
    /// PBKDF2 salt, base64 (16 bytes).
    #[serde(with = "crate::base64_bytes")]
    #[cfg_attr(feature = "schema-gen", schemars(with = "String"))]
    pub salt: [u8; SALT_BYTES],
    /// PBKDF2 iteration count (≥ 1), tuned by the installing client.
    #[cfg_attr(feature = "schema-gen", schemars(range(min = 1)))]
    pub iterations: u32,
    /// The derived key `K`, base64 (32 bytes).
    #[serde(with = "crate::base64_bytes")]
    #[cfg_attr(feature = "schema-gen", schemars(with = "String"))]
    pub k: [u8; KEY_BYTES],
}

impl SecretEntry {
    /// Build an entry from a password, deriving `k` on the spot. Client
    /// side only: the board never derives.
    #[must_use]
    pub fn from_password(
        label: impl Into<String>,
        tier: Tier,
        password: &[u8],
        salt: [u8; SALT_BYTES],
        iterations: u32,
    ) -> Self {
        Self {
            label: label.into(),
            tier,
            salt,
            iterations,
            k: crate::pbkdf2_sha256::derive_login_key(password, &salt, iterations),
        }
    }
}

/// `k` is login-equivalent: a Debug print (a log line, a panic message)
/// must never carry it.
impl fmt::Debug for SecretEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretEntry")
            .field("label", &self.label)
            .field("tier", &self.tier)
            .field("iterations", &self.iterations)
            .field("k", &"<redacted>")
            .finish_non_exhaustive()
    }
}

/// Validate a parsed file's secret list: a bounded count, and a PBKDF2
/// iteration count of at least one on every entry.
pub(crate) fn validate_secrets(secrets: &[SecretEntry]) -> Result<(), crate::AccessFileError> {
    if secrets.len() > crate::MAX_SECRETS_PER_FILE {
        return Err(crate::AccessFileError::TooManySecrets(secrets.len()));
    }
    if let Some(entry) = secrets.iter().find(|entry| entry.iterations == 0) {
        return Err(crate::AccessFileError::ZeroIterations {
            label: entry.label.clone(),
        });
    }
    Ok(())
}

/// Read the `version` field alone, before the full parse: a newer file may
/// carry fields this build refuses, and the honest error for it is "wrong
/// version", not "malformed".
pub(crate) fn read_version(bytes: &[u8]) -> Result<u32, crate::AccessFileError> {
    #[derive(Deserialize)]
    struct VersionProbe {
        version: u32,
    }
    serde_json::from_slice::<VersionProbe>(bytes)
        .map(|probe| probe.version)
        .map_err(|error| crate::AccessFileError::Malformed(alloc::format!("{error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;

    #[test]
    fn round_trips_with_base64_binary_fields() {
        let entry = SecretEntry::from_password("camp", Tier::Play, b"s'mores", [1u8; 16], 2);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"label\":\"camp\""), "{json}");
        assert!(json.contains("\"tier\":\"play\""), "{json}");
        assert!(
            json.contains("\"salt\":\"AQEBAQEBAQEBAQEBAQEBAQ==\""),
            "{json}"
        );
        let back: SecretEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn debug_never_prints_the_key() {
        let entry = SecretEntry::from_password("mine", Tier::Edit, b"pw", [0u8; 16], 1);
        let printed = format!("{entry:?}");
        assert!(printed.contains("<redacted>"), "{printed}");
        assert!(!printed.contains(&format!("{:?}", entry.k)), "{printed}");
    }

    #[test]
    fn unknown_fields_are_refused() {
        let json = "{\"label\":\"x\",\"tier\":\"edit\",\"salt\":\"AAAAAAAAAAAAAAAAAAAAAA==\",\
                    \"iterations\":1,\"k\":\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\",\
                    \"password\":\"oops\"}";
        assert!(serde_json::from_str::<SecretEntry>(json).is_err());
    }
}
