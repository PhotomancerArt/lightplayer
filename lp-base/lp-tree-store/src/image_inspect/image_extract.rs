//! [`StoreImage::extract`]: the committed tree's files as bytes, for
//! recovery and backups. It reads the trusted sectors only, so it works on
//! an image the store itself refuses to mount (a sector at a newer format
//! version): the caller reports the odd sector.

use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::image_report::EntryKindReport;
use super::store_image::StoreImage;

/// A file read out of the committed tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtractedFile {
    /// Absolute store path (`/projects/a/project.json`).
    pub path: String,
    pub bytes: Vec<u8>,
}

/// A file the extraction did not produce, and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkippedEntry {
    pub path: String,
    pub why: String,
}

/// What [`StoreImage::extract`] read.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Extraction {
    pub files: Vec<ExtractedFile>,
    /// Files in the tree that could not be written out safely or could not
    /// be read.
    pub skipped: Vec<SkippedEntry>,
    /// Files written out that `check` would also question.
    pub warnings: Vec<String>,
}

impl<'a> StoreImage<'a> {
    /// Every file of the committed tree. `Err` when the image holds no
    /// complete root to read. A name that would leave an output directory
    /// (empty, `.`, `..`, containing `/` or NUL, not UTF-8) is skipped, as
    /// is a file whose path is also a directory with files in it.
    pub fn extract(&self) -> Result<Extraction, &'static str> {
        if self.report.chosen.is_none() {
            return Err("the image holds no complete root to read");
        }
        let files: Vec<_> = self
            .report
            .tree
            .iter()
            .filter(|e| e.kind == EntryKindReport::File)
            .collect();
        // Every directory some file lives under.
        let mut dirs: BTreeSet<&str> = BTreeSet::new();
        for f in &files {
            let mut p = f.path.as_str();
            while let Some(i) = p.rfind('/') {
                p = &p[..i];
                if !p.is_empty() {
                    dirs.insert(p);
                }
            }
        }
        let mut out = Extraction::default();
        let mut seen: BTreeSet<&str> = BTreeSet::new();
        for f in files {
            let skip = |why: String| SkippedEntry {
                path: f.path.clone(),
                why,
            };
            if let Some(why) = f.name_problem {
                out.skipped.push(skip(format!("{why}; not written")));
                continue;
            }
            if f.path.split('/').skip(1).any(unsafe_component) {
                out.skipped.push(skip(
                    "a path component that could leave the output directory; not written".into(),
                ));
                continue;
            }
            if dirs.contains(f.path.as_str()) {
                out.skipped.push(skip(
                    "a directory of the same name holds other files; not written".into(),
                ));
                continue;
            }
            if !seen.insert(f.path.as_str()) {
                out.skipped
                    .push(skip("the path is already extracted from another entry".into()));
                continue;
            }
            match self.node_bytes(f.id) {
                Ok(bytes) => {
                    if bytes.len() != f.size as usize {
                        out.warnings.push(format!(
                            "{}: the entry says {} bytes, the node holds {}",
                            f.path,
                            f.size,
                            bytes.len()
                        ));
                    }
                    out.files.push(ExtractedFile {
                        path: f.path.clone(),
                        bytes,
                    });
                }
                Err(why) => out.skipped.push(skip(format!("unreadable: {why}"))),
            }
        }
        Ok(out)
    }
}

fn unsafe_component(c: &str) -> bool {
    c.is_empty() || c == "." || c == ".." || c.contains('\0') || c.contains('\\')
}
