//! Catalog tree gates, file-only (no Studio): the directory an entry lives
//! in agrees with its manifest, licensing is stated where the export lint
//! and `catalog/COPYING.md` expect it, and every entry carries the blurb
//! the gallery card shows.
//!
//! The manifest is the truth and the bucket is for authors; these tests
//! are what keeps the two from drifting (`docs/adr/*catalog-content-tree.md`).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use lpc_model::{NodeDef, ProjectKind, ProjectManifest, SlotShapeRegistry};

/// Bucket directories under `catalog/` and the manifest kind each one
/// requires. `templates` is reserved: the vocabulary has no `template`
/// kind yet, so the walk refuses the directory until it does — adding the
/// kind is one arm here.
const BUCKET_KINDS: &[(&str, &str)] = &[("projects", "general"), ("patterns", "pattern")];

/// The catalog's default license: an entry stating it (or stating none)
/// needs no manifest row.
const DEFAULT_LICENSE: &str = "CC0-1.0";

struct Entry {
    bucket: String,
    slug: String,
    dir: PathBuf,
    manifest: ProjectManifest,
}

#[test]
fn every_bucket_agrees_with_its_manifest_kind() -> Result<()> {
    let mut failures = Vec::new();
    for entry in entries()? {
        let expected = BUCKET_KINDS
            .iter()
            .find(|(bucket, _)| *bucket == entry.bucket)
            .map(|(_, kind)| *kind)
            .unwrap_or_else(|| {
                panic!(
                    "catalog/{}/ is not a known bucket ({:?}); if this is `templates`, add the \
                     `template` kind to the manifest vocabulary first",
                    entry.bucket,
                    BUCKET_KINDS.iter().map(|(b, _)| *b).collect::<Vec<_>>()
                )
            });
        let actual = match entry.manifest.project_kind() {
            ProjectKind::General => "general",
            ProjectKind::Pattern { .. } => "pattern",
            ProjectKind::Show => "show",
            ProjectKind::Rig { .. } => "rig",
        };
        if actual != expected {
            failures.push(format!(
                "catalog/{}/{}: bucket wants kind `{expected}`, project.json says `{actual}`",
                entry.bucket, entry.slug
            ));
        }
        if let ProjectKind::Pattern { exports } = entry.manifest.project_kind() {
            for export in exports {
                if !entry.dir.join(&export).join("module.json").is_file() {
                    failures.push(format!(
                        "catalog/{}/{}: exports `{export}` but has no {export}/module.json",
                        entry.bucket, entry.slug
                    ));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// `catalog/COPYING.md` both ways: every entry whose carrying module states
/// a license other than CC0 has a row naming its slug, the SPDX tag and a
/// source URL; every row names an entry that exists. A module that states
/// no license is CC0 by the catalog's default (`catalog/README.md`:
/// "CC0 unless a project's provenance says otherwise") and needs no row —
/// the export lint, not this gate, is what nudges patterns to state one.
#[test]
fn copying_manifest_matches_the_tree_both_ways() -> Result<()> {
    let copying = std::fs::read_to_string(catalog_root().join("COPYING.md"))
        .context("read catalog/COPYING.md")?;
    let entries = entries()?;

    let mut failures = Vec::new();
    for entry in &entries {
        for (module_rel, license) in carrying_module_licenses(entry)? {
            let Some(license) = license else {
                continue;
            };
            if license == DEFAULT_LICENSE {
                continue;
            }
            let needle = format!("`{}`", entry.slug);
            let Some(row) = copying.lines().find(|line| line.contains(&needle)) else {
                failures.push(format!(
                    "catalog/{}/{}: {module_rel} is {license} but COPYING.md has no row for `{}`",
                    entry.bucket, entry.slug, entry.slug
                ));
                continue;
            };
            if !row.contains(&license) {
                failures.push(format!(
                    "COPYING.md row for `{}` does not name license {license}:\n{row}",
                    entry.slug
                ));
            }
            if !row.contains("http") {
                failures.push(format!(
                    "COPYING.md row for `{}` names no source URL:\n{row}",
                    entry.slug
                ));
            }
        }
    }

    let slugs: Vec<&str> = entries.iter().map(|entry| entry.slug.as_str()).collect();
    for line in copying.lines() {
        if !line.starts_with("| `") {
            continue;
        }
        let slug = line
            .trim_start_matches("| `")
            .split('`')
            .next()
            .expect("row starts with | `slug`");
        if !slugs.contains(&slug) {
            failures.push(format!(
                "COPYING.md lists `{slug}` but no catalog entry has that slug"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// An entry's blurb (`project.json` `description`), when it authors one,
/// is one sentence's worth — never over the length a card or picker
/// line would clamp at. Optional: the G1 ruling (2026-09-06) keeps the
/// copy off the card face, so an entry without one is complete.
#[test]
fn every_description_is_one_line() -> Result<()> {
    const MAX_CHARS: usize = 160;
    let mut failures = Vec::new();
    for entry in entries()? {
        match entry.manifest.description.as_deref().map(str::trim) {
            None | Some("") => {}
            Some(description) if description.chars().count() > MAX_CHARS => failures.push(format!(
                "catalog/{}/{}: description is {} chars, over the {MAX_CHARS} the card clamps at",
                entry.bucket,
                entry.slug,
                description.chars().count()
            )),
            Some(_) => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    Ok(())
}

/// The module whose `provenance` speaks for the entry: each exported
/// module for a pattern, the root module for everything else. Returns
/// `(project-relative path, license)` per carrying module.
fn carrying_module_licenses(entry: &Entry) -> Result<Vec<(String, Option<String>)>> {
    let registry = SlotShapeRegistry::default();
    let module_paths: Vec<String> = match entry.manifest.project_kind() {
        ProjectKind::Pattern { exports } | ProjectKind::Rig { exports } => exports
            .iter()
            .map(|export| format!("{export}/module.json"))
            .collect(),
        ProjectKind::General | ProjectKind::Show => vec!["module.json".to_string()],
    };
    let mut out = Vec::new();
    for rel in module_paths {
        let text = std::fs::read_to_string(entry.dir.join(&rel))
            .with_context(|| format!("catalog/{}/{}: read {rel}", entry.bucket, entry.slug))?;
        let def = NodeDef::read_json(&registry, &text).map_err(|err| {
            anyhow::anyhow!(
                "catalog/{}/{}: parse {rel}: {err}",
                entry.bucket,
                entry.slug
            )
        })?;
        let module = def.as_module().with_context(|| {
            format!(
                "catalog/{}/{}: {rel} is not a module",
                entry.bucket, entry.slug
            )
        })?;
        let license = module
            .provenance
            .data
            .as_ref()
            .and_then(|provenance| provenance.license.data.as_ref())
            .map(|slot| slot.value().trim().to_string())
            .filter(|license| !license.is_empty());
        out.push((rel, license));
    }
    Ok(out)
}

fn entries() -> Result<Vec<Entry>> {
    let root = catalog_root();
    let mut entries = Vec::new();
    for bucket in std::fs::read_dir(&root).context("read catalog/")? {
        let bucket = bucket?.path();
        if !bucket.is_dir() {
            continue;
        }
        let bucket_name = bucket.file_name().unwrap().to_string_lossy().into_owned();
        for dir in std::fs::read_dir(&bucket)? {
            let dir = dir?.path();
            if !dir.is_dir() {
                continue;
            }
            let slug = dir.file_name().unwrap().to_string_lossy().into_owned();
            let manifest_path = dir.join("project.json");
            let text = std::fs::read_to_string(&manifest_path).with_context(|| {
                format!("catalog/{bucket_name}/{slug}: every entry directory needs a project.json")
            })?;
            let manifest = ProjectManifest::read_json(&text).map_err(|err| {
                anyhow::anyhow!("catalog/{bucket_name}/{slug}/project.json: {err}")
            })?;
            entries.push(Entry {
                bucket: bucket_name.clone(),
                slug,
                dir,
                manifest,
            });
        }
    }
    entries.sort_by(|a, b| (&a.bucket, &a.slug).cmp(&(&b.bucket, &b.slug)));
    assert!(
        entries.len() >= 16,
        "the catalog walk is vacuous: {} entries",
        entries.len()
    );
    Ok(entries)
}

fn catalog_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace dir")
        .join("catalog")
}
