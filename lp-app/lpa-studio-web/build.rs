use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use syn::{Attribute, Item, Meta};

fn main() {
    println!("cargo:rerun-if-changed=src");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    emit_git_facts(&manifest_dir);
    let src_dir = manifest_dir.join("src");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
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
