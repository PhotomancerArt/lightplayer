//! File-sync client operations (roadmap M2b): request builders and
//! response folds for `ChangesSince` pulls, chunked pushes, and package
//! hashing. Pure and runtime-neutral — `LpClient` drives them.

use lpc_model::{AsLpPathBuf, FsVersion};
use lpc_wire::{
    ClientRequest, FsRequest, WireServerMsgBody,
    budget::FILE_SYNC_CHUNK_BYTES,
    server::{FileChangeKind, FileCursor, FsResponse},
};

use crate::client_error::{ClientError, ClientResult};
use crate::project_deploy::project_file_path;

/// One reassembled file change from a `ChangesSince` pull.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileUpdate {
    /// Path relative to the project prefix (leading `/`).
    pub path: String,
    /// `Some(bytes)` for an upsert, `None` for a delete tombstone.
    pub content: Option<Vec<u8>>,
}

/// Build one page request of a `ChangesSince` pull.
pub fn changes_since_request(
    project_id: &str,
    since: FsVersion,
    cursor: Option<FileCursor>,
) -> ClientRequest {
    ClientRequest::Filesystem(FsRequest::ChangesSince {
        prefix: format!("/projects/{project_id}").as_str().as_path_buf(),
        since,
        cursor,
    })
}

/// Build the requests to write one file, chunking when it exceeds a frame's
/// raw chunk budget. Single-chunk files use the plain `Write` form.
pub fn file_write_requests(
    project_id: &str,
    relative_path: &str,
    bytes: &[u8],
) -> Vec<ClientRequest> {
    let path = project_file_path(project_id, relative_path);
    if bytes.len() <= FILE_SYNC_CHUNK_BYTES {
        return vec![ClientRequest::Filesystem(FsRequest::Write {
            path: path.as_str().as_path_buf(),
            data: bytes.to_vec(),
        })];
    }
    bytes
        .chunks(FILE_SYNC_CHUNK_BYTES)
        .enumerate()
        .map(|(index, chunk)| {
            ClientRequest::Filesystem(FsRequest::WriteChunk {
                path: path.as_str().as_path_buf(),
                offset: (index * FILE_SYNC_CHUNK_BYTES) as u32,
                data: chunk.to_vec(),
            })
        })
        .collect()
}

/// Build the package-hash request for a project.
pub fn hash_package_request(project_id: &str) -> ClientRequest {
    ClientRequest::Filesystem(FsRequest::HashPackage {
        prefix: format!("/projects/{project_id}").as_str().as_path_buf(),
    })
}

/// Extract the hash from a `HashPackage` response.
pub fn validate_hash_package_response(response: &WireServerMsgBody) -> ClientResult<String> {
    match response {
        WireServerMsgBody::Filesystem(FsResponse::PackageHash {
            hash, error: None, ..
        }) => Ok(hash.clone()),
        WireServerMsgBody::Filesystem(FsResponse::PackageHash { error: Some(e), .. }) => {
            Err(ClientError::Server(format!("hash package failed: {e}")))
        }
        other => Err(ClientError::unexpected_response("fs.hash_package", other)),
    }
}

/// Fold for a paginated `ChangesSince` pull: reassembles chunks in order and
/// captures the **first** page's version (the client's next `since` — changes
/// landing mid-pagination re-surface on the following pull).
#[derive(Debug, Default)]
pub struct ChangesCollector {
    updates: Vec<FileUpdate>,
    version: Option<FsVersion>,
}

impl ChangesCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Fold one page. Returns the cursor for the next page, or `None` when
    /// the enumeration is complete.
    pub fn accept(&mut self, response: &WireServerMsgBody) -> ClientResult<Option<FileCursor>> {
        let WireServerMsgBody::Filesystem(FsResponse::Changes {
            entries,
            next,
            version,
            error,
        }) = response
        else {
            return Err(ClientError::unexpected_response(
                "fs.changes_since",
                response,
            ));
        };
        if let Some(error) = error {
            return Err(ClientError::Server(format!(
                "changes since failed: {error}"
            )));
        }
        if self.version.is_none() {
            self.version =
                Some(version.ok_or_else(|| {
                    ClientError::Protocol("changes page missing fs version".into())
                })?);
        }
        for chunk in entries {
            let path = chunk.path.as_str();
            match chunk.kind {
                FileChangeKind::Delete => {
                    self.upsert_entry(path, None)?;
                }
                FileChangeKind::Upsert => {
                    let continuation = chunk.offset > 0;
                    if continuation {
                        let Some(FileUpdate {
                            content: Some(buffer),
                            ..
                        }) = self.updates.last_mut().filter(|u| u.path == path)
                        else {
                            return Err(ClientError::Protocol(format!(
                                "chunk continuation for {path} without a preceding chunk"
                            )));
                        };
                        if buffer.len() != chunk.offset as usize {
                            return Err(ClientError::Protocol(format!(
                                "out-of-order chunk for {path}: have {} bytes, chunk at {}",
                                buffer.len(),
                                chunk.offset
                            )));
                        }
                        buffer.extend_from_slice(&chunk.data);
                    } else {
                        self.upsert_entry(path, Some(chunk.data.clone()))?;
                    }
                }
            }
        }
        Ok(next.clone())
    }

    /// Finish the fold, yielding the reassembled updates and the version to
    /// use as the next pull's `since`.
    pub fn finish(self) -> ClientResult<(Vec<FileUpdate>, FsVersion)> {
        let version = self
            .version
            .ok_or_else(|| ClientError::Protocol("changes pull produced no pages".into()))?;
        Ok((self.updates, version))
    }

    fn upsert_entry(&mut self, path: &str, content: Option<Vec<u8>>) -> ClientResult<()> {
        // enumeration is path-ordered and latest-per-path; duplicates mean a
        // protocol violation except for the cursor-resume overlap case where
        // the same path restarts — replace, last writer wins
        if let Some(existing) = self.updates.iter_mut().find(|u| u.path == path) {
            existing.content = content;
        } else {
            self.updates.push(FileUpdate {
                path: path.to_string(),
                content,
            });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_model::AsLpPathBuf;
    use lpc_wire::server::FileChunk;

    fn changes_body(
        entries: Vec<FileChunk>,
        next: Option<FileCursor>,
        version: Option<i64>,
    ) -> WireServerMsgBody {
        WireServerMsgBody::Filesystem(FsResponse::Changes {
            entries,
            next,
            version: version.map(FsVersion::new),
            error: None,
        })
    }

    fn upsert(path: &str, offset: u32, total: u32, data: &[u8]) -> FileChunk {
        FileChunk {
            path: path.as_path_buf(),
            kind: FileChangeKind::Upsert,
            offset,
            total,
            data: data.to_vec(),
        }
    }

    #[test]
    fn collector_reassembles_across_pages_and_keeps_first_version() {
        let mut collector = ChangesCollector::new();
        let next = collector
            .accept(&changes_body(
                vec![upsert("/big.bin", 0, 10, b"01234")],
                Some(FileCursor {
                    path: "/big.bin".as_path_buf(),
                    offset: 5,
                }),
                Some(7),
            ))
            .unwrap();
        assert_eq!(next.unwrap().offset, 5);

        // later page reports a newer version; the first one must win
        let next = collector
            .accept(&changes_body(
                vec![
                    upsert("/big.bin", 5, 10, b"56789"),
                    FileChunk {
                        path: "/gone.json".as_path_buf(),
                        kind: FileChangeKind::Delete,
                        offset: 0,
                        total: 0,
                        data: Vec::new(),
                    },
                ],
                None,
                Some(9),
            ))
            .unwrap();
        assert!(next.is_none());

        let (updates, version) = collector.finish().unwrap();
        assert_eq!(version, FsVersion::new(7));
        assert_eq!(updates.len(), 2);
        assert_eq!(updates[0].path, "/big.bin");
        assert_eq!(updates[0].content.as_deref(), Some(b"0123456789".as_ref()));
        assert_eq!(updates[1].content, None);
    }

    #[test]
    fn collector_rejects_out_of_order_chunks() {
        let mut collector = ChangesCollector::new();
        let result = collector.accept(&changes_body(
            vec![upsert("/big.bin", 5, 10, b"56789")],
            None,
            Some(1),
        ));
        assert!(result.is_err());

        let mut collector = ChangesCollector::new();
        collector
            .accept(&changes_body(
                vec![upsert("/big.bin", 0, 10, b"01234")],
                None,
                Some(1),
            ))
            .unwrap();
        let result = collector.accept(&changes_body(
            vec![upsert("/big.bin", 7, 10, b"789")],
            None,
            Some(1),
        ));
        assert!(result.is_err());
    }

    #[test]
    fn file_write_requests_chunk_only_when_needed() {
        let small = file_write_requests("x", "small.json", &[1u8; FILE_SYNC_CHUNK_BYTES]);
        assert_eq!(small.len(), 1);
        assert!(matches!(
            &small[0],
            ClientRequest::Filesystem(FsRequest::Write { .. })
        ));

        let big = file_write_requests("x", "big.svg", &[1u8; FILE_SYNC_CHUNK_BYTES * 2 + 1]);
        assert_eq!(big.len(), 3);
        match &big[2] {
            ClientRequest::Filesystem(FsRequest::WriteChunk { offset, data, .. }) => {
                assert_eq!(*offset as usize, FILE_SYNC_CHUNK_BYTES * 2);
                assert_eq!(data.len(), 1);
            }
            _ => panic!("expected WriteChunk"),
        }
    }
}
