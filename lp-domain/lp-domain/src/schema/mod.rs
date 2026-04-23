//! Schema layer: Artifact + Migration trait shapes; empty Registry.

use core::marker::PhantomData;

pub trait Artifact {
    const KIND: &'static str;
    const CURRENT_VERSION: u32;
    // TODO(M5): add `: serde::de::DeserializeOwned` and `: schemars::JsonSchema` bounds
    //          when the migration framework + codegen tooling come online.
}

pub trait Migration {
    const KIND: &'static str;
    const FROM: u32;

    fn migrate(value: &mut toml::Value);
}

#[derive(Default)]
pub struct Registry {
    // TODO(M5): replace with the real registry shape (artifact factories + migration chains).
    _stub: PhantomData<()>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;

    struct DummyArtifact;
    impl Artifact for DummyArtifact {
        const KIND: &'static str = "dummy";
        const CURRENT_VERSION: u32 = 1;
    }

    struct DummyMigration;
    impl Migration for DummyMigration {
        const KIND: &'static str = "dummy";
        const FROM: u32 = 0;
        fn migrate(value: &mut toml::Value) {
            // bump a version field if present
            if let toml::Value::Table(t) = value {
                t.insert("version".into(), toml::Value::Integer(1));
            }
        }
    }

    #[test]
    fn artifact_constants_are_accessible() {
        assert_eq!(DummyArtifact::KIND, "dummy");
        assert_eq!(DummyArtifact::CURRENT_VERSION, 1);
    }

    #[test]
    fn migration_constants_are_accessible() {
        assert_eq!(DummyMigration::KIND, "dummy");
        assert_eq!(DummyMigration::FROM, 0);
    }

    #[test]
    fn kind_version_serde_round_trip() {
        let kind_version = (DummyArtifact::KIND, DummyArtifact::CURRENT_VERSION);
        let json = serde_json::to_string(&kind_version).expect("serialize");
        let back: (String, u32) = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            (back.0.as_str(), back.1),
            (DummyArtifact::KIND, kind_version.1)
        );
    }

    #[test]
    fn migration_runs_against_toml_value() {
        let mut value = toml::Value::Table(toml::value::Table::new());
        DummyMigration::migrate(&mut value);
        match value {
            toml::Value::Table(t) => assert_eq!(t.get("version").unwrap().as_integer(), Some(1)),
            _ => panic!("expected table"),
        }
    }

    #[test]
    fn registry_is_constructible() {
        let _: Registry = Registry::new();
    }
}
