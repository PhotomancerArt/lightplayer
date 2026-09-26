//! One installed secret: a labelled, tiered login key.

use alloc::string::String;
use core::fmt;
use serde::{Deserialize, Serialize};

use crate::secret_kind::SecretKind;
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
/// `kind` and `added_at` are for people too ("Who has access"); the board
/// grants by `tier` alone.
///
/// This is the version-2 entry shape. A version-1 file's entries carry no
/// `kind` and no `addedAt`; they are read through a private v1 shape and
/// become `kind: password` with no time.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SecretEntry {
    /// Human name for this secret.
    pub label: String,
    /// Who holds it: a browser, an account, or a password.
    pub kind: SecretKind,
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
    /// When it was added, epoch seconds, as the adding client said
    /// (caller-supplied: nothing here reads a clock). Absent when unknown,
    /// which every version-1 entry is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub added_at: Option<u64>,
}

impl SecretEntry {
    /// Build a `password` entry from a password, deriving `k` on the spot.
    /// Client side only: the board never derives. No `added_at`; see
    /// [`Self::with_kind`] and [`Self::with_added_at`].
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
            kind: SecretKind::Password,
            tier,
            salt,
            iterations,
            k: crate::pbkdf2_sha256::derive_login_key(password, &salt, iterations),
            added_at: None,
        }
    }

    /// The same entry, held by `kind`.
    #[must_use]
    pub fn with_kind(mut self, kind: SecretKind) -> Self {
        self.kind = kind;
        self
    }

    /// The same entry, added at `epoch_seconds`.
    #[must_use]
    pub fn with_added_at(mut self, epoch_seconds: u64) -> Self {
        self.added_at = Some(epoch_seconds);
        self
    }
}

/// `k` is login-equivalent: a Debug print (a log line, a panic message)
/// must never carry it.
impl fmt::Debug for SecretEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SecretEntry")
            .field("label", &self.label)
            .field("kind", &self.kind)
            .field("tier", &self.tier)
            .field("iterations", &self.iterations)
            .field("k", &"<redacted>")
            .field("added_at", &self.added_at)
            .finish_non_exhaustive()
    }
}

/// The version-1 entry shape, kept only to read version-1 files: no `kind`,
/// no `addedAt`, every other field spelled exactly as v2 spells it. It
/// keeps `deny_unknown_fields`, so a v1 file carrying a v2 field is refused
/// rather than half-read.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct SecretEntryV1 {
    label: String,
    tier: Tier,
    #[serde(with = "crate::base64_bytes")]
    salt: [u8; SALT_BYTES],
    iterations: u32,
    #[serde(with = "crate::base64_bytes")]
    k: [u8; KEY_BYTES],
}

impl From<SecretEntryV1> for SecretEntry {
    /// Before v2 a password was the only kind of secret there was, and no
    /// entry recorded when it was added.
    fn from(v1: SecretEntryV1) -> Self {
        Self {
            label: v1.label,
            kind: SecretKind::Password,
            tier: v1.tier,
            salt: v1.salt,
            iterations: v1.iterations,
            k: v1.k,
            added_at: None,
        }
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
        assert!(json.contains("\"kind\":\"password\""), "{json}");
        assert!(json.contains("\"tier\":\"play\""), "{json}");
        assert!(
            json.contains("\"salt\":\"AQEBAQEBAQEBAQEBAQEBAQ==\""),
            "{json}"
        );
        assert!(
            !json.contains("addedAt"),
            "an absent time is omitted: {json}"
        );
        let back: SecretEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn kind_and_added_at_round_trip() {
        let entry = SecretEntry::from_password("Yona's MacBook", Tier::Edit, b"k", [2u8; 16], 1)
            .with_kind(SecretKind::Browser)
            .with_added_at(1_790_000_000);
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"kind\":\"browser\""), "{json}");
        assert!(json.contains("\"addedAt\":1790000000"), "{json}");
        let back: SecretEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(back, entry);
    }

    #[test]
    fn a_v2_entry_needs_its_kind_and_a_v1_entry_reads_as_a_password() {
        let json = "{\"label\":\"x\",\"tier\":\"edit\",\"salt\":\"AAAAAAAAAAAAAAAAAAAAAA==\",\
                    \"iterations\":1,\"k\":\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\"}";
        assert!(serde_json::from_str::<SecretEntry>(json).is_err());
        let entry = SecretEntry::from(serde_json::from_str::<SecretEntryV1>(json).unwrap());
        assert_eq!(entry.label, "x");
        assert_eq!(entry.kind, SecretKind::Password);
        assert_eq!(entry.added_at, None);
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
        let json = "{\"label\":\"x\",\"kind\":\"password\",\"tier\":\"edit\",\
                    \"salt\":\"AAAAAAAAAAAAAAAAAAAAAA==\",\"iterations\":1,\
                    \"k\":\"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=\",\
                    \"password\":\"oops\"}";
        assert!(serde_json::from_str::<SecretEntry>(json).is_err());
    }
}
