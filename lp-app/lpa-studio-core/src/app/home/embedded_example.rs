//! The catalog registry: generated file tables (`build.rs`) typed at
//! runtime through the manifest parser.
//!
//! Catalog entries are first-party published projects (examples vision D1)
//! with bare-slug addresses (`/p/<slug>`, the id tail). Opening one is
//! STATELESS (D2): a transient memory-backed session, nothing installed —
//! an explicit save forks a copy with `SeededFrom { source: id }`
//! provenance (the "Remixed from" line). The home landing, Explore and the
//! device card's project picker all list this table.
//!
//! The build script walks `catalog/<bucket>/<slug>/` (buckets `projects`
//! and `patterns`; `templates` reserved) and emits one `include_bytes!`
//! table per entry, so the wasm bundle carries the files and the
//! checked-in entry IS what the gallery opens. Nothing here names an
//! entry: `embedded_examples()` types the generated rows once, on first
//! use, by parsing each entry's `project.json` with
//! [`lpc_model::ProjectManifest::read_json`] — the one real parser, so
//! `name`/`kind`/`description` can never drift from what the tree gates
//! check. Adding content is adding a folder.
//!
//! Ids are bucket-free — `catalog/<slug>` — so reclassifying an entry
//! never changes what a library's "Remixed from" line points at; the
//! pre-catalog spelling `examples/<slug>` is still accepted on lookup
//! (see [`embedded_example`]). Slug uniqueness is test-pinned — the id
//! tail is the URL.

use std::sync::OnceLock;

use lpc_model::{ProjectKind, ProjectManifest};

/// The generated file tables — see `build.rs` for the shape and the
/// ordering rules.
mod generated {
    include!(concat!(env!("OUT_DIR"), "/catalog_files.generated.rs"));
}

/// One file in an embedded package: its package-relative path and bytes.
pub type ExampleFile = (&'static str, &'static [u8]);

/// The id prefix every catalog entry carries (`catalog/<slug>`).
pub const CATALOG_ID_PREFIX: &str = "catalog/";

/// The pre-catalog id prefix (`examples/<slug>`), still persisted in
/// user libraries as `SeededFrom { source }` provenance and in the cloud
/// store. Accepted on lookup so those "Remixed from" lines keep resolving;
/// never written anew.
pub const LEGACY_EXAMPLE_ID_PREFIX: &str = "examples/";

/// A catalog bucket: the directory under `catalog/` an entry lives in.
/// Buckets mirror the manifest `kind` (a test keeps them agreeing); the
/// manifest is the truth, the bucket is for authors. Declaration order is
/// display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CatalogBucket {
    /// Real pieces (`kind` absent → `General`).
    Projects,
    /// Single-effect pattern projects (`kind: pattern`, exporting `effect/`).
    Patterns,
    /// Reserved for `kind: template` (follow-up plan); no directory today.
    Templates,
}

impl CatalogBucket {
    /// Every bucket, in display order.
    pub const ALL: &[CatalogBucket] = &[Self::Projects, Self::Patterns, Self::Templates];

    /// The directory name under `catalog/`.
    pub fn dir_name(self) -> &'static str {
        match self {
            Self::Projects => "projects",
            Self::Patterns => "patterns",
            Self::Templates => "templates",
        }
    }

    fn from_dir_name(name: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|b| b.dir_name() == name)
    }
}

/// One catalog entry, typed from its manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmbeddedExample {
    /// Stable, bucket-free id: `catalog/<slug>`.
    pub id: &'static str,
    /// The directory name under the bucket — the `/p/<slug>` address.
    pub slug: &'static str,
    pub bucket: CatalogBucket,
    /// `project.json`'s `name`; the slug when the manifest names nothing.
    pub name: &'static str,
    /// `project.json`'s authored kind (`General` when absent).
    pub kind: &'static ProjectKind,
    /// `project.json`'s card blurb; empty when absent.
    pub description: &'static str,
    /// The package's files, in deploy order (`project.json` first,
    /// `module.json` second, the rest sorted by path).
    pub files: &'static [ExampleFile],
}

impl EmbeddedExample {
    /// The example's package files as owned (relative path, bytes) pairs.
    pub fn files(&self) -> Vec<(String, Vec<u8>)> {
        self.files
            .iter()
            .map(|(path, bytes)| ((*path).to_string(), bytes.to_vec()))
            .collect()
    }

    /// The example's canonical bare slug — the id tail
    /// (`catalog/fyeah-sign` → `fyeah-sign`). This is the `/p/<slug>`
    /// address (PD3): entry directories are `[a-z0-9-]` names, so the
    /// tail is URL-ready as-is. Uniqueness across the table is pinned by
    /// a test below.
    pub fn slug(&self) -> &'static str {
        self.slug
    }

    /// One package file's bytes by package-relative path.
    pub fn file(&self, path: &str) -> Option<&'static [u8]> {
        self.files
            .iter()
            .find(|(file, _)| *file == path)
            .map(|(_, bytes)| *bytes)
    }
}

/// All catalog entries, in display order: bucket rank
/// ([`CatalogBucket::ALL`]), then slug.
pub fn embedded_examples() -> &'static [EmbeddedExample] {
    static REGISTRY: OnceLock<Vec<EmbeddedExample>> = OnceLock::new();
    REGISTRY.get_or_init(build_registry).as_slice()
}

/// Look up an embedded example by id. The legacy `examples/<slug>`
/// spelling resolves to the same entry as `catalog/<slug>`.
pub fn embedded_example(id: &str) -> Option<EmbeddedExample> {
    let id = canonical_example_id(id);
    embedded_examples()
        .iter()
        .copied()
        .find(|example| example.id == id)
}

/// Look up an embedded example by its bare slug (the id tail) — the
/// `/p/<slug>` resolution leg. An unknown slug is `None`, never a guess.
pub fn embedded_example_by_slug(slug: &str) -> Option<EmbeddedExample> {
    embedded_examples()
        .iter()
        .copied()
        .find(|example| example.slug == slug)
}

/// Rewrite the legacy `examples/` prefix to `catalog/`; every other id is
/// returned unchanged.
pub fn canonical_example_id(id: &str) -> std::borrow::Cow<'_, str> {
    match id.strip_prefix(LEGACY_EXAMPLE_ID_PREFIX) {
        Some(slug) => std::borrow::Cow::Owned(format!("{CATALOG_ID_PREFIX}{slug}")),
        None => std::borrow::Cow::Borrowed(id),
    }
}

/// Type the generated rows through the manifest parser, once per process.
/// The handful of owned strings are leaked so the entry stays `Copy` and
/// `'static` for its ~40 consumers: sixteen entries, once.
///
/// A manifest that fails to parse is a panic naming the entry — the
/// `lp-cli` tree gates guarantee it cannot happen on a green tree.
fn build_registry() -> Vec<EmbeddedExample> {
    generated::CATALOG_FILES
        .iter()
        .map(|(bucket, slug, files)| {
            let bucket = CatalogBucket::from_dir_name(bucket)
                .unwrap_or_else(|| panic!("catalog/{bucket}/{slug}: unknown bucket"));
            let manifest_bytes = files
                .iter()
                .find(|(path, _)| *path == "project.json")
                .map(|(_, bytes)| *bytes)
                .unwrap_or_else(|| panic!("catalog/{}/{slug}: no project.json", bucket.dir_name()));
            let manifest_text = std::str::from_utf8(manifest_bytes).unwrap_or_else(|e| {
                panic!("catalog/{}/{slug}: project.json: {e}", bucket.dir_name())
            });
            let manifest = ProjectManifest::read_json(manifest_text).unwrap_or_else(|e| {
                panic!("catalog/{}/{slug}: project.json: {e}", bucket.dir_name())
            });
            EmbeddedExample {
                id: leak(format!("{CATALOG_ID_PREFIX}{slug}")),
                slug,
                bucket,
                name: manifest.name.clone().map_or(slug, leak),
                kind: Box::leak(Box::new(manifest.project_kind())),
                description: manifest.description.clone().map_or("", leak),
                files,
            }
        })
        .collect()
}

fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::project::demo_project::DEMO_PROJECT_ID;
    use std::path::{Path, PathBuf};

    #[test]
    fn demo_example_is_embedded_with_files() {
        let example = embedded_example(DEMO_PROJECT_ID).expect("demo example is embedded");
        assert_eq!(example.name, "Fyeah Sign");
        assert_eq!(example.bucket, CatalogBucket::Projects);
        assert_eq!(*example.kind, ProjectKind::General);
        let files = example.files();
        assert!(
            files
                .iter()
                .any(|(path, _)| path == "project.json" && !files.is_empty())
        );
    }

    #[test]
    fn unknown_example_is_none() {
        assert!(embedded_example("catalog/unknown").is_none());
        assert!(embedded_example("examples/unknown").is_none());
    }

    /// Libraries seeded before the catalog move carry
    /// `SeededFrom { source: "examples/<slug>" }`; that spelling must keep
    /// resolving to the entry now addressed as `catalog/<slug>`.
    #[test]
    fn legacy_example_ids_resolve_to_catalog_entries() {
        let legacy = embedded_example("examples/fyeah-sign").expect("legacy id resolves");
        assert_eq!(legacy.id, "catalog/fyeah-sign");
        assert_eq!(canonical_example_id("examples/plasma"), "catalog/plasma");
        assert_eq!(canonical_example_id("catalog/plasma"), "catalog/plasma");
        assert_eq!(canonical_example_id("prj123"), "prj123");
    }

    /// The north star: every directory under a catalog bucket with a
    /// `project.json` is in the table, and every table row is such a
    /// directory — no Rust edit registers content.
    #[test]
    fn every_catalog_directory_is_registered_and_vice_versa() {
        let mut on_disk: Vec<(CatalogBucket, String)> = Vec::new();
        for bucket in CatalogBucket::ALL {
            let dir = catalog_root().join(bucket.dir_name());
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries {
                let path = entry.expect("dir entry").path();
                if path.is_dir() && path.join("project.json").is_file() {
                    let slug = path.file_name().unwrap().to_string_lossy().into_owned();
                    on_disk.push((*bucket, slug));
                }
            }
        }
        on_disk.sort();
        let mut registered: Vec<(CatalogBucket, String)> = embedded_examples()
            .iter()
            .map(|e| (e.bucket, e.slug.to_string()))
            .collect();
        registered.sort();
        assert_eq!(registered, on_disk);
        assert!(
            on_disk.len() >= 16,
            "the catalog walk is vacuous: {on_disk:?}"
        );
    }

    /// Display order is bucket rank, then slug — the home page's order.
    #[test]
    fn registry_order_is_bucket_rank_then_slug() {
        let keys: Vec<(CatalogBucket, &str)> = embedded_examples()
            .iter()
            .map(|e| (e.bucket, e.slug))
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    /// `name` comes from the manifest, never from a hand table.
    #[test]
    fn names_match_the_manifests() {
        for example in embedded_examples() {
            let text = std::fs::read_to_string(
                catalog_root()
                    .join(example.bucket.dir_name())
                    .join(example.slug)
                    .join("project.json"),
            )
            .expect("read project.json");
            let manifest = ProjectManifest::read_json(&text).expect("manifest parses");
            assert_eq!(
                manifest.name.as_deref().unwrap_or(example.slug),
                example.name
            );
            assert_eq!(manifest.project_kind(), *example.kind);
            assert_eq!(
                manifest.description.as_deref().unwrap_or(""),
                example.description
            );
        }
    }

    /// Bare slugs are the `/p/<slug>` grammar (PD3): every id tail must be
    /// unique or two examples would share an address.
    #[test]
    fn example_slugs_are_unique_id_tails() {
        let mut seen = std::collections::BTreeSet::new();
        for example in embedded_examples() {
            assert_eq!(example.id, format!("catalog/{}", example.slug()));
            assert!(
                seen.insert(example.slug()),
                "duplicate example slug: {}",
                example.slug()
            );
        }
        assert_eq!(
            embedded_example_by_slug("fyeah-sign").map(|e| e.id),
            Some("catalog/fyeah-sign")
        );
        assert!(embedded_example_by_slug("unknown").is_none());
    }

    /// The docs-example contract: plasma-duo drives the SAME shader (and
    /// clock) bytes as the gallery's plasma, so the "What's a shader?"
    /// page's edit-me listing and the standalone example never drift
    /// apart. A plasma shader tweak not copied over breaks this loudly.
    #[test]
    fn plasma_duo_shares_plasmas_shader_bytes() {
        let plasma = embedded_example("catalog/plasma").expect("plasma is embedded");
        let duo = embedded_example("catalog/plasma-duo").expect("plasma-duo is embedded");
        let plasma_files: std::collections::BTreeMap<_, _> = plasma.files().into_iter().collect();
        let duo_files: std::collections::BTreeMap<_, _> = duo.files().into_iter().collect();
        for shared in ["shader.glsl", "shader.json", "clock.json"] {
            assert_eq!(
                plasma_files[&shared.to_string()],
                duo_files[&shared.to_string()],
                "{shared} must stay byte-identical between plasma and plasma-duo"
            );
        }
    }

    /// The mapping-and-patching claim, pinned: the peach's patch documents
    /// say where the lamps land on the wire, which is a fact about the
    /// installation and not about how anything samples them. So the 1D and
    /// 2D peaches — same artwork, opposite declarations — carry the SAME
    /// patch bytes, and the mapping documents they patch against too. A
    /// change to one that is not copied to the other breaks this loudly.
    #[test]
    fn the_two_peaches_share_their_patch_and_mapping_bytes() {
        let one_d = embedded_example("catalog/peach-1d").expect("peach-1d is embedded");
        let two_d = embedded_example("catalog/peach-2d").expect("peach-2d is embedded");
        let one_d_files: std::collections::BTreeMap<_, _> = one_d.files().into_iter().collect();
        let two_d_files: std::collections::BTreeMap<_, _> = two_d.files().into_iter().collect();
        for shared in [
            "body/peach_body.patch.json",
            "leaf/peach_leaf.patch.json",
            "body/peach_body.map2d.json",
            "leaf/peach_leaf.map2d.json",
        ] {
            assert_eq!(
                one_d_files[&shared.to_string()],
                two_d_files[&shared.to_string()],
                "{shared} must stay byte-identical between peach-1d and peach-2d"
            );
        }
    }

    #[test]
    fn every_example_ships_the_two_container_files() {
        // Mitosis (modules.md §1/§6): a package is unopenable without BOTH
        // the container manifest and the root module. Found the hard way
        // when a fixture's mapping document was left out of the demo list.
        for example in embedded_examples() {
            let files = example.files();
            for required in ["project.json", "module.json"] {
                assert!(
                    files.iter().any(|(path, _)| path == required),
                    "{} must ship {required}",
                    example.id
                );
            }
            assert_eq!(
                files.first().map(|(path, _)| path.as_str()),
                Some("project.json"),
                "{} deploys the container manifest first",
                example.id
            );
            assert_eq!(
                files.get(1).map(|(path, _)| path.as_str()),
                Some("module.json"),
                "{} deploys the root module second",
                example.id
            );
        }
    }

    #[test]
    fn example_ids_and_names_are_unique() {
        let mut ids: Vec<&str> = embedded_examples().iter().map(|it| it.id).collect();
        let count = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), count, "example ids collide");
    }

    fn catalog_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../catalog")
    }
}
