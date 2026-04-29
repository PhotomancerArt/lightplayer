//! **Schema versioning and migrations**: traits that typed **artifacts** will
//! implement, plus a placeholder [`Registry`] for the migration framework in M5
//! (`docs/roadmaps/2026-04-22-lp-domain/m2-domain-skeleton.md` — `Artifact`,
//! `Migration`, empty registry; M5 fills behavior per `overview.md` milestone table).

use core::marker::PhantomData;

use crate::shape::Slot;

/// Metadata for a **versioned, on-disk** LightPlayer artifact: pattern, effect,
/// transition, stack, live, or playlist, each with its own `KIND` string and
/// schema `CURRENT_VERSION` (`docs/roadmaps/2026-04-22-lp-domain/overview.md`
/// — crate layout and `schema_version` story).
///
/// Typed deserialize, JSON Schema bounds, and migration wiring come in
/// M5+ (`// TODO` on this trait).
pub trait Artifact {
    /// TOML/JSON `kind` discriminator and file extension family (e.g. `"pattern"`).
    const KIND: &'static str;
    /// Breaking-schema bump only; see `single schema_version` in `overview.md` compatibility model.
    const CURRENT_VERSION: u32;

    /// On-disk `schema_version` field after load (validated against [`CURRENT_VERSION`](Self::CURRENT_VERSION) by the loader).
    fn schema_version(&self) -> u32;

    /// Visit every top-level [`Slot`] this artifact owns for load-time default materialization.
    ///
    /// Visuals with a `[params]` table walk the inner params-table root [`Slot`];
    /// nested fields are reached via [`Slot::default_value`](crate::shape::Slot::default_value).
    fn walk_slots<F: FnMut(&Slot)>(&self, _f: F) {}
}

/// One **migrator** in a `FROM` → `FROM+1` chain on raw [`toml::Value`]
/// (hybrid model in `docs/roadmaps/2026-04-22-lp-domain/overview.md` — data flow on load).
pub trait Migration {
    /// Must match the [`Artifact::KIND`] this migration applies to.
    const KIND: &'static str;
    /// Source schema version this function can upgrade from.
    const FROM: u32;

    /// Rewrite `value` in place to the next version’s shape. Chains run until
    /// the document reaches `Artifact::CURRENT_VERSION`, then a typed
    /// `Deserialize` runs (`overview.md`).
    fn migrate(value: &mut toml::Value);
}

/// Placeholder for the **global** migration and artifact-factory table (M5
/// per `m2-domain-skeleton.md` and `schema/mod` `TODO` here). M2 only needs a
/// constructible type for tests and forward references.
#[derive(Default)]
pub struct Registry {
    // TODO(M5): replace with the real registry shape (artifact factories + migration chains).
    _stub: PhantomData<()>,
}

impl Registry {
    /// Creates an empty placeholder registry. Real registration APIs land with M5.
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

        fn schema_version(&self) -> u32 {
            Self::CURRENT_VERSION
        }
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
