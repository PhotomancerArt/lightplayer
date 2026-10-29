//! A board as the project stage meets it: a minimal channel-1 responder over
//! a real filesystem — the tree store on simulated NOR flash (the C6's and
//! the S3's store), or memory (no transactions, like the classic's
//! littlefs). It answers the requests the stage's edge makes, with the
//! server's own path forms (`ListAvailableProjects` answers `projects/<name>`
//! under a relative base, `ListDir` lists files and directories), and drops
//! an open batch when its link goes, as `lpa-server`'s `BatchState` does.
//! A load is recorded, not run: the engine's half is `lpa-server`'s
//! `tests/old_startup_project.rs`.

use std::collections::BTreeMap;

use lp_nor_sim::{FaultPlan, NorFlashSim, NorGeometry};
use lp_tree_store::{LpFsTree, SoftSha256, StoreConfig, TreeStore};
use lpc_model::AsLpPath;
use lpfs::{LpFs, LpFsMemory};

/// The board's startup choice, at its filesystem's root.
pub const CONFIG: &str = "/lightplayer.json";

/// A request the link carried, or the link going.
pub type Answer<T> = Option<Result<T, String>>;

pub struct Board {
    fs: Box<dyn LpFs>,
    tree: Option<LpFsTree<NorFlashSim, SoftSha256>>,
    batch_open: bool,
    /// Requests answered on this link so far.
    pub requests: usize,
    /// Drop the link instead of answering request number `n` (0-based).
    pub drop_at: Option<usize>,
    /// Loads asked for, in order.
    pub loads: Vec<String>,
    /// The flash op count after each committed batch (tree boards).
    pub commits_at: Vec<u64>,
}

impl Board {
    /// A fresh tree store holding `files` (absolute paths).
    pub fn tree_with(files: &BTreeMap<String, Vec<u8>>) -> Self {
        let mut flash = NorFlashSim::new(NorGeometry::c6(64));
        TreeStore::format(&mut flash, SoftSha256, StoreConfig::default())
            .map_err(|(e, ..)| e)
            .expect("format");
        let board = Self::tree(flash);
        for (path, bytes) in files {
            board.fs.write_file(path.as_str().as_path(), bytes).unwrap();
        }
        board
    }

    /// The tree store on `flash`, mounted.
    pub fn tree(flash: NorFlashSim) -> Self {
        let tree = LpFsTree::new(mount(flash));
        let handle = tree.handle();
        Self::over(Box::new(tree), Some(handle))
    }

    /// A memory filesystem holding `files`.
    pub fn memory_with(files: &BTreeMap<String, Vec<u8>>) -> Self {
        let fs = LpFsMemory::new();
        for (path, bytes) in files {
            fs.write_file(path.as_str().as_path(), bytes).unwrap();
        }
        Self::over(Box::new(fs), None)
    }

    fn over(fs: Box<dyn LpFs>, tree: Option<LpFsTree<NorFlashSim, SoftSha256>>) -> Self {
        Self {
            fs,
            tree,
            batch_open: false,
            requests: 0,
            drop_at: None,
            loads: Vec::new(),
            commits_at: Vec::new(),
        }
    }

    // --- channel 1 -------------------------------------------------------------

    /// `ListAvailableProjects`: every `/projects/<name>` holding a
    /// `project.json`, as `projects/<name>`.
    pub fn list_projects(&mut self) -> Answer<Vec<String>> {
        self.carry(|fs| {
            let entries = fs
                .list_dir("/projects".as_path(), false)
                .map_err(|e| e.to_string())?;
            Ok(entries
                .into_iter()
                .filter(|entry| {
                    fs.file_exists(entry.join("project.json").as_path())
                        .unwrap_or(false)
                })
                .map(|entry| {
                    format!(
                        "projects/{}",
                        entry
                            .as_str()
                            .trim_end_matches('/')
                            .rsplit('/')
                            .next()
                            .unwrap()
                    )
                })
                .collect())
        })
    }

    pub fn read(&mut self, path: &str) -> Answer<Vec<u8>> {
        self.carry(|fs| fs.read_file(path.as_path()).map_err(|e| e.to_string()))
    }

    /// `ListDir` recursive: files and directories, as `lpfs` lists them.
    pub fn list_dir(&mut self, path: &str) -> Answer<Vec<String>> {
        self.carry(|fs| {
            fs.list_dir(path.as_path(), true)
                .map(|entries| entries.iter().map(|e| String::from(e.as_str())).collect())
                .map_err(|e| e.to_string())
        })
    }

    /// `BeginBatch`: `Ok(atomic)`; a board with no transactions opens none.
    pub fn begin_batch(&mut self) -> Answer<bool> {
        let answer = self.carry(|fs| {
            if !fs.batches_are_atomic() {
                return Ok(false);
            }
            fs.begin_batch().map_err(|e| e.to_string())?;
            Ok(true)
        });
        if let Some(Ok(true)) = answer {
            self.batch_open = true;
        }
        answer
    }

    pub fn write(&mut self, path: &str, bytes: &[u8]) -> Answer<()> {
        self.carry(|fs| {
            fs.write_file(path.as_path(), bytes)
                .map_err(|e| e.to_string())
        })
    }

    pub fn delete(&mut self, path: &str) -> Answer<()> {
        self.carry(|fs| fs.delete_file(path.as_path()).map_err(|e| e.to_string()))
    }

    pub fn commit_batch(&mut self) -> Answer<()> {
        let answer = self.carry(|fs| fs.commit_batch().map_err(|e| e.to_string()));
        if answer.is_some() {
            self.batch_open = false;
        }
        if let Some(Ok(())) = answer {
            if let Some(tree) = &self.tree {
                let ops = tree.with_store(|store| store.flash().ops_since_plan());
                self.commits_at.push(ops);
            }
        }
        answer
    }

    pub fn abort_batch(&mut self) -> Answer<()> {
        let answer = self.carry(|fs| fs.abort_batch().map_err(|e| e.to_string()));
        if answer.is_some() {
            self.batch_open = false;
        }
        answer
    }

    pub fn load(&mut self, path: &str) -> Answer<()> {
        let path = String::from(path);
        let answer = self.carry(|_| Ok(()));
        if let Some(Ok(())) = answer {
            self.loads.push(path);
        }
        answer
    }

    /// The link went: an open batch is dropped (the server's
    /// `BatchState::link_ended`), and a new link starts counting afresh.
    pub fn new_link(&mut self) {
        if self.batch_open {
            let _ = self.fs.abort_batch();
            self.batch_open = false;
        }
        self.requests = 0;
        self.drop_at = None;
    }

    fn carry<T>(&mut self, f: impl FnOnce(&dyn LpFs) -> Result<T, String>) -> Answer<T> {
        if self.drop_at == Some(self.requests) {
            return None;
        }
        self.requests += 1;
        Some(f(&*self.fs))
    }

    // --- the test's view -------------------------------------------------------

    /// Every file under `/projects/` and the startup choice.
    pub fn snapshot(&self) -> BTreeMap<String, Vec<u8>> {
        snapshot(&*self.fs)
    }

    pub fn flash(&self) -> NorFlashSim {
        self.tree_store(|store| store.flash().clone())
    }

    pub fn set_plan(&self, plan: FaultPlan) {
        self.tree_store(|store| store.flash_mut().set_plan(plan));
    }

    pub fn ops_since_plan(&self) -> u64 {
        self.tree_store(|store| store.flash().ops_since_plan())
    }

    fn tree_store<T>(&self, f: impl FnOnce(&mut TreeStore<NorFlashSim, SoftSha256>) -> T) -> T {
        self.tree.as_ref().expect("a tree board").with_store(f)
    }
}

/// Every file under `/projects/` and the startup choice, on `fs`.
pub fn snapshot(fs: &dyn LpFs) -> BTreeMap<String, Vec<u8>> {
    let mut out = BTreeMap::new();
    let entries = fs.list_dir("/projects".as_path(), true).unwrap_or_default();
    for entry in entries {
        if fs.is_dir(entry.as_path()).unwrap_or(false) {
            continue;
        }
        let bytes = fs.read_file(entry.as_path()).expect("a listed file reads");
        out.insert(String::from(entry.as_str()), bytes);
    }
    if let Ok(bytes) = fs.read_file(CONFIG.as_path()) {
        out.insert(String::from(CONFIG), bytes);
    }
    out
}

/// The files of one project (`/projects/<name>/…`) in a snapshot.
pub fn project_of(snapshot: &BTreeMap<String, Vec<u8>>, name: &str) -> BTreeMap<String, Vec<u8>> {
    let prefix = format!("/projects/{name}/");
    snapshot
        .iter()
        .filter(|(path, _)| path.starts_with(&prefix))
        .map(|(path, bytes)| (path.clone(), bytes.clone()))
        .collect()
}

pub fn mount(flash: NorFlashSim) -> TreeStore<NorFlashSim, SoftSha256> {
    match TreeStore::mount(flash, SoftSha256, StoreConfig::default()) {
        Ok(store) => store,
        Err((error, _, _)) => panic!("mount: {error:?}"),
    }
}
