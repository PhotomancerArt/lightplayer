//! File-sync operations: paginated changes-since, chunked writes, package
//! hash (roadmap M2b).
//!
//! Every operation is stateless per request (pull-model rule: no server-side
//! client state). Pagination determinism comes from bytewise path ordering;
//! a [`FileCursor`] identifies an exact resume point. Reads never consume or
//! clear the fs change log — [`crate::server`]'s own `advance_frame`
//! consumption is untouched.

extern crate alloc;

use alloc::format;
use alloc::string::ToString;
use alloc::vec::Vec;

use lpc_model::{FsVersion, LpPath, LpPathBuf};
use lpc_wire::budget::{
    FILE_SYNC_CHUNK_BYTES, FILE_SYNC_PAGE_MAX_ENTRIES, FILE_SYNC_PAGE_RAW_BYTES,
};
use lpc_wire::server::{FileChangeKind, FileChunk, FileCursor, FsResponse};
use lpfs::{FsEventKind, LpFs};

/// Handle `FsRequest::ChangesSince`: one page of the enumeration.
pub fn handle_changes_since(
    fs: &dyn LpFs,
    prefix: &LpPath,
    since: FsVersion,
    cursor: Option<FileCursor>,
) -> FsResponse {
    // capture before enumeration; clients adopt the FIRST page's version
    let version = fs.current_version();

    let mut items: Vec<(LpPathBuf, FileChangeKind)> = Vec::new();
    if since.as_i64() == 0 {
        // full enumeration: the change log need not cover pre-tracking files
        match fs.list_dir(prefix, true) {
            Ok(paths) => {
                for path in paths {
                    if fs.is_dir(path.as_path()).unwrap_or(false) {
                        continue;
                    }
                    if let Some(rel) = relativize(prefix, path.as_path()) {
                        items.push((rel, FileChangeKind::Upsert));
                    }
                }
            }
            Err(e) => return changes_error(format!("list_dir {}: {e}", prefix.as_str())),
        }
    } else {
        // `since` is the last revision the client has; strictly newer changes
        for event in fs.get_changes_since(since.next()) {
            if let Some(rel) = relativize(prefix, event.path.as_path()) {
                let kind = match event.kind {
                    FsEventKind::Delete => FileChangeKind::Delete,
                    _ => FileChangeKind::Upsert,
                };
                items.push((rel, kind));
            }
        }
    }
    items.sort_by(|a, b| a.0.as_str().as_bytes().cmp(b.0.as_str().as_bytes()));

    // resume: first item at or after the cursor path (a cursor for a path
    // that vanished mid-pagination degrades to "start at the next path")
    let (start_index, mut offset) = match &cursor {
        None => (0, 0usize),
        Some(c) => {
            let index = items
                .iter()
                .position(|(p, _)| p.as_str().as_bytes() >= c.path.as_str().as_bytes())
                .unwrap_or(items.len());
            let offset = if items.get(index).is_some_and(|(p, _)| p == &c.path) {
                c.offset as usize
            } else {
                0
            };
            (index, offset)
        }
    };

    let mut entries: Vec<FileChunk> = Vec::new();
    let mut raw_total = 0usize;
    let mut next: Option<FileCursor> = None;

    'items: for (path, kind) in items.iter().skip(start_index) {
        if entries.len() >= FILE_SYNC_PAGE_MAX_ENTRIES || raw_total >= FILE_SYNC_PAGE_RAW_BYTES {
            next = Some(FileCursor {
                path: path.clone(),
                offset: offset as u32,
            });
            break;
        }
        match kind {
            FileChangeKind::Delete => {
                entries.push(FileChunk {
                    path: path.clone(),
                    kind: FileChangeKind::Delete,
                    offset: 0,
                    total: 0,
                    data: Vec::new(),
                });
            }
            FileChangeKind::Upsert => {
                let absolute = join_prefix(prefix, path.as_path());
                let bytes = match fs.read_file(absolute.as_path()) {
                    Ok(bytes) => bytes,
                    // deleted between change-log read and file read: tombstone
                    Err(_) => {
                        entries.push(FileChunk {
                            path: path.clone(),
                            kind: FileChangeKind::Delete,
                            offset: 0,
                            total: 0,
                            data: Vec::new(),
                        });
                        offset = 0;
                        continue;
                    }
                };
                let total = bytes.len();
                if total == 0 {
                    entries.push(FileChunk {
                        path: path.clone(),
                        kind: FileChangeKind::Upsert,
                        offset: 0,
                        total: 0,
                        data: Vec::new(),
                    });
                } else {
                    while offset < total {
                        if entries.len() >= FILE_SYNC_PAGE_MAX_ENTRIES
                            || raw_total >= FILE_SYNC_PAGE_RAW_BYTES
                        {
                            next = Some(FileCursor {
                                path: path.clone(),
                                offset: offset as u32,
                            });
                            break 'items;
                        }
                        let take = FILE_SYNC_CHUNK_BYTES
                            .min(FILE_SYNC_PAGE_RAW_BYTES - raw_total)
                            .min(total - offset);
                        entries.push(FileChunk {
                            path: path.clone(),
                            kind: FileChangeKind::Upsert,
                            offset: offset as u32,
                            total: total as u32,
                            data: bytes[offset..offset + take].to_vec(),
                        });
                        raw_total += take;
                        offset += take;
                    }
                }
            }
        }
        offset = 0;
    }

    FsResponse::Changes {
        entries,
        next,
        version: Some(version),
        error: None,
    }
}

/// Handle `FsRequest::WriteChunk`: stateless chunked upload.
pub fn handle_write_chunk(
    fs: &mut dyn LpFs,
    path: LpPathBuf,
    offset: u32,
    data: &[u8],
) -> FsResponse {
    let result = if offset == 0 {
        fs.write_file(path.as_path(), data)
    } else {
        match fs.file_size(path.as_path()) {
            Ok(len) if len == u64::from(offset) => fs.append_file(path.as_path(), data),
            Ok(len) => {
                return FsResponse::WriteChunk {
                    path,
                    offset,
                    written: 0,
                    error: Some(format!(
                        "offset mismatch: file is {len} bytes, chunk at {offset}"
                    )),
                };
            }
            Err(e) => {
                return FsResponse::WriteChunk {
                    path,
                    offset,
                    written: 0,
                    error: Some(format!("{e}")),
                };
            }
        }
    };
    match result {
        Ok(()) => FsResponse::WriteChunk {
            path,
            offset,
            written: data.len() as u32,
            error: None,
        },
        Err(e) => FsResponse::WriteChunk {
            path,
            offset,
            written: 0,
            error: Some(format!("{e}")),
        },
    }
}

/// Handle `FsRequest::HashPackage`: canonical package hash of `prefix`.
pub fn handle_hash_package(fs: &dyn LpFs, prefix: LpPathBuf) -> FsResponse {
    let view = match fs.chroot(prefix.as_path()) {
        Ok(view) => view,
        Err(e) => {
            return FsResponse::PackageHash {
                prefix,
                hash: alloc::string::String::new(),
                error: Some(format!("{e}")),
            };
        }
    };
    let hash = {
        let view = view.borrow();
        lpc_history::hash_package(&*view)
    };
    match hash {
        Ok((hash, _manifest)) => FsResponse::PackageHash {
            prefix,
            hash: hash.to_string(),
            error: None,
        },
        Err(e) => FsResponse::PackageHash {
            prefix,
            hash: alloc::string::String::new(),
            error: Some(format!("{e}")),
        },
    }
}

/// Path under `prefix`, kept absolute (leading `/`), or `None` if outside.
fn relativize(prefix: &LpPath, path: &LpPath) -> Option<LpPathBuf> {
    let prefix_str = prefix.as_str().trim_end_matches('/');
    let path_str = path.as_str();
    let rest = path_str.strip_prefix(prefix_str)?;
    if rest.is_empty() {
        return None; // the prefix directory itself
    }
    if !rest.starts_with('/') {
        return None; // e.g. /projects/xy vs prefix /projects/x
    }
    Some(LpPathBuf::from(rest))
}

fn join_prefix(prefix: &LpPath, rel: &LpPath) -> LpPathBuf {
    LpPathBuf::from(format!(
        "{}{}",
        prefix.as_str().trim_end_matches('/'),
        rel.as_str()
    ))
}

fn changes_error(message: alloc::string::String) -> FsResponse {
    FsResponse::Changes {
        entries: Vec::new(),
        next: None,
        version: None,
        error: Some(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::AsLpPath;

    #[test]
    fn relativize_keeps_absolute_form_and_rejects_outside() {
        let prefix = "/projects/x".as_path();
        assert_eq!(
            relativize(prefix, "/projects/x/a/b.json".as_path())
                .unwrap()
                .as_str(),
            "/a/b.json"
        );
        assert!(relativize(prefix, "/projects/x".as_path()).is_none());
        assert!(relativize(prefix, "/projects/xy/a.json".as_path()).is_none());
        assert!(relativize(prefix, "/other/a.json".as_path()).is_none());
    }

    #[test]
    fn join_prefix_round_trips_relativize() {
        let prefix = "/projects/x".as_path();
        let rel = "/a/b.json".as_path();
        let joined = join_prefix(prefix, rel);
        assert_eq!(joined.as_str(), "/projects/x/a/b.json");
        assert_eq!(
            relativize(prefix, joined.as_path()).unwrap().as_str(),
            "/a/b.json"
        );
    }
}
