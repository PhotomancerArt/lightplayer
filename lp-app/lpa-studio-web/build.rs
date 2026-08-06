use std::collections::{BTreeMap, HashMap};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use syn::{Attribute, Expr, Item, Lit, Meta};

fn main() {
    println!("cargo:rerun-if-changed=src");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    emit_git_facts(&manifest_dir);
    ensure_tailwind_placeholder(&manifest_dir);
    let src_dir = manifest_dir.join("src");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    generate_docs_checks(&manifest_dir, &src_dir, &out_dir);
    let generated_path = out_dir.join("story_registry.generated.rs");

    let story_files = discover_story_files(&src_dir).unwrap_or_else(|error| {
        panic!("failed to discover Studio story files under {src_dir:?}: {error}")
    });
    let story_modules = story_files
        .iter()
        .map(|story_file| {
            StoryModule::read(&src_dir, story_file).unwrap_or_else(|error| {
                panic!(
                    "failed to parse Studio story file {}:\n{error}",
                    story_file.display()
                )
            })
        })
        .collect::<Vec<_>>();

    validate_story_ids(&story_modules);
    validate_default_story_id(&src_dir, &story_modules);
    fs::write(generated_path, generate_registry(&story_modules))
        .expect("write generated story registry");
}

/// Guarantee `assets/tailwind.css` exists so `asset!("/assets/tailwind.css")`
/// resolves — it is checked at compile time, and rustc runs before anything
/// else can create the file.
///
/// Its CONTENT is dx's to produce: every `dx build`/`dx serve` installs the
/// Tailwind CLI and overwrites this file from `tailwind.css`. That is why the
/// file is gitignored rather than tracked. A tracked generated bundle merges
/// by text, and the merge of two branches' bundles is neither branch's — the
/// correct answer is a regeneration over the merged `src/`, which git cannot
/// perform. Tracking it produced silently-wrong CSS on merge commits and a
/// recurring "regenerate tailwind.css post-merge" chore.
///
/// Only the plain-cargo paths (`cargo check`, clippy, rust-analyzer) ever see
/// this placeholder, and only on a fresh clone. It must never reach a deploy:
/// `scripts/pages/prepare-pages-artifact.mjs` enforces a byte floor on the
/// emitted stylesheet for exactly that reason.
///
/// Deliberately NOT paired with a `rerun-if-changed` on the file: dx rewrites
/// it on every build, so watching it would re-run this script every time
/// (docs/defects — never watch files you write).
fn ensure_tailwind_placeholder(manifest_dir: &Path) {
    const PLACEHOLDER: &str = "/*! tailwindcss v4.1.5 | MIT License | https://tailwindcss.com */\n\
                               @layer theme, base, components, utilities;\n\
                               @layer utilities;\n";

    let asset_path = manifest_dir.join("assets/tailwind.css");
    if asset_path.exists() {
        return;
    }
    if let Some(assets_dir) = asset_path.parent() {
        fs::create_dir_all(assets_dir)
            .unwrap_or_else(|error| panic!("create {}: {error}", assets_dir.display()));
    }
    fs::write(&asset_path, PLACEHOLDER)
        .unwrap_or_else(|error| panic!("write {}: {error}", asset_path.display()));
}

/// Bake git facts into the build so the header's version chip can show the
/// branch on dev builds (deploys fetch `version.json` instead, which wins).
/// Best-effort by design: no git, no repo, or a failing command simply
/// leaves the env vars unset and the chip falls back to "dev build" —
/// this must never fail the build.
///
/// The dirty flag is only as fresh as the last build-script run; we rerun
/// on HEAD moves (commit/branch switch) but deliberately not on every file
/// edit, so a stale dirty marker between builds is accepted.
fn emit_git_facts(manifest_dir: &Path) {
    let git = |args: &[&str]| -> Option<String> {
        let output = std::process::Command::new("git")
            .args(args)
            .current_dir(manifest_dir)
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
    };

    let sha = git(&["rev-parse", "--short=8", "HEAD"]).filter(|sha| !sha.is_empty());
    // A detached HEAD (mid-rebase, CI checkouts, `git checkout <sha>`)
    // reports the literal "HEAD", which tells a reader nothing — show the
    // commit instead.
    let branch = git(&["rev-parse", "--abbrev-ref", "HEAD"])
        .filter(|branch| !branch.is_empty() && branch != "HEAD")
        .or_else(|| sha.clone());
    if let Some(branch) = branch {
        println!("cargo:rustc-env=STUDIO_GIT_BRANCH={branch}");
    }
    if let Some(sha) = sha {
        println!("cargo:rustc-env=STUDIO_GIT_SHA={sha}");
    }
    if let Some(status) = git(&["status", "--porcelain"]) {
        let dirty = if status.is_empty() { "0" } else { "1" };
        println!("cargo:rustc-env=STUDIO_GIT_DIRTY={dirty}");
    }
    // Rerun when HEAD moves. `--absolute-git-dir` resolves linked worktrees,
    // where `.git` is a file pointing at the real gitdir.
    if let Some(git_dir) = git(&["rev-parse", "--absolute-git-dir"]) {
        println!("cargo:rerun-if-changed={git_dir}/HEAD");
    }
}

fn discover_story_files(src_dir: &Path) -> io::Result<Vec<PathBuf>> {
    let mut story_files = Vec::new();
    collect_story_files(src_dir, &mut story_files)?;
    story_files.sort();
    Ok(story_files)
}

fn collect_story_files(dir: &Path, story_files: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_story_files(&path, story_files)?;
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|file_name| file_name.to_str()) else {
            continue;
        };
        if file_name.ends_with("_stories.rs") {
            story_files.push(path);
        }
    }
    Ok(())
}

#[derive(Debug)]
struct StoryModule {
    path: PathBuf,
    module_path: String,
    stories: Vec<StorySpec>,
}

impl StoryModule {
    fn read(src_dir: &Path, story_file: &Path) -> Result<Self, String> {
        let source = fs::read_to_string(story_file)
            .map_err(|error| format!("could not read story file: {error}"))?;
        let parsed = syn::parse_file(&source)
            .map_err(|error| format!("Rust parse error before story discovery: {error}"))?;
        let path_info = StoryPathInfo::from_path(src_dir, story_file)?;
        let module_path = story_module_path(src_dir, story_file)?;
        let source_path = story_source_path(src_dir, story_file)?;

        let mut stories = Vec::new();
        for item in parsed.items {
            let Item::Fn(function) = item else {
                continue;
            };
            let Some(attribute) = function.attrs.iter().find(|attr| is_story_attr(attr)) else {
                continue;
            };
            let metadata =
                StoryMetadata::from_attribute(attribute, &function.sig.ident.to_string())?;
            let story_segment = route_segment_from_ident(&function.sig.ident.to_string());
            let id = path_info.story_id(&story_segment);
            stories.push(StorySpec {
                id,
                source_path: source_path.clone(),
                family: path_info.family.clone(),
                category: path_info.category.clone(),
                component: path_info.component.clone(),
                story: story_segment,
                function_name: function.sig.ident.to_string(),
                label: metadata.label,
                description: metadata.description,
                screenshot: metadata.screenshot,
            });
        }

        if stories.is_empty() {
            return Err(format!(
                "story file matched `*_stories.rs` but contains no `#[story]` functions.\n\
                 Add functions like `#[story] fn example() -> Element {{ ... }}`,\n\
                 or rename the file so it does not end with `_stories.rs`."
            ));
        }

        Ok(Self {
            path: story_file.to_path_buf(),
            module_path,
            stories,
        })
    }
}

#[derive(Debug)]
struct StoryPathInfo {
    family: String,
    category: Option<String>,
    component: String,
}

impl StoryPathInfo {
    fn from_path(src_dir: &Path, story_file: &Path) -> Result<Self, String> {
        let relative = story_file
            .strip_prefix(src_dir)
            .map_err(|_| "story file is not under src".to_string())?;
        let segments = relative
            .iter()
            .map(|segment| segment.to_string_lossy().into_owned())
            .collect::<Vec<_>>();

        match segments.as_slice() {
            [source_root, file_name] => Ok(Self {
                family: story_family_from_source_root(source_root)?,
                category: None,
                component: component_from_story_file(file_name)?,
            }),
            [source_root, category, file_name] => Ok(Self {
                family: story_family_from_source_root(source_root)?,
                category: Some(route_segment_from_ident(category)),
                component: component_from_story_file(file_name)?,
            }),
            _ => Err(format!(
                "unsupported story path `{}`.\n\
                 Expected a story file under `src/base`, `src/core`, \
                 `src/app`, or `src/exploration`, using either \
                 `<component>_stories.rs` or `<category>/<component>_stories.rs`.",
                relative.display()
            )),
        }
    }

    fn story_id(&self, story: &str) -> String {
        let mut id = self.family.clone();
        id.push('/');
        if let Some(category) = &self.category {
            id.push_str(category);
            id.push('/');
        }
        id.push_str(&self.component);
        id.push('/');
        id.push_str(story);
        id
    }
}

#[derive(Debug)]
struct StoryMetadata {
    label: String,
    description: String,
    screenshot: bool,
}

impl StoryMetadata {
    fn from_attribute(attribute: &Attribute, function_name: &str) -> Result<Self, String> {
        let mut label = None;
        let mut description = None;
        let mut screenshot = false;
        let mut errors = Vec::new();

        match &attribute.meta {
            Meta::Path(_) => {}
            Meta::List(_) => {
                attribute
                    .parse_nested_meta(|meta| {
                        if meta.path.is_ident("label") {
                            let value = meta.value()?;
                            let literal: syn::LitStr = value.parse()?;
                            if label.replace(literal.value()).is_some() {
                                errors.push(format!(
                                    "`{function_name}` has duplicate `label` entries in #[story]"
                                ));
                            }
                            return Ok(());
                        }

                        if meta.path.is_ident("description") {
                            let value = meta.value()?;
                            let literal: syn::LitStr = value.parse()?;
                            if description.replace(literal.value()).is_some() {
                                errors.push(format!(
                                    "`{function_name}` has duplicate `description` entries in #[story]"
                                ));
                            }
                            return Ok(());
                        }

                        if meta.path.is_ident("screenshot") {
                            if screenshot {
                                errors.push(format!(
                                    "`{function_name}` has duplicate `screenshot` entries in #[story]"
                                ));
                            }
                            screenshot = true;
                            return Ok(());
                        }

                        let name = meta
                            .path
                            .get_ident()
                            .map(ToString::to_string)
                            .unwrap_or_else(|| "<unknown>".to_string());
                        errors.push(format!(
                            "`{function_name}` uses unsupported #[story] argument `{name}`; \
                             use `#[story]`, `label = \"...\"`, `description = \"...\"`, \
                             or `screenshot`"
                        ));
                        Ok(())
                    })
                    .map_err(|error| {
                        format!("could not parse #[story(...)] on `{function_name}`: {error}")
                    })?;
            }
            Meta::NameValue(_) => {
                errors.push(format!(
                    "`{function_name}` uses unsupported #[story = ...] syntax; \
                     use `#[story]` or `#[story(label = \"...\")]`"
                ));
            }
        }

        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }

        Ok(Self {
            label: label.unwrap_or_else(|| story_label_from_ident(function_name)),
            description: description.unwrap_or_default(),
            screenshot,
        })
    }
}

fn story_label_from_ident(function_name: &str) -> String {
    let mut label = String::with_capacity(function_name.len());
    let mut previous_was_space = false;
    for ch in function_name.chars() {
        if ch.is_ascii_alphanumeric() {
            if label.is_empty() {
                label.push(ch.to_ascii_uppercase());
            } else {
                label.push(ch.to_ascii_lowercase());
            }
            previous_was_space = false;
        } else if !label.is_empty() && !previous_was_space {
            label.push(' ');
            previous_was_space = true;
        }
    }
    if label.ends_with(' ') {
        label.pop();
    }
    label
}

#[derive(Debug)]
struct StorySpec {
    id: String,
    source_path: String,
    family: String,
    category: Option<String>,
    component: String,
    story: String,
    function_name: String,
    label: String,
    description: String,
    screenshot: bool,
}

fn validate_story_ids(story_modules: &[StoryModule]) {
    let mut seen = HashMap::<&str, &Path>::new();
    let mut duplicates = Vec::new();
    for module in story_modules {
        for story in &module.stories {
            if let Some(existing_path) = seen.insert(&story.id, &module.path) {
                duplicates.push(format!(
                    "`{}` is declared in both `{}` and `{}`",
                    story.id,
                    existing_path.display(),
                    module.path.display()
                ));
            }
        }
    }

    if !duplicates.is_empty() {
        panic!(
            "duplicate Studio story ids detected:\n{}",
            duplicates.join("\n")
        );
    }
}

/// Fail the build when `DEFAULT_STORY_ID` names a story that no longer
/// exists. A stale default renders an EMPTY storybook page — which the
/// capture pipeline reports as "No story links were discovered", far from
/// the actual cause. Caught the hard way when the step-stack device pane
/// was deleted and took `studio/layout/studio-shell/simulator-idle` with
/// it.
fn validate_default_story_id(src_dir: &Path, story_modules: &[StoryModule]) {
    let registry_path = src_dir.join("stories/story_registry.rs");
    let source = fs::read_to_string(&registry_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", registry_path.display()));
    let Some(default_id) = source
        .split("pub const DEFAULT_STORY_ID: &str = \"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
    else {
        panic!(
            "could not read DEFAULT_STORY_ID from {}",
            registry_path.display()
        );
    };
    // Either a story id or a component-overview id (`<component>/overview`,
    // synthesized by the story book rather than declared by a `#[story]`).
    let exists = story_modules
        .iter()
        .flat_map(|module| &module.stories)
        .any(|story| match story.id.rsplit_once('/') {
            Some((component, _)) => {
                story.id == default_id || default_id == format!("{component}/overview")
            }
            None => story.id == default_id,
        });
    assert!(
        exists,
        "DEFAULT_STORY_ID `{default_id}` in {} names no registered story — the storybook \n\
         would render an empty page and capture would find zero stories. Point it at a \n\
         story that exists.",
        registry_path.display()
    );
}

fn generate_registry(story_modules: &[StoryModule]) -> String {
    let mut generated = String::new();
    generated.push_str("// @generated by lpa-studio-web/build.rs\n\n");
    generated.push_str("pub const GENERATED_AT_UTC: &str = \"");
    generated.push_str(&rust_string_literal(&build_timestamp_utc()));
    generated.push_str("\";\n\n");

    generated.push_str(
        "\npub fn all_generated_stories() -> Vec<crate::stories::story::StoryDescriptor> {\n",
    );
    generated.push_str("    vec![\n");
    for story_module in story_modules {
        for story in &story_module.stories {
            generated.push_str("        crate::stories::story::StoryDescriptor::new(\n");
            generated.push_str("            \"");
            generated.push_str(&rust_string_literal(&story.id));
            generated.push_str("\",\n");
            generated.push_str("            \"");
            generated.push_str(&rust_string_literal(&story.source_path));
            generated.push_str("\",\n");
            generated.push_str("            \"");
            generated.push_str(&rust_string_literal(&story.family));
            generated.push_str("\",\n");
            generated.push_str("            ");
            match &story.category {
                Some(category) => {
                    generated.push_str("Some(\"");
                    generated.push_str(&rust_string_literal(category));
                    generated.push_str("\")");
                }
                None => generated.push_str("None"),
            }
            generated.push_str(",\n");
            generated.push_str("            \"");
            generated.push_str(&rust_string_literal(&story.component));
            generated.push_str("\",\n");
            generated.push_str("            \"");
            generated.push_str(&rust_string_literal(&story.story));
            generated.push_str("\",\n");
            generated.push_str("            \"");
            generated.push_str(&rust_string_literal(&story.label));
            generated.push_str("\",\n");
            generated.push_str("            \"");
            generated.push_str(&rust_string_literal(&story.description));
            generated.push_str("\",\n");
            generated.push_str("            ");
            generated.push_str(if story.screenshot { "true" } else { "false" });
            generated.push_str(",\n");
            generated.push_str("        ),\n");
        }
    }
    generated.push_str("    ]\n");
    generated.push_str("}\n");

    generated.push_str(
        "\npub fn render_generated_story(id: &str) -> Option<dioxus::prelude::Element> {\n",
    );
    generated.push_str("    match id {\n");
    for story_module in story_modules {
        for story in &story_module.stories {
            generated.push_str("        \"");
            generated.push_str(&rust_string_literal(&story.id));
            generated.push_str("\" => Some(");
            generated.push_str(&story_module.module_path);
            generated.push_str("::");
            generated.push_str(&story.function_name);
            generated.push_str("()),\n");
        }
    }
    generated.push_str("        _ => None,\n");
    generated.push_str("    }\n");
    generated.push_str("}\n");

    generated
}

fn build_timestamp_utc() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is after Unix epoch")
        .as_secs();
    format_unix_timestamp_utc(seconds)
}

fn format_unix_timestamp_utc(seconds: u64) -> String {
    let days = (seconds / 86_400) as i64;
    let day_seconds = seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    let hour = day_seconds / 3_600;
    let minute = (day_seconds % 3_600) / 60;
    let second = day_seconds % 60;

    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let days = days_since_epoch + 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let day_of_era = days - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let mut year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };

    (year, month as u32, day as u32)
}

fn is_story_attr(attribute: &Attribute) -> bool {
    attribute
        .path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "story")
}

fn story_family_from_source_root(source_root: &str) -> Result<String, String> {
    match source_root {
        "base" => Ok("base".to_string()),
        "core" => Ok("core".to_string()),
        "app" => Ok("studio".to_string()),
        "exploration" => Ok("exploration".to_string()),
        _ => Err(format!(
            "unsupported story source root `{source_root}`.\n\
             Component stories should live beside their components in `base`, \
             `core`, or `app`. Design spikes may live in `exploration`."
        )),
    }
}

fn component_from_story_file(file_name: &str) -> Result<String, String> {
    let Some(component) = file_name.strip_suffix("_stories.rs") else {
        return Err(format!(
            "story file `{file_name}` should end with `_stories.rs`"
        ));
    };
    if component.is_empty() {
        return Err(format!(
            "story file `{file_name}` must include a component name before `_stories.rs`"
        ));
    }
    Ok(route_segment_from_ident(component))
}

fn route_segment_from_ident(value: &str) -> String {
    let mut segment = String::with_capacity(value.len());
    let mut previous_was_separator = false;
    for ch in value.chars() {
        let normalized = if ch.is_ascii_alphanumeric() {
            previous_was_separator = false;
            ch.to_ascii_lowercase()
        } else if previous_was_separator {
            continue;
        } else {
            previous_was_separator = true;
            '-'
        };
        segment.push(normalized);
    }
    segment.trim_matches('-').to_string()
}

fn story_module_path(src_dir: &Path, story_file: &Path) -> Result<String, String> {
    let relative = story_file
        .strip_prefix(src_dir)
        .map_err(|_| "story file is not under src".to_string())?;
    let mut module_path = "crate".to_string();
    for component in relative.components() {
        let segment = component.as_os_str().to_string_lossy();
        let segment = segment.strip_suffix(".rs").unwrap_or(&segment);
        module_path.push_str("::");
        module_path.push_str(segment);
    }
    Ok(module_path)
}

fn story_source_path(src_dir: &Path, story_file: &Path) -> Result<String, String> {
    let relative = story_file
        .strip_prefix(src_dir)
        .map_err(|_| "story file is not under src".to_string())?;
    Ok(format!("src/{}", slash_path(relative)))
}

fn slash_path(path: &Path) -> String {
    path.iter()
        .map(|segment| segment.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

fn rust_string_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

// ==========================================================================
// Docs article scan
// ==========================================================================
//
// The in-app docs section (`src/app/docs`) compiles articles in from
// `docs/user-guide/`. Articles reference code by name: `embed` directive
// fences, `sim=` handles a page declares, and `#/docs/<slug>[#<anchor>]`
// links. None of those references is checked by the compiler, so this scan
// turns them into data and generates the tests that check them
// (`docs_checks.generated.rs`, wrapped by `src/app/docs/docs_checks.rs`).
//
// The scan is deliberately line-based rather than a second markdown parser:
// the generated `scan_matches_the_markdown_parser` test compares its embed
// names and heading anchors against the real renderer's parse of the real
// articles, so any divergence (setext headings, exotic inline formatting in
// a heading) shows up as a failing test on the article that triggers it
// rather than as a silently wrong check.

/// One `PAGES` entry as declared in `src/app/docs/mod.rs`.
#[derive(Debug)]
struct DocPageDecl {
    slug: String,
    /// Article file name inside `docs/user-guide/`.
    file: String,
    /// Declared sims: `(name, example_id)` in declaration order.
    sims: Vec<(String, String)>,
}

/// A registered article plus everything the scan found in it.
#[derive(Debug)]
struct DocsArticle {
    page: DocPageDecl,
    scan: ArticleScan,
}

/// What one article's markdown references, in document order.
#[derive(Debug, Default)]
struct ArticleScan {
    /// `embed` fence names.
    embeds: Vec<String>,
    /// `(embed name, sim handle)` for every `sim=` argument, comma lists
    /// split into one entry per handle.
    sim_refs: Vec<(String, String)>,
    /// `(slug, anchor)` for every `#/docs/...` link target; anchor is empty
    /// for an unanchored link.
    links: Vec<(String, String)>,
    /// Heading anchor ids, deduped exactly as the renderer dedupes them.
    anchors: Vec<String>,
}

fn generate_docs_checks(manifest_dir: &Path, src_dir: &Path, out_dir: &Path) {
    let docs_dir = user_guide_dir(manifest_dir);
    println!("cargo:rerun-if-changed={}", docs_dir.display());
    for markdown_file in markdown_files(&docs_dir) {
        println!("cargo:rerun-if-changed={}", markdown_file.display());
    }

    let pages = read_docs_manifest(&src_dir.join("app/docs/mod.rs"));
    let mut articles = pages
        .into_iter()
        .map(|page| {
            let path = docs_dir.join(&page.file);
            let markdown = fs::read_to_string(&path).unwrap_or_else(|error| {
                panic!(
                    "docs page `{}` names `{}`, which could not be read: {error}",
                    page.slug,
                    path.display()
                )
            });
            DocsArticle {
                scan: scan_article(&markdown),
                page,
            }
        })
        .collect::<Vec<_>>();
    articles.sort_by(|left, right| left.page.slug.cmp(&right.page.slug));

    fs::write(
        out_dir.join("docs_checks.generated.rs"),
        render_docs_checks(&articles),
    )
    .expect("write generated docs checks");
}

/// `docs/user-guide/`, from `lp-app/lpa-studio-web/`.
fn user_guide_dir(manifest_dir: &Path) -> PathBuf {
    let repo_root = manifest_dir
        .parent()
        .and_then(Path::parent)
        .unwrap_or_else(|| panic!("crate at {} has no repo root", manifest_dir.display()));
    let docs_dir = repo_root.join("docs/user-guide");
    assert!(
        docs_dir.is_dir(),
        "expected the user guide at {} (resolved from {})",
        docs_dir.display(),
        manifest_dir.display()
    );
    docs_dir
}

/// Every `.md` file in the guide directory, sorted. Files that are not
/// registered in `PAGES` (the contributor-facing `STYLE.md`, for one) are
/// still watched — adding an article should rebuild the checks — but only
/// registered pages are scanned, since only they render.
fn markdown_files(docs_dir: &Path) -> Vec<PathBuf> {
    let mut files = fs::read_dir(docs_dir)
        .unwrap_or_else(|error| panic!("read {}: {error}", docs_dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|extension| extension == "md"))
        .collect::<Vec<_>>();
    files.sort();
    files
}

/// Read the `PAGES` manifest out of `app/docs/mod.rs`.
///
/// Parsed rather than duplicated: the slug ↔ article-file mapping and the
/// per-page sim declarations both live there, and a copy here would be the
/// exact kind of drift these checks exist to prevent. A shape this does not
/// understand fails the build with the offending entry named.
fn read_docs_manifest(mod_path: &Path) -> Vec<DocPageDecl> {
    let source = fs::read_to_string(mod_path)
        .unwrap_or_else(|error| panic!("read {}: {error}", mod_path.display()));
    let parsed = syn::parse_file(&source)
        .unwrap_or_else(|error| panic!("Rust parse error in {}: {error}", mod_path.display()));
    let pages = parsed
        .items
        .iter()
        .find_map(|item| match item {
            Item::Const(item) if item.ident == "PAGES" => Some(item),
            _ => None,
        })
        .unwrap_or_else(|| panic!("no `PAGES` const in {}", mod_path.display()));
    let entries = array_elements(&pages.expr).unwrap_or_else(|| {
        panic!(
            "`PAGES` in {} is not a `&[...]` array literal",
            mod_path.display()
        )
    });
    entries
        .iter()
        .map(|entry| parse_doc_page_entry(entry, mod_path))
        .collect()
}

fn parse_doc_page_entry(expr: &Expr, mod_path: &Path) -> DocPageDecl {
    let Expr::Struct(entry) = expr else {
        panic!(
            "every `PAGES` entry in {} must be a `DocPage {{ ... }}` literal",
            mod_path.display()
        );
    };

    let mut slug = None;
    let mut file = None;
    let mut sims = Vec::new();
    for field in &entry.fields {
        let syn::Member::Named(name) = &field.member else {
            continue;
        };
        match name.to_string().as_str() {
            "slug" => slug = string_literal(&field.expr),
            "markdown" => file = include_str_file_name(&field.expr),
            "sims" => sims = parse_sim_specs(&field.expr, mod_path),
            _ => {}
        }
    }

    let slug = slug.unwrap_or_else(|| {
        panic!(
            "a `PAGES` entry in {} has no string-literal `slug`",
            mod_path.display()
        )
    });
    let file = file.unwrap_or_else(|| {
        panic!(
            "docs page `{slug}` in {} must set `markdown` to `include_str!(\"...\")`",
            mod_path.display()
        )
    });
    DocPageDecl { slug, file, sims }
}

fn parse_sim_specs(expr: &Expr, mod_path: &Path) -> Vec<(String, String)> {
    let elements = array_elements(expr).unwrap_or_else(|| {
        panic!(
            "a `PAGES` entry in {} sets `sims` to something other than a `&[...]` array",
            mod_path.display()
        )
    });
    elements
        .iter()
        .map(|element| {
            let Expr::Struct(spec) = element else {
                panic!(
                    "every `sims` entry in {} must be a `DocsSimSpec {{ ... }}` literal",
                    mod_path.display()
                );
            };
            let mut name = None;
            let mut example_id = None;
            for field in &spec.fields {
                let syn::Member::Named(field_name) = &field.member else {
                    continue;
                };
                match field_name.to_string().as_str() {
                    "name" => name = string_literal(&field.expr),
                    "example_id" => example_id = string_literal(&field.expr),
                    _ => {}
                }
            }
            match (name, example_id) {
                (Some(name), Some(example_id)) => (name, example_id),
                _ => panic!(
                    "a `DocsSimSpec` in {} is missing a string-literal `name` or `example_id`",
                    mod_path.display()
                ),
            }
        })
        .collect()
}

/// `&[a, b]` or `[a, b]` → its elements.
fn array_elements(expr: &Expr) -> Option<Vec<Expr>> {
    match expr {
        Expr::Reference(reference) => array_elements(&reference.expr),
        Expr::Array(array) => Some(array.elems.iter().cloned().collect()),
        _ => None,
    }
}

fn string_literal(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Lit(literal) => match &literal.lit {
            Lit::Str(value) => Some(value.value()),
            _ => None,
        },
        _ => None,
    }
}

/// `include_str!("../../docs/user-guide/x.md")` → `x.md`.
fn include_str_file_name(expr: &Expr) -> Option<String> {
    let Expr::Macro(macro_expr) = expr else {
        return None;
    };
    if !macro_expr
        .mac
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "include_str")
    {
        return None;
    }
    let path: syn::LitStr = macro_expr.mac.parse_body().ok()?;
    let path = path.value();
    Some(path.rsplit('/').next()?.to_string())
}

// -- markdown scanning ------------------------------------------------------

fn scan_article(markdown: &str) -> ArticleScan {
    let mut scan = ArticleScan::default();
    let mut anchor_counts: HashMap<String, u32> = HashMap::new();
    // The open fence's marker character and length, per CommonMark: a fence
    // closes on the same character, at least as long, with nothing after it.
    let mut open_fence: Option<(char, usize)> = None;

    for line in markdown.lines() {
        let trimmed = line.trim_start();
        if let Some((marker, length)) = fence_marker(trimmed) {
            match open_fence {
                Some((open_marker, open_length)) => {
                    if marker == open_marker
                        && length >= open_length
                        && trimmed[length..].trim().is_empty()
                    {
                        open_fence = None;
                    }
                }
                None => {
                    if let Some((name, args)) = parse_embed_info(trimmed[length..].trim()) {
                        for (key, value) in &args {
                            if key == "sim" {
                                for sim in value.split(',').filter(|sim| !sim.is_empty()) {
                                    scan.sim_refs.push((name.clone(), sim.to_string()));
                                }
                            }
                        }
                        scan.embeds.push(name);
                    }
                    open_fence = Some((marker, length));
                }
            }
            continue;
        }
        if open_fence.is_some() {
            continue;
        }

        if let Some(text) = heading_text(line) {
            let slug = slugify(&text);
            let count = anchor_counts.entry(slug.clone()).or_insert(0);
            *count += 1;
            scan.anchors.push(if *count == 1 {
                slug
            } else {
                format!("{slug}-{count}")
            });
        }
        collect_doc_links(line, &mut scan.links);
    }

    scan
}

/// A fence opener/closer: three or more backticks or tildes.
fn fence_marker(trimmed_line: &str) -> Option<(char, usize)> {
    let marker = trimmed_line.chars().next()?;
    if marker != '`' && marker != '~' {
        return None;
    }
    let length = trimmed_line.chars().take_while(|ch| *ch == marker).count();
    (length >= 3).then_some((marker, length))
}

/// The `embed <name> [key=value]...` fence grammar, kept identical to
/// `src/base/markdown_text.rs`'s `code_block_or_embed`: `embed` must be the
/// info string's first word, and a bare `embed` with no name is malformed
/// (it renders as a plain code block, so it is not an embed reference).
fn parse_embed_info(info: &str) -> Option<(String, Vec<(String, String)>)> {
    let mut words = info.split_whitespace();
    if words.next()? != "embed" {
        return None;
    }
    let name = words.next()?.to_string();
    let args = words
        .filter_map(|word| {
            word.split_once('=')
                .map(|(key, value)| (key.to_string(), value.to_string()))
        })
        .collect();
    Some((name, args))
}

/// An ATX heading's text, or `None` for any other line. Only column-zero
/// headings count — those are the ones the renderer gives an `id` to.
fn heading_text(line: &str) -> Option<String> {
    if !line.starts_with('#') {
        return None;
    }
    let hashes = line.chars().take_while(|ch| *ch == '#').count();
    if hashes > 6 {
        return None;
    }
    let rest = &line[hashes..];
    if !rest.is_empty() && !rest.starts_with([' ', '\t']) {
        return None;
    }
    // Trailing `#`s and other punctuation need no stripping: slugify drops
    // every non-alphanumeric anyway.
    Some(strip_inline_link_targets(rest.trim()))
}

/// Reduce `[label](target)` to `label` so a link inside a heading slugs to
/// the same anchor the renderer computes (which sees only the label).
fn strip_inline_link_targets(text: &str) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '[' {
            let close = chars[index + 1..].iter().position(|ch| *ch == ']');
            if let Some(close) = close.map(|offset| index + 1 + offset) {
                if chars.get(close + 1) == Some(&'(') {
                    if let Some(end) = chars[close + 2..].iter().position(|ch| *ch == ')') {
                        out.extend(&chars[index + 1..close]);
                        index = close + 2 + end + 1;
                        continue;
                    }
                }
            }
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

/// Collect `#/docs/<slug>[#<anchor>]` markdown link targets from one line.
/// Only real link destinations (`](...)`) count — prose that mentions the
/// route shape in backticks is not a link.
fn collect_doc_links(line: &str, links: &mut Vec<(String, String)>) {
    let mut rest = line;
    while let Some(index) = rest.find("](") {
        let after = &rest[index + 2..];
        let end = after
            .find(|ch: char| ch == ')' || ch.is_whitespace())
            .unwrap_or(after.len());
        let target = after[..end].trim_matches(['<', '>']);
        if let Some(target) = target.strip_prefix("#/docs/") {
            let (slug, anchor) = match target.split_once('#') {
                Some((slug, anchor)) => (slug, anchor),
                None => (target, ""),
            };
            links.push((slug.to_string(), anchor.to_string()));
        }
        rest = &after[end..];
    }
}

/// Heading text → anchor id.
///
/// SOURCE OF TRUTH: `src/base/markdown_text.rs`'s `slugify`. A build script
/// cannot depend on its own crate, so this is a verbatim duplicate; the
/// generated `scan_matches_the_markdown_parser` test compares this side's
/// output against that one's over every real article, so the two cannot
/// drift silently. Change one, change the other.
fn slugify(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut needs_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            if needs_dash && !slug.is_empty() {
                slug.push('-');
            }
            needs_dash = false;
            slug.push(ch.to_ascii_lowercase());
        } else {
            needs_dash = true;
        }
    }
    slug
}

// -- codegen ----------------------------------------------------------------

fn render_docs_checks(articles: &[DocsArticle]) -> String {
    let mut generated = String::new();
    generated.push_str("// @generated by lpa-studio-web/build.rs\n");
    generated.push_str("// Scanned from docs/user-guide/, driven by app/docs/mod.rs's PAGES.\n\n");
    generated.push_str(&render_docs_links(articles));
    generated.push('\n');
    generated.push_str(&render_generated_checks(articles));
    generated
}

/// The compile-time link surface: `docs_links::<page>::HREF` and one const
/// per heading anchor. A link written through these can only name a page
/// and anchor that exist, because a missing one is a missing const.
fn render_docs_links(articles: &[DocsArticle]) -> String {
    let mut generated = String::new();
    generated.push_str(
        "/// Every in-app docs link target, as consts: a page's `HREF` plus one\n\
         /// const per heading anchor. Naming a page or anchor that does not exist\n\
         /// is a missing const, i.e. a compile error rather than a dead link.\n",
    );
    generated.push_str(
        "#[allow(\n    dead_code,\n    reason = \"the link surface is consumed by docs help links in a later phase\"\n)]\n",
    );
    generated.push_str("pub mod docs_links {\n");
    let mut modules = BTreeMap::new();
    for article in articles {
        let module = identifier(&article.page.slug, false);
        if let Some(existing) = modules.insert(module.clone(), article.page.slug.clone()) {
            panic!(
                "docs slugs `{existing}` and `{}` both map to the link module `{module}`",
                article.page.slug
            );
        }
    }
    for article in articles {
        let module = identifier(&article.page.slug, false);
        let slug = &article.page.slug;
        generated.push_str(&format!("    /// `#/docs/{slug}`\n"));
        generated.push_str(&format!("    pub mod {module} {{\n"));
        generated.push_str(&format!(
            "        pub const HREF: &str = \"#/docs/{}\";\n",
            rust_string_literal(slug)
        ));
        let mut consts = BTreeMap::new();
        for anchor in &article.scan.anchors {
            let name = identifier(anchor, true);
            if let Some(existing) = consts.insert(name.clone(), anchor.clone()) {
                panic!(
                    "docs page `{slug}` anchors `{existing}` and `{anchor}` both map to \
                     the link const `{name}`; rename one of the headings"
                );
            }
            assert!(
                name != "HREF",
                "docs page `{slug}` has a heading whose anchor collides with `HREF`"
            );
        }
        for (name, anchor) in &consts {
            generated.push_str(&format!(
                "        pub const {name}: &str = \"#/docs/{}#{}\";\n",
                rust_string_literal(slug),
                rust_string_literal(anchor)
            ));
        }
        generated.push_str("    }\n");
    }
    generated.push_str("}\n");
    generated
}

fn render_generated_checks(articles: &[DocsArticle]) -> String {
    let mut generated = String::new();
    generated.push_str("#[cfg(test)]\nmod generated_checks {\n");
    generated.push_str(
        "    use crate::app::docs::PAGES;\n\
         \x20   use crate::app::docs::embeds::EMBED_NAMES;\n\
         \x20   use crate::base::markdown_text::{MdNode, heading_anchors, parse_markdown};\n\n",
    );
    generated.push_str(
        "    /// One registered article as `build.rs` scanned it.\n\
         \x20   struct ScannedArticle {\n\
         \x20       slug: &'static str,\n\
         \x20       file: &'static str,\n\
         \x20       sims: &'static [(&'static str, &'static str)],\n\
         \x20       embeds: &'static [&'static str],\n\
         \x20       sim_refs: &'static [(&'static str, &'static str)],\n\
         \x20       links: &'static [(&'static str, &'static str)],\n\
         \x20       anchors: &'static [&'static str],\n\
         \x20   }\n\n",
    );

    generated.push_str("    const SCANNED: &[ScannedArticle] = &[\n");
    for article in articles {
        generated.push_str("        ScannedArticle {\n");
        generated.push_str(&format!(
            "            slug: \"{}\",\n",
            rust_string_literal(&article.page.slug)
        ));
        generated.push_str(&format!(
            "            file: \"{}\",\n",
            rust_string_literal(&article.page.file)
        ));
        generated.push_str(&format!(
            "            sims: {},\n",
            pair_slice(&article.page.sims)
        ));
        generated.push_str(&format!(
            "            embeds: {},\n",
            string_slice(&article.scan.embeds)
        ));
        generated.push_str(&format!(
            "            sim_refs: {},\n",
            pair_slice(&article.scan.sim_refs)
        ));
        generated.push_str(&format!(
            "            links: {},\n",
            pair_slice(&article.scan.links)
        ));
        generated.push_str(&format!(
            "            anchors: {},\n",
            string_slice(&article.scan.anchors)
        ));
        generated.push_str("        },\n");
    }
    generated.push_str("    ];\n\n");

    generated.push_str(GENERATED_CHECK_TESTS);
    generated.push_str("}\n");
    generated
}

/// The assertions themselves are fixed text — only `SCANNED` varies per
/// build — so they live here as one literal rather than being assembled.
const GENERATED_CHECK_TESTS: &str = r####"
    fn scanned(slug: &str) -> &'static ScannedArticle {
        SCANNED
            .iter()
            .find(|article| article.slug == slug)
            .unwrap_or_else(|| panic!("no scanned article for docs page `{slug}`"))
    }

    /// The scan reads `PAGES` out of the source at build time; if that read
    /// ever drifts from the const the app actually renders, every other
    /// check here is checking the wrong thing.
    #[test]
    fn the_scan_matches_the_manifest() {
        assert_eq!(
            SCANNED.len(),
            PAGES.len(),
            "build.rs scanned {} article(s) but PAGES declares {}",
            SCANNED.len(),
            PAGES.len()
        );
        for page in PAGES {
            let article = scanned(page.slug);
            let declared = page
                .sims
                .iter()
                .map(|sim| (sim.name, sim.example_id))
                .collect::<Vec<_>>();
            assert_eq!(
                article.sims.to_vec(),
                declared,
                "docs page `{}` ({}) declares different sims than build.rs read",
                page.slug,
                article.file
            );
        }
    }

    /// A typo'd directive name would otherwise render as an unknown-embed
    /// box in the shipped article.
    #[test]
    fn every_embed_fence_names_a_registered_directive() {
        for article in SCANNED {
            for embed in article.embeds {
                assert!(
                    EMBED_NAMES.contains(embed),
                    "{} uses `embed {embed}`, which is not a registered directive; \
                     known directives: {EMBED_NAMES:?}",
                    article.file
                );
            }
        }
    }

    /// `sim=` addresses a sim the page declares in its `PAGES` entry.
    #[test]
    fn every_sim_reference_is_declared_by_its_page() {
        for article in SCANNED {
            for (embed, sim) in article.sim_refs {
                assert!(
                    article.sims.iter().any(|(name, _)| name == sim),
                    "{} has `embed {embed} ... sim={sim}`, but page `{}` declares {:?}",
                    article.file,
                    article.slug,
                    article.sims.iter().map(|(name, _)| *name).collect::<Vec<_>>()
                );
            }
        }
    }

    /// In-docs links resolve to a real page and, when anchored, to a real
    /// heading on it.
    #[test]
    fn every_docs_link_resolves() {
        for article in SCANNED {
            for (slug, anchor) in article.links {
                assert!(
                    PAGES.iter().any(|page| page.slug == *slug),
                    "{} links to `#/docs/{slug}`, which is not a page in PAGES",
                    article.file
                );
                if !anchor.is_empty() {
                    let target = scanned(slug);
                    assert!(
                        target.anchors.contains(anchor),
                        "{} links to `#/docs/{slug}#{anchor}`, but `{}` has no such heading; \
                         its anchors: {:?}",
                        article.file,
                        target.file,
                        target.anchors
                    );
                }
            }
        }
    }

    /// The build-time scan is line-based; the renderer uses pulldown-cmark.
    /// This pins the two together over the real articles, so a heading style
    /// or fence shape the scan gets wrong fails here instead of quietly
    /// weakening the checks above (it also guards the duplicated `slugify`).
    #[test]
    fn the_scan_matches_the_markdown_parser() {
        for page in PAGES {
            let article = scanned(page.slug);
            let parsed = parse_markdown(page.markdown)
                .into_iter()
                .filter_map(|node| match node {
                    MdNode::Embed { name, .. } => Some(name),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                article.embeds.to_vec(),
                parsed,
                "build.rs and the markdown renderer disagree about the embed fences in {}",
                article.file
            );
            assert_eq!(
                article.anchors.to_vec(),
                heading_anchors(page.markdown),
                "build.rs and the markdown renderer disagree about the heading anchors in {}",
                article.file
            );
        }
    }
"####;

fn string_slice(values: &[String]) -> String {
    if values.is_empty() {
        return "&[]".to_string();
    }
    let items = values
        .iter()
        .map(|value| format!("\"{}\"", rust_string_literal(value)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("&[{items}]")
}

fn pair_slice(values: &[(String, String)]) -> String {
    if values.is_empty() {
        return "&[]".to_string();
    }
    let items = values
        .iter()
        .map(|(left, right)| {
            format!(
                "(\"{}\", \"{}\")",
                rust_string_literal(left),
                rust_string_literal(right)
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("&[{items}]")
}

/// Slug or anchor → a Rust identifier: non-alphanumerics become `_`, and a
/// leading digit gets an `_` prefix. `upper` picks const case.
fn identifier(value: &str, upper: bool) -> String {
    let mut ident = String::with_capacity(value.len() + 1);
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            ident.push(if upper {
                ch.to_ascii_uppercase()
            } else {
                ch.to_ascii_lowercase()
            });
        } else {
            ident.push('_');
        }
    }
    if ident.is_empty() || ident.starts_with(|ch: char| ch.is_ascii_digit()) {
        ident.insert(0, '_');
    }
    if !upper && RUST_KEYWORDS.contains(&ident.as_str()) {
        ident.push('_');
    }
    ident
}

/// Enough of the keyword list that a plausible slug (`type`, `move`, `use`)
/// cannot generate a module name that will not compile.
const RUST_KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "if", "impl", "in",
    "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "static", "struct", "super", "trait", "true", "try", "type", "typeof",
    "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];
