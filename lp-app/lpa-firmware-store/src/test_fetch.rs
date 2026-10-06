//! Test support: a [`FirmwareFetch`] over a map of URLs, and a synthetic
//! release laid out at a store origin.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use lpc_firmware_release::{
    EncodedPieceFile, Encoding1, EncodingEntry, OtaManifest, PackageRef, PieceFile, Requires,
    sha256_hex,
};

use crate::engine_cache::LocalBoxFuture;
use crate::firmware_fetch::{FetchError, FirmwareFetch};

/// URL → bytes, recording every URL asked for. Cloning shares it.
#[derive(Clone, Default)]
pub struct FakeFetch {
    inner: Rc<RefCell<FakeInner>>,
}

#[derive(Default)]
struct FakeInner {
    responses: BTreeMap<String, Vec<u8>>,
    calls: Vec<String>,
    offline: bool,
}

impl FakeFetch {
    pub fn put(&self, url: &str, bytes: Vec<u8>) {
        self.inner
            .borrow_mut()
            .responses
            .insert(url.to_string(), bytes);
    }

    pub fn remove(&self, url: &str) {
        self.inner.borrow_mut().responses.remove(url);
    }

    pub fn go_offline(&self) {
        self.inner.borrow_mut().offline = true;
    }

    pub fn calls(&self) -> Vec<String> {
        self.inner.borrow().calls.clone()
    }
}

impl FirmwareFetch for FakeFetch {
    fn get(&self, url: &str) -> LocalBoxFuture<'_, Result<Option<Vec<u8>>, FetchError>> {
        let mut inner = self.inner.borrow_mut();
        inner.calls.push(url.to_string());
        let reply = if inner.offline {
            Err(FetchError::Offline("test".into()))
        } else {
            Ok(inner.responses.get(url).cloned())
        };
        Box::pin(std::future::ready(reply))
    }
}

/// A synthetic release `2026.10.05-3+103285d5d05e` of `esp32c6-4mb`.
pub struct ReleaseFixture {
    pub manifest: OtaManifest,
    pub engine: Vec<u8>,
    files: BTreeMap<&'static str, Vec<u8>>,
}

impl ReleaseFixture {
    /// A fetch that serves this release at `origin` by version, build id
    /// and `latest` (the redirect already followed).
    pub fn fetch(&self, origin: &str) -> FakeFetch {
        let fetch = FakeFetch::default();
        let base = format!("{origin}/firmware/esp32c6-4mb");
        let manifest = self.manifest.to_json_bytes();
        for release in ["2026.10.05-3", "2026.10.05-3%2B103285d5d05e", "latest"] {
            fetch.put(
                &format!("{base}/{release}/ota-manifest.json"),
                manifest.clone(),
            );
        }
        for (file, bytes) in &self.files {
            fetch.put(&format!("{base}/2026.10.05-3/{file}"), bytes.clone());
        }
        fetch
    }
}

pub fn release_fixture() -> ReleaseFixture {
    let core = vec![0xc0; 5000];
    let engine = vec![0xe0; 300];
    let core_z = vec![0x7a; 30];
    let engine_z = vec![0x7b; 20];
    let package = br#"{"schemaVersion":2}"#.to_vec();
    let image = vec![0x1e; 4096];
    let piece = |file: &str, bytes: &[u8]| PieceFile {
        file: file.into(),
        length: bytes.len() as u64,
        sha256: sha256_hex(bytes),
    };
    let manifest = OtaManifest {
        format: 1,
        target: "esp32c6-4mb".into(),
        chip: "esp32c6".into(),
        version: "2026.10.05-3".into(),
        commit: "103285d5d05e0c2223260f7a3ccf69eb32cf40cc".into(),
        wire_proto: 36,
        requires: Requires {
            layout: 1,
            loader: 1,
        },
        core: piece("core.bin", &core),
        engine: piece("engine.bin", &engine),
        encodings: vec![EncodingEntry::deflate1(Encoding1 {
            id: 1,
            codec: "deflate-raw".into(),
            chunk_bytes: 4096,
            window_bytes: 32768,
            core: EncodedPieceFile {
                file: "core.z".into(),
                length: 30,
                sha256: sha256_hex(&core_z),
                chunks: vec![30, 0],
            },
            engine: EncodedPieceFile {
                file: "engine.z".into(),
                length: 20,
                sha256: sha256_hex(&engine_z),
                chunks: vec![20],
            },
        })],
        package: PackageRef {
            file: "package.json".into(),
            length: package.len() as u64,
            sha256: sha256_hex(&package),
            image: piece("fw-esp32c6-merged.bin", &image),
        },
    };
    manifest.validate().expect("the fixture is valid");
    let files = BTreeMap::from([
        ("core.bin", core),
        ("engine.bin", engine.clone()),
        ("core.z", core_z),
        ("engine.z", engine_z),
        ("package.json", package),
        ("fw-esp32c6-merged.bin", image),
    ]);
    ReleaseFixture {
        manifest,
        engine,
        files,
    }
}
