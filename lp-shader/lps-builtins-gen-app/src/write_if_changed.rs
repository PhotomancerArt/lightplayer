//! Write a generated Rust source file only when its formatted text differs from disk.
//!
//! The generators emit raw text. Writing it as-is and formatting afterwards
//! rewrites every file with identical bytes on every run, which bumps mtimes
//! and makes cargo rebuild `lps-builtin-ids` and everything above it. So the
//! text is formatted in memory (`rustfmt` on stdin, same toolchain and edition
//! as `cargo fmt`), compared with the file, and written only on a difference.

use std::fs;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

/// Edition of the workspace (`edition = "2024"` in the root `Cargo.toml`);
/// `cargo fmt` passes it to rustfmt, stdin input has to be told.
const EDITION: &str = "2024";

/// Format `unformatted` with rustfmt, then write it to `path` unless the file
/// already holds exactly those bytes. Creates the parent directory if needed.
pub(crate) fn write_if_changed(path: &Path, unformatted: &str) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)?;

    // Run inside the repo so rustup picks the pinned toolchain and rustfmt
    // finds the repo's config, exactly as `cargo fmt` from the root does.
    let formatted = format_with_rustfmt(parent, unformatted)?;

    if fs::read(path).is_ok_and(|current| current == formatted.as_bytes()) {
        return Ok(());
    }
    fs::write(path, formatted)
}

fn format_with_rustfmt(cwd: &Path, source: &str) -> io::Result<String> {
    let mut child = Command::new("rustfmt")
        .args(["--edition", EDITION, "--emit", "stdout"])
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let mut stdin = child.stdin.take().expect("rustfmt stdin is piped");
    let input = source.to_owned();
    let writer = std::thread::spawn(move || stdin.write_all(input.as_bytes()));

    let output = child.wait_with_output()?;
    writer
        .join()
        .map_err(|_| io::Error::other("rustfmt stdin writer panicked"))??;

    if !output.status.success() {
        return Err(io::Error::other(format!(
            "rustfmt failed on generated source for {}: {}",
            cwd.display(),
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    String::from_utf8(output.stdout).map_err(io::Error::other)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_formatted_text_then_leaves_an_identical_file_alone() {
        let dir = std::env::temp_dir().join(format!("lp-write-if-changed-{}", std::process::id()));
        let path = dir.join("generated.rs");
        let _ = fs::remove_dir_all(&dir);

        write_if_changed(&path, "fn  a( ){ }\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "fn a() {}\n");
        let first = fs::metadata(&path).unwrap().modified().unwrap();

        // Same content, spelled unformatted: nothing is rewritten.
        std::thread::sleep(std::time::Duration::from_millis(50));
        write_if_changed(&path, "fn  a( ){ }\n").unwrap();
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), first);

        // Different content is written.
        write_if_changed(&path, "fn b(){}\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "fn b() {}\n");

        fs::remove_dir_all(&dir).unwrap();
    }
}
