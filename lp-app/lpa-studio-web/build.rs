use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use syn::{Attribute, Item};

fn main() {
    println!("cargo:rerun-if-changed=src");

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let src_dir = manifest_dir.join("src");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));
    let generated_path = out_dir.join("story_registry.generated.rs");

    let story_files = discover_story_files(&src_dir).unwrap_or_else(|error| {
        panic!("failed to discover Studio story files under {src_dir:?}: {error}")
    });
    let story_modules = story_files
        .iter()
        .enumerate()
        .map(|(index, story_file)| {
            StoryModule::read(&src_dir, story_file, index).unwrap_or_else(|error| {
                panic!(
                    "failed to parse Studio story file {}:\n{error}",
                    story_file.display()
                )
            })
        })
        .collect::<Vec<_>>();

    validate_story_ids(&story_modules);
    fs::write(generated_path, generate_registry(&story_modules))
        .expect("write generated story registry");
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
        if file_name.ends_with("_story.rs") {
            story_files.push(path);
        }
    }
    Ok(())
}

#[derive(Debug)]
struct StoryModule {
    path: PathBuf,
    module_name: String,
    stories: Vec<StorySpec>,
}

impl StoryModule {
    fn read(src_dir: &Path, story_file: &Path, index: usize) -> Result<Self, String> {
        let source = fs::read_to_string(story_file)
            .map_err(|error| format!("could not read story file: {error}"))?;
        let parsed = syn::parse_file(&source)
            .map_err(|error| format!("Rust parse error before story discovery: {error}"))?;
        let path_info = StoryPathInfo::from_path(src_dir, story_file)?;
        let module_name = story_module_name(src_dir, story_file, index);

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
                family: path_info.family.clone(),
                category: path_info.category.clone(),
                component: path_info.component.clone(),
                story: story_segment,
                function_name: function.sig.ident.to_string(),
                label: metadata.label,
                description: metadata.description,
            });
        }

        if stories.is_empty() {
            return Err(format!(
                "story file matched `*_story.rs` but contains no `#[story(...)]` functions.\n\
                 Add functions like `#[story(label = \"...\", description = \"...\")] fn example() -> Element {{ ... }}`,\n\
                 or rename the file so it does not end with `_story.rs`."
            ));
        }

        Ok(Self {
            path: story_file.to_path_buf(),
            module_name,
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
                 Expected a story file under `src/ui_base`, `src/ui_core`, \
                 `src/ui_studio`, or `src/exploration`, using either \
                 `<component>_story.rs` or `<category>/<component>_story.rs`.",
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
}

impl StoryMetadata {
    fn from_attribute(attribute: &Attribute, function_name: &str) -> Result<Self, String> {
        let mut label = None;
        let mut description = None;
        let mut errors = Vec::new();

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

                let name = meta
                    .path
                    .get_ident()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "<unknown>".to_string());
                errors.push(format!(
                    "`{function_name}` uses unsupported #[story] argument `{name}`; \
                     expected `label = \"...\"` or `description = \"...\"`"
                ));
                Ok(())
            })
            .map_err(|error| {
                format!("could not parse #[story(...)] on `{function_name}`: {error}")
            })?;

        if !errors.is_empty() {
            return Err(errors.join("\n"));
        }

        let label = label
            .ok_or_else(|| format!("`{function_name}` is missing `label = \"...\"` in #[story]"))?;
        let description = description.ok_or_else(|| {
            format!("`{function_name}` is missing `description = \"...\"` in #[story]")
        })?;

        Ok(Self { label, description })
    }
}

#[derive(Debug)]
struct StorySpec {
    id: String,
    family: String,
    category: Option<String>,
    component: String,
    story: String,
    function_name: String,
    label: String,
    description: String,
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

fn generate_registry(story_modules: &[StoryModule]) -> String {
    let mut generated = String::new();
    generated.push_str("// @generated by lpa-studio-web/build.rs\n\n");

    for story_module in story_modules {
        generated.push_str("#[path = \"");
        generated.push_str(&rust_string_literal_path(&story_module.path));
        generated.push_str("\"]\n");
        generated.push_str("mod ");
        generated.push_str(&story_module.module_name);
        generated.push_str(";\n");
    }

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
            generated.push_str(&story_module.module_name);
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

fn is_story_attr(attribute: &Attribute) -> bool {
    attribute
        .path()
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "story")
}

fn story_family_from_source_root(source_root: &str) -> Result<String, String> {
    match source_root {
        "ui_base" => Ok("base".to_string()),
        "ui_core" => Ok("core".to_string()),
        "ui_studio" => Ok("studio".to_string()),
        "exploration" => Ok("exploration".to_string()),
        _ => Err(format!(
            "unsupported story source root `{source_root}`.\n\
             Component stories should live beside their components in `ui_base`, \
             `ui_core`, or `ui_studio`. Design spikes may live in `exploration`."
        )),
    }
}

fn component_from_story_file(file_name: &str) -> Result<String, String> {
    let Some(component) = file_name.strip_suffix("_story.rs") else {
        return Err(format!(
            "story file `{file_name}` should end with `_story.rs`"
        ));
    };
    if component.is_empty() {
        return Err(format!(
            "story file `{file_name}` must include a component name before `_story.rs`"
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

fn story_module_name(src_dir: &Path, story_file: &Path, index: usize) -> String {
    let relative = story_file
        .strip_prefix(src_dir)
        .expect("story file under src");
    let mut name = format!("story_{index}");
    for component in relative.components() {
        name.push('_');
        let segment = component.as_os_str().to_string_lossy();
        for ch in segment.chars() {
            if ch.is_ascii_alphanumeric() {
                name.push(ch.to_ascii_lowercase());
            } else {
                name.push('_');
            }
        }
    }
    name
}

fn rust_string_literal_path(path: &Path) -> String {
    rust_string_literal(&path.to_string_lossy())
}

fn rust_string_literal(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
