//! Mutation plan: collect, preview, confirm, and apply file mutations.

use crate::colors;
use crate::targets::Target;
use crate::util::file_update::FileUpdate;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

/// Extract just the run content (expression) without the "// run:" prefix.
fn extract_run_content(lines: &[String], line: usize) -> String {
    if let Some(line_content) = lines.get(line - 1) {
        let trimmed = line_content.trim();
        if let Some(run_body) = trimmed.strip_prefix("// run:") {
            return run_body.trim().to_string();
        }
    }
    String::new()
}

/// Extract the annotation kind (e.g., "@unimplemented") from an annotation string.
fn extract_kind_from_annotation(annotation: &str) -> Option<&str> {
    // annotation format: "// @kind(target)"
    let rest = annotation.strip_prefix("// @")?;
    let paren = rest.find('(')?;
    Some(&rest[..paren])
}

/// Extract the target name from an annotation string.
fn extract_target_from_annotation(annotation: &str) -> &str {
    // annotation format: "// @kind(target)"
    if let Some(start) = annotation.find('@') {
        if let Some(paren) = annotation[start..].find('(') {
            let after_kind = start + paren + 1;
            if let Some(end) = annotation[after_kind..].find(')') {
                return &annotation[after_kind..after_kind + end];
            }
        }
    }
    ""
}

/// Get the display color for an annotation kind.
fn annotation_color(kind: Option<&str>) -> &'static str {
    match kind {
        Some("broken") => colors::RED,
        Some("unimplemented") => colors::YELLOW,
        Some("unsupported") => colors::DIM,
        _ => colors::YELLOW,
    }
}

/// A single mutation action to be applied to a file.
#[derive(Debug, Clone)]
pub enum MutationAction {
    /// Add an annotation before a run directive.
    AddAnnotation {
        /// Path to the file to modify.
        path: PathBuf,
        /// Line number of the run directive.
        line: usize,
        /// The annotation to add (e.g., "// @unimplemented(rv32n.q32)").
        annotation: String,
    },
    /// Remove an annotation matching a target.
    RemoveAnnotation {
        /// Path to the file to modify.
        path: PathBuf,
        /// Line number of the run directive.
        line: usize,
        /// Target name to match (e.g., "rv32n.q32").
        target_name: String,
    },
}

impl MutationAction {
    /// Get the path for this action.
    fn path(&self) -> &PathBuf {
        match self {
            MutationAction::AddAnnotation { path, .. } => path,
            MutationAction::RemoveAnnotation { path, .. } => path,
        }
    }

    /// Get the line number for this action.
    fn line(&self) -> usize {
        match self {
            MutationAction::AddAnnotation { line, .. } => *line,
            MutationAction::RemoveAnnotation { line, .. } => *line,
        }
    }
}

/// A plan for applying mutations to files.
#[derive(Debug, Default)]
pub struct MutationPlan {
    /// Actions to be applied.
    pub actions: Vec<MutationAction>,
}

impl MutationPlan {
    /// Create a new empty plan.
    pub fn new() -> Self {
        Self {
            actions: Vec::new(),
        }
    }

    /// Check if the plan is empty.
    pub fn is_empty(&self) -> bool {
        self.actions.is_empty()
    }

    /// Get the number of actions.
    pub fn len(&self) -> usize {
        self.actions.len()
    }

    /// Add an action to the plan.
    pub fn push(&mut self, action: MutationAction) {
        self.actions.push(action);
    }

    /// Add multiple actions to the plan.
    pub fn extend(&mut self, actions: impl IntoIterator<Item = MutationAction>) {
        self.actions.extend(actions);
    }

    /// Print a preview of what would be done.
    pub fn preview(&self, filetests_dir: &Path) {
        if self.actions.is_empty() {
            println!("No mutations to apply.");
            return;
        }

        // Sort actions by path then line
        let mut sorted: Vec<_> = self.actions.iter().collect();
        sorted.sort_by_key(|a| (a.path(), a.line()));

        // Group by action type
        let mut add_actions: Vec<&MutationAction> = Vec::new();
        let mut remove_actions: Vec<&MutationAction> = Vec::new();

        for action in &sorted {
            match action {
                MutationAction::AddAnnotation { .. } => add_actions.push(action),
                MutationAction::RemoveAnnotation { .. } => remove_actions.push(action),
            }
        }

        // Cache file contents for run line context (using PathBuf as key)
        let mut file_contents: BTreeMap<PathBuf, Vec<String>> = BTreeMap::new();
        for action in &sorted {
            let path = action.path();
            if !file_contents.contains_key(path) {
                if let Ok(content) = std::fs::read_to_string(path) {
                    let lines: Vec<String> = content.lines().map(String::from).collect();
                    file_contents.insert(path.clone(), lines);
                }
            }
        }

        // Determine the annotation kind and color for display
        let ann_kind = self.annotation_kind();
        let ann_color = annotation_color(ann_kind);
        let ann_name = ann_kind.map_or("annotation", |k| k);

        if !remove_actions.is_empty() {
            let unique_files: BTreeSet<_> = remove_actions.iter().map(|a| a.path()).collect();
            let header = if colors::should_color() {
                format!(
                    "Ready to remove {}{}{} from {} file(s)?",
                    ann_color,
                    ann_name,
                    colors::RESET,
                    unique_files.len()
                )
            } else {
                format!(
                    "Ready to remove {} from {} file(s)?",
                    ann_name,
                    unique_files.len()
                )
            };
            println!("{header}");

            for action in &remove_actions {
                if let MutationAction::RemoveAnnotation {
                    path,
                    line,
                    target_name,
                } = action
                {
                    let rel_path = path.strip_prefix(filetests_dir).unwrap_or(path);
                    let run_content = self.get_run_content(path, *line, &file_contents);
                    if colors::should_color() {
                        println!(
                            "  {:12}  {}:{}  {}",
                            target_name,
                            colors::colorize(&rel_path.display().to_string(), colors::DIM),
                            line,
                            colors::colorize(&run_content, colors::GREEN),
                        );
                    } else {
                        println!(
                            "  {:12}  {}:{}  {}",
                            target_name,
                            rel_path.display(),
                            line,
                            run_content
                        );
                    }
                }
            }
        }

        if !add_actions.is_empty() {
            let unique_files: BTreeSet<_> = add_actions.iter().map(|a| a.path()).collect();
            if !remove_actions.is_empty() {
                println!();
            }
            let header = if colors::should_color() {
                format!(
                    "Ready to add {}{}{} to {} file(s)?",
                    ann_color,
                    ann_name,
                    colors::RESET,
                    unique_files.len()
                )
            } else {
                format!(
                    "Ready to add {} to {} file(s)?",
                    ann_name,
                    unique_files.len()
                )
            };
            println!("{header}");

            for action in &add_actions {
                if let MutationAction::AddAnnotation {
                    path,
                    line,
                    annotation,
                } = action
                {
                    let rel_path = path.strip_prefix(filetests_dir).unwrap_or(path);
                    let run_content = self.get_run_content(path, *line, &file_contents);
                    let target_name = extract_target_from_annotation(annotation);
                    if colors::should_color() {
                        println!(
                            "  {:12}  {}:{}  {}",
                            target_name,
                            colors::colorize(&rel_path.display().to_string(), colors::DIM),
                            line,
                            colors::colorize(&run_content, colors::GREEN),
                        );
                    } else {
                        println!(
                            "  {:12}  {}:{}  {}",
                            target_name,
                            rel_path.display(),
                            line,
                            run_content
                        );
                    }
                }
            }
        }
    }

    /// Determine the annotation kind from the first action (for header coloring).
    fn annotation_kind(&self) -> Option<&str> {
        self.actions.first().and_then(|a| match a {
            MutationAction::AddAnnotation { annotation, .. } => {
                extract_kind_from_annotation(annotation)
            }
            MutationAction::RemoveAnnotation { .. } => Some("@unimplemented"),
        })
    }

    /// Get just the run content (expression) for the line.
    fn get_run_content(
        &self,
        path: &Path,
        line: usize,
        file_contents: &BTreeMap<PathBuf, Vec<String>>,
    ) -> String {
        if let Some(lines) = file_contents.get(path) {
            extract_run_content(lines, line)
        } else {
            String::new()
        }
    }

    /// Prompt the user for confirmation.
    pub fn confirm_with_user(&self) -> anyhow::Result<bool> {
        print!("Type 'yes' to apply: ");
        std::io::stdout().flush()?;

        let mut confirmation = String::new();
        std::io::stdin().read_line(&mut confirmation)?;

        Ok(confirmation.trim() == "yes")
    }

    /// Apply all mutations and return the count of applied actions.
    pub fn apply(&self, filetests_dir: &Path) -> anyhow::Result<usize> {
        if self.actions.is_empty() {
            return Ok(0);
        }

        // Determine annotation kind and color for consistent display
        let ann_kind = self.annotation_kind();
        let ann_color = annotation_color(ann_kind);
        let ann_name = ann_kind.map_or("annotation", |k| k);

        // Group actions by file path (sorted)
        let mut by_file: BTreeMap<&Path, Vec<&MutationAction>> = BTreeMap::new();
        for action in &self.actions {
            by_file.entry(action.path()).or_default().push(action);
        }

        // Sort actions within each file by line number (ascending)
        for actions in by_file.values_mut() {
            actions.sort_by_key(|a| a.line());
        }

        let mut applied_count = 0usize;
        let mut modified_files: BTreeSet<&Path> = BTreeSet::new();

        // Process each file
        for (path, actions) in by_file {
            let rel_path = path.strip_prefix(filetests_dir).unwrap_or(path);

            // Cache file contents for run line context
            let file_content = std::fs::read_to_string(path).ok();
            let file_lines: Option<Vec<String>> = file_content
                .as_ref()
                .map(|c| c.lines().map(String::from).collect());

            let file_update = FileUpdate::new(path);

            // Group actions by line number for batch removal
            let mut i = 0;
            while i < actions.len() {
                let action = actions[i];
                let line = action.line();

                // Check if this is a remove action
                if let MutationAction::RemoveAnnotation { target_name, .. } = action {
                    // Collect all RemoveAnnotation actions at this line
                    let mut targets: Vec<&Target> = Vec::new();
                    let mut target_names: Vec<&str> = Vec::new();

                    // Resolve the first target
                    match Target::from_name(target_name) {
                        Ok(t) => {
                            targets.push(t);
                            target_names.push(target_name);
                        }
                        Err(e) => {
                            eprintln!(
                                "  Warning: unknown target '{}' at {}:{}: {}",
                                target_name,
                                rel_path.display(),
                                line,
                                e
                            );
                            i += 1;
                            continue;
                        }
                    }

                    // Collect remaining RemoveAnnotation actions at the same line
                    i += 1;
                    while i < actions.len() {
                        let next = actions[i];
                        if let MutationAction::RemoveAnnotation {
                            line: next_line,
                            target_name: next_target_name,
                            ..
                        } = next
                        {
                            if *next_line == line {
                                match Target::from_name(next_target_name) {
                                    Ok(t) => {
                                        targets.push(t);
                                        target_names.push(next_target_name);
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "  Warning: unknown target '{}' at {}:{}: {}",
                                            next_target_name,
                                            rel_path.display(),
                                            line,
                                            e
                                        );
                                    }
                                }
                                i += 1;
                                continue;
                            }
                        }
                        break;
                    }

                    // Get run content for display
                    let run_content = if let Some(ref lines) = file_lines {
                        extract_run_content(lines, line)
                    } else {
                        String::new()
                    };

                    // Batch remove all targets at this line
                    match file_update.remove_annotations_matching_targets(line, &targets) {
                        Ok(removed_count) if removed_count > 0 => {
                            // Print one line per removed target
                            for target_name in target_names {
                                if colors::should_color() {
                                    println!(
                                        "  {:12}  {}:{}  {}{}  {}",
                                        target_name,
                                        colors::colorize(
                                            &rel_path.display().to_string(),
                                            colors::DIM
                                        ),
                                        line,
                                        colors::colorize(&run_content, colors::GREEN),
                                        colors::RESET,
                                        colors::colorize(&format!("{ann_name} removed"), ann_color)
                                    );
                                } else {
                                    println!(
                                        "  {:12}  {}:{}  {}  {} removed",
                                        target_name,
                                        rel_path.display(),
                                        line,
                                        run_content,
                                        ann_name
                                    );
                                }
                                applied_count += 1;
                            }
                            modified_files.insert(path);
                        }
                        Ok(_) => {
                            // Nothing removed - skip silently
                        }
                        Err(e) => {
                            eprintln!(
                                "  Warning: failed to remove annotations at {}:{}: {}",
                                rel_path.display(),
                                line,
                                e
                            );
                        }
                    }
                } else if let MutationAction::AddAnnotation { annotation, .. } = action {
                    let target_name = extract_target_from_annotation(annotation);
                    let run_content = if let Some(ref lines) = file_lines {
                        extract_run_content(lines, line)
                    } else {
                        String::new()
                    };

                    match file_update.add_annotation(line, annotation) {
                        Ok(()) => {
                            if colors::should_color() {
                                println!(
                                    "  {:12}  {}:{}  {}{}",
                                    target_name,
                                    colors::colorize(&rel_path.display().to_string(), colors::DIM),
                                    line,
                                    colors::colorize(&run_content, colors::GREEN),
                                    colors::RESET
                                );
                            } else {
                                println!(
                                    "  {:12}  {}:{}  {}",
                                    target_name,
                                    rel_path.display(),
                                    line,
                                    run_content
                                );
                            }
                            applied_count += 1;
                            modified_files.insert(path);
                        }
                        Err(e) => {
                            eprintln!(
                                "  Warning: failed to add annotation at {}:{}: {}",
                                rel_path.display(),
                                line,
                                e
                            );
                        }
                    }
                    i += 1;
                } else {
                    i += 1;
                }
            }
        }

        // Print summary
        if applied_count > 0 {
            let summary = format!(
                "Applied {} mutation(s) across {} file(s). Re-run filetests to verify.",
                applied_count,
                modified_files.len()
            );
            println!("{}", colors::colorize(&summary, colors::GREEN));
        }

        Ok(applied_count)
    }
}
