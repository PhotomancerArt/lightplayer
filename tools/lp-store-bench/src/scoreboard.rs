//! The scoreboard: append-only JSONL, one record per line, flushed per line so
//! a crash or a deadline loses nothing. Schema in the crate README.

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::Serialize;

/// Where records go. `Scoreboard::memory()` keeps them in RAM (tests).
pub struct Scoreboard {
    file: Option<Mutex<BufWriter<File>>>,
    memory: Mutex<Vec<serde_json::Value>>,
    keep_in_memory: bool,
    dir: Option<PathBuf>,
}

impl Scoreboard {
    /// Append to `<dir>/scoreboard.jsonl` (created if missing).
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(dir.join("scoreboard.jsonl"))?;
        Ok(Self {
            file: Some(Mutex::new(BufWriter::new(f))),
            memory: Mutex::new(Vec::new()),
            keep_in_memory: false,
            dir: Some(dir.into()),
        })
    }

    pub fn memory() -> Self {
        Self {
            file: None,
            memory: Mutex::new(Vec::new()),
            keep_in_memory: true,
            dir: None,
        }
    }

    pub fn dir(&self) -> Option<&Path> {
        self.dir.as_deref()
    }

    /// Write one record; `kind` becomes its `"type"` field.
    pub fn write<T: Serialize>(&self, kind: &str, record: &T) {
        let mut v = serde_json::to_value(record).expect("record serializes");
        if let serde_json::Value::Object(m) = &mut v {
            m.insert("type".into(), kind.into());
        }
        if let Some(f) = &self.file {
            let mut f = f.lock().unwrap();
            serde_json::to_writer(&mut *f, &v).expect("write scoreboard");
            f.write_all(b"\n").expect("write scoreboard");
            f.flush().expect("flush scoreboard");
        }
        if self.keep_in_memory {
            self.memory.lock().unwrap().push(v);
        }
    }

    pub fn records(&self) -> Vec<serde_json::Value> {
        self.memory.lock().unwrap().clone()
    }
}

/// Read every record of `<dir>/scoreboard.jsonl` (bad lines skipped).
pub fn read_scoreboard(dir: &Path) -> std::io::Result<Vec<serde_json::Value>> {
    let text = std::fs::read_to_string(dir.join("scoreboard.jsonl"))?;
    Ok(text
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect())
}
