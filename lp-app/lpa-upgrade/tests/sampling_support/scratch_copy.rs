//! A throwaway copy of a project with one playlist starting on another
//! entry, so every entry's shader is sampled by the live engine.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static NEXT: AtomicU32 = AtomicU32::new(0);

/// A copy of `project` in the temp directory, deleted on drop.
pub struct ScratchCopy {
    pub dir: PathBuf,
}

impl ScratchCopy {
    /// Copy `project`, then start the playlist defined in `playlist_file`
    /// (from the project root) on entry `entry`.
    pub fn with_idle_entry(project: &Path, playlist_file: &str, entry: u64) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "lpa-upgrade-v12-sampling-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        copy_dir(project, &dir);
        // The temp directory is behind a symlink on macOS (/var → /private/var).
        let dir = dir.canonicalize().unwrap();
        let path = dir.join(playlist_file);
        // Edit the one number in place: the file keeps its own form.
        let text = std::fs::read_to_string(&path).unwrap();
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        let old = json["idle_entry"]
            .as_u64()
            .expect("the playlist names its idle entry");
        let edited = text.replacen(
            &format!("\"idle_entry\": {old}"),
            &format!("\"idle_entry\": {entry}"),
            1,
        );
        assert!(
            old == entry || edited != text,
            "{playlist_file}: idle_entry rewritten"
        );
        std::fs::write(&path, edited).unwrap();
        Self { dir }
    }
}

impl Drop for ScratchCopy {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), target).unwrap();
        }
    }
}
