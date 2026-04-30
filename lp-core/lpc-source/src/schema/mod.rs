//! **Schema versioning and migrations** for authored artifacts.

pub mod migration {
    //! One step in a `FROM` → `FROM+1` migration chain on raw [`toml::Value`].

    /// One **migrator** in a `FROM` → `FROM+1` chain on raw [`toml::Value`].
    pub trait Migration {
        /// Must match the [`crate::artifact::SrcArtifact::KIND`] this migration applies to.
        const KIND: &'static str;
        /// Source schema version this function can upgrade from.
        const FROM: u32;

        /// Rewrite `value` in place to the next version’s shape.
        fn migrate(value: &mut toml::Value);
    }
}

pub mod registry {
    //! Global migration / artifact-factory table placeholder (M5).

    use core::marker::PhantomData;

    /// Placeholder for the **global** migration and artifact-factory table (M5).
    #[derive(Default)]
    pub struct Registry {
        // TODO(M5): replace with the real registry shape (artifact factories + migration chains).
        _stub: PhantomData<()>,
    }

    impl Registry {
        /// Creates an empty placeholder registry.
        pub fn new() -> Self {
            Self::default()
        }
    }
}

pub use migration::Migration;
pub use registry::Registry;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifact::SrcArtifact;
    use alloc::string::String;

    struct DummyArtifact;
    impl SrcArtifact for DummyArtifact {
        const KIND: &'static str = "dummy";
        const CURRENT_VERSION: u32 = 1;

        fn schema_version(&self) -> u32 {
            Self::CURRENT_VERSION
        }
    }

    struct DummyMigration;
    impl Migration for DummyMigration {
        const KIND: &'static str = "dummy";
        const FROM: u32 = 0;
        fn migrate(value: &mut toml::Value) {
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
