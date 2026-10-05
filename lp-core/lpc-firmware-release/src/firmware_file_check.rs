//! Which files a release holds for a target, and checking bytes against them.

use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;

use crate::lower_hex::sha256_hex;
use crate::ota_manifest::OtaManifest;

/// One file the manifest names: its name in the lookup, length and SHA-256.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FirmwareFileRef<'a> {
    /// The name inside the lookup and after `<target>.` in the asset name.
    pub file: &'a str,
    /// Length in bytes.
    pub length: u64,
    /// SHA-256, lowercase hex.
    pub sha256: &'a str,
}

/// Why bytes do not match the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FirmwareFileError {
    /// The manifest does not name this file.
    UnknownFile(String),
    /// The bytes are not the length the manifest says.
    LengthMismatch {
        /// The file.
        file: String,
        /// The manifest's length.
        expected: u64,
        /// The bytes' length.
        actual: u64,
    },
    /// The bytes do not hash to the manifest's SHA-256.
    HashMismatch {
        /// The file.
        file: String,
        /// The manifest's SHA-256.
        expected: String,
        /// The bytes' SHA-256.
        actual: String,
    },
}

impl fmt::Display for FirmwareFileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownFile(file) => write!(f, "{file}: not named by the manifest"),
            Self::LengthMismatch {
                file,
                expected,
                actual,
            } => write!(f, "{file}: {actual} bytes, the manifest says {expected}"),
            Self::HashMismatch {
                file,
                expected,
                actual,
            } => write!(f, "{file}: sha256 {actual}, the manifest says {expected}"),
        }
    }
}

impl core::error::Error for FirmwareFileError {}

impl OtaManifest {
    /// Every file this manifest names — the allowlist of a lookup besides
    /// `ota-manifest.json` itself: `core`, `engine`, every **known**
    /// encoding's files (encoding 1's two `.z` files), the package manifest
    /// and its merged image. An unknown encoding's files are not listed: a
    /// reader cannot know what they are.
    pub fn files(&self) -> Vec<FirmwareFileRef<'_>> {
        let mut out = Vec::with_capacity(6);
        out.push(piece_ref(
            &self.core.file,
            self.core.length,
            &self.core.sha256,
        ));
        out.push(piece_ref(
            &self.engine.file,
            self.engine.length,
            &self.engine.sha256,
        ));
        if let Some(e) = self.encoding1() {
            out.push(piece_ref(&e.core.file, e.core.length, &e.core.sha256));
            out.push(piece_ref(&e.engine.file, e.engine.length, &e.engine.sha256));
        }
        out.push(piece_ref(
            &self.package.file,
            self.package.length,
            &self.package.sha256,
        ));
        out.push(piece_ref(
            &self.package.image.file,
            self.package.image.length,
            &self.package.image.sha256,
        ));
        out
    }

    /// The named file, if the manifest names it.
    pub fn file(&self, name: &str) -> Option<FirmwareFileRef<'_>> {
        self.files().into_iter().find(|f| f.file == name)
    }

    /// Check `bytes` against the manifest's entry for `file`: length first,
    /// then SHA-256. The error names the file.
    pub fn verify(&self, file: &str, bytes: &[u8]) -> Result<(), FirmwareFileError> {
        let entry = self
            .file(file)
            .ok_or_else(|| FirmwareFileError::UnknownFile(String::from(file)))?;
        let actual_len = bytes.len() as u64;
        if actual_len != entry.length {
            return Err(FirmwareFileError::LengthMismatch {
                file: String::from(file),
                expected: entry.length,
                actual: actual_len,
            });
        }
        let actual = sha256_hex(bytes);
        if actual != entry.sha256 {
            return Err(FirmwareFileError::HashMismatch {
                file: String::from(file),
                expected: String::from(entry.sha256),
                actual,
            });
        }
        Ok(())
    }
}

fn piece_ref<'a>(file: &'a str, length: u64, sha256: &'a str) -> FirmwareFileRef<'a> {
    FirmwareFileRef {
        file,
        length,
        sha256,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ota_manifest::tests::sample;
    use alloc::vec;

    #[test]
    fn files_lists_every_known_file() {
        let m = sample();
        let names: Vec<&str> = m.files().iter().map(|f| f.file).collect();
        assert_eq!(
            names,
            vec![
                "core.bin",
                "engine.bin",
                "core.z",
                "engine.z",
                "package.json",
                "fw-esp32c6-merged.bin"
            ]
        );
        let mut raw_only = sample();
        raw_only.encodings.clear();
        assert_eq!(raw_only.files().len(), 4);
    }

    #[test]
    fn verify_checks_length_then_hash() {
        let bytes = b"engine bytes".to_vec();
        let mut m = sample();
        m.engine.length = bytes.len() as u64;
        m.engine.sha256 = sha256_hex(&bytes);
        m.verify("engine.bin", &bytes).unwrap();

        assert_eq!(
            m.verify("engine.bin", b"short"),
            Err(FirmwareFileError::LengthMismatch {
                file: "engine.bin".into(),
                expected: bytes.len() as u64,
                actual: 5,
            })
        );
        let mut tampered = bytes.clone();
        tampered[0] ^= 1;
        assert!(matches!(
            m.verify("engine.bin", &tampered),
            Err(FirmwareFileError::HashMismatch { file, .. }) if file == "engine.bin"
        ));
        assert_eq!(
            m.verify("secrets.txt", &bytes),
            Err(FirmwareFileError::UnknownFile("secrets.txt".into()))
        );
    }
}
