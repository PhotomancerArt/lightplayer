//! This Studio's own build, as its bundle carries it (DS10): the
//! [`OwnBuildSource`] a served Studio installs.
//!
//! For each served build the bundle holds `firmware/<target>/manifest.json`
//! (the package manifest) and the merged image it names; a split target
//! also holds `firmware/<target>/ota/`: the package's `ota-manifest.json`,
//! `core.z` and `engine.z`. Never `core.bin` / `engine.bin`: those are
//! **sliced out of the merged image** by the package manifest's `split`
//! offsets, as the split image's design planned (M2's D13), so the bundle
//! grows by the compressed files only.
//!
//! **Update-capable, checked twice, before a fact is believed.** A Studio
//! build carries an update-capable firmware build only when
//!
//! - its `ota/ota-manifest.json` parses, and names this very package —
//!   the package manifest's bytes hash to the manifest's `package` entry
//!   (so a stale `ota/` beside a newer single-image package is refused),
//!   and the merged image it names is the package's; and
//! - the image's own manifest core says so: its `ota.layout` is the one
//!   the update files require (update protocol Part B, DD29).
//!
//! Anything else — a single-image package (the fast local build), no `ota/`
//! directory, a mismatch — is **no build of its own**: [`facts`] stays
//! `None`, no board is offered an over-the-air update, and over USB the card
//! keeps today's flash (`device_update_route`). A restore still works.
//!
//! [`load`] fetches the merged image and the two `.z` files, checks the
//! image against the package manifest, slices the pieces, and builds the
//! [`HostBuild`] through `HostBuild::from_ota_manifest`, which checks every
//! piece and every `.z` file against `ota-manifest.json`. A mismatch is an
//! error naming the file ("this Studio's firmware files don't match: …").
//!
//! The facts load asynchronously ([`BundledOwnBuildSource::read_facts`],
//! spawned by the shell at start); until they arrive [`facts`] is `None`,
//! and the controller reads them again after every device fold.
//!
//! [`facts`]: OwnBuildSource::facts
//! [`load`]: OwnBuildSource::load

use std::cell::RefCell;
use std::rc::Rc;

use lpa_firmware_store::FirmwareFetch;
use lpa_update::{HostBuild, HostBuildFacts};
use lpc_firmware_release::OtaManifest;

use super::own_build_source::OwnBuildSource;
use super::update_store_builds::facts_from_ota_manifest;
use crate::app::library::LocalBoxFuture;

/// The words every refusal starts with: what the terminal and the log say.
pub const OWN_BUILD_MISMATCH: &str = "this Studio's firmware files don't match";

/// The package manifest's name in the bundle, beside the merged image.
const PACKAGE_MANIFEST: &str = "manifest.json";

/// Where a target's update files sit, under its firmware directory.
const OTA_DIR: &str = "ota";

/// One served build's update-capable package, by what its two manifests
/// say: everything [`Self::build`] needs to turn fetched bytes into a
/// [`HostBuild`].
#[derive(Clone, Debug)]
pub struct BundledOwnBuild {
    facts: HostBuildFacts,
    ota: OtaManifest,
    image: BundledImage,
    core: Slice,
    engine: Slice,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BundledImage {
    file: String,
    length: u64,
    sha256: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Slice {
    offset: usize,
    len: usize,
}

impl BundledOwnBuild {
    /// Read a package manifest's bytes and its `ota-manifest.json`'s, and
    /// check they describe one update-capable build (see the module docs).
    pub fn read(package_manifest: &[u8], ota_manifest: &[u8]) -> Result<Self, String> {
        let ota = OtaManifest::parse_valid(ota_manifest)
            .map_err(|e| mismatch(format!("ota-manifest.json: {e}")))?;
        ota.verify(&ota.package.file, package_manifest)
            .map_err(|e| mismatch(format!("{PACKAGE_MANIFEST} is not this build's: {e}")))?;
        let package: serde_json::Value = serde_json::from_slice(package_manifest)
            .map_err(|e| mismatch(format!("{PACKAGE_MANIFEST}: {e}")))?;

        let layout = package
            .pointer("/core/ota/layout")
            .and_then(serde_json::Value::as_u64);
        if layout != Some(u64::from(ota.requires.layout)) {
            return Err(mismatch(format!(
                "the image's manifest core says ota layout {layout:?}, the update files need {}",
                ota.requires.layout
            )));
        }

        let image = match package.pointer("/images") {
            Some(serde_json::Value::Array(images)) if images.len() == 1 => &images[0],
            _ => {
                return Err(mismatch(format!(
                    "{PACKAGE_MANIFEST}: not one merged image"
                )));
            }
        };
        let image = BundledImage {
            file: string_at(image, "/path")?,
            length: image
                .pointer("/sizeBytes")
                .and_then(serde_json::Value::as_u64)
                .ok_or_else(|| mismatch(format!("{PACKAGE_MANIFEST}: no image size")))?,
            sha256: string_at(image, "/sha256")?,
        };
        let named = &ota.package.image;
        if (named.file.as_str(), named.length, named.sha256.as_str())
            != (image.file.as_str(), image.length, image.sha256.as_str())
        {
            return Err(mismatch(format!(
                "ota-manifest.json names image {} ({} B), the package {} ({} B)",
                named.file, named.length, image.file, image.length
            )));
        }

        let core = slice_at(&package, "core")?;
        let engine = slice_at(&package, "engine")?;
        let facts = facts_from_ota_manifest(&ota)
            .ok_or_else(|| mismatch("ota-manifest.json: a hash is not hex".to_string()))?;
        Ok(Self {
            facts,
            ota,
            image,
            core,
            engine,
        })
    }

    /// The build's facts: every card's update standing reads them.
    pub fn facts(&self) -> &HostBuildFacts {
        &self.facts
    }

    /// The merged image's file name, beside the package manifest.
    pub fn image_file(&self) -> &str {
        &self.image.file
    }

    /// The compressed files encoding 1 lists, under `ota/` (none: raw only).
    pub fn encoded_files(&self) -> Vec<String> {
        self.ota
            .encoding1()
            .map(|e| vec![e.core.file.clone(), e.engine.file.clone()])
            .unwrap_or_default()
    }

    /// The build, from the merged image and the encoded files (by name):
    /// the image checked against the package, the pieces sliced out of it,
    /// and everything checked against `ota-manifest.json`.
    pub fn build(&self, image: &[u8], encoded: &[(String, Vec<u8>)]) -> Result<HostBuild, String> {
        if image.len() as u64 != self.image.length
            || lpc_firmware_release::sha256_hex(image) != self.image.sha256
        {
            return Err(mismatch(format!(
                "{} is not the image the package names",
                self.image.file
            )));
        }
        let piece = |slice: Slice| -> Option<Vec<u8>> {
            image
                .get(slice.offset..slice.offset.checked_add(slice.len)?)
                .map(<[u8]>::to_vec)
        };
        let core = piece(self.core);
        let engine = piece(self.engine);
        let read = |name: &str| -> Option<Vec<u8>> {
            if name == self.ota.core.file {
                core.clone()
            } else if name == self.ota.engine.file {
                engine.clone()
            } else {
                encoded
                    .iter()
                    .find(|(file, _)| file == name)
                    .map(|(_, bytes)| bytes.clone())
            }
        };
        let build = HostBuild::from_ota_manifest(&self.ota, read)
            .map_err(|e| mismatch(format!("{e:?}")))?;
        if build.facts() != self.facts {
            return Err(mismatch(
                "the pieces are not the build ota-manifest.json names".to_string(),
            ));
        }
        Ok(build)
    }
}

/// Where a served Studio's own build is read from: a fetch of the bundle's
/// own files under `base` (the served `firmware/` directory, a same-origin
/// path or URL) for `targets`, the first update-capable one winning.
pub struct BundledOwnBuildSource<F: FirmwareFetch> {
    state: Rc<SourceState<F>>,
}

struct SourceState<F> {
    fetch: F,
    base: String,
    targets: Vec<String>,
    read: RefCell<Option<Result<(String, BundledOwnBuild), String>>>,
}

impl<F: FirmwareFetch + 'static> BundledOwnBuildSource<F> {
    pub fn new(fetch: F, base: &str, targets: Vec<String>) -> Self {
        Self {
            state: Rc::new(SourceState {
                fetch,
                base: base.trim_end_matches('/').to_string(),
                targets,
                read: RefCell::new(None),
            }),
        }
    }

    /// Read every target's manifests until one is an update-capable build;
    /// the shell spawns this once at start. Answers what it found, or why
    /// none (a single-image Studio is `Err` with the reason, and no error).
    pub fn read_facts(&self) -> LocalBoxFuture<'static, Result<HostBuildFacts, String>> {
        let state = Rc::clone(&self.state);
        Box::pin(async move {
            let mut why = Vec::new();
            for target in &state.targets {
                match read_target(&state, target).await {
                    Ok(build) => {
                        let facts = build.facts().clone();
                        *state.read.borrow_mut() = Some(Ok((target.clone(), build)));
                        return Ok(facts);
                    }
                    Err(reason) => why.push(format!("{target}: {reason}")),
                }
            }
            let why = match why.is_empty() {
                true => "no served build".to_string(),
                false => why.join("; "),
            };
            *state.read.borrow_mut() = Some(Err(why.clone()));
            Err(why)
        })
    }
}

impl<F: FirmwareFetch + 'static> Clone for BundledOwnBuildSource<F> {
    fn clone(&self) -> Self {
        Self {
            state: Rc::clone(&self.state),
        }
    }
}

impl<F: FirmwareFetch + 'static> OwnBuildSource for BundledOwnBuildSource<F> {
    fn facts(&self) -> Option<HostBuildFacts> {
        match &*self.state.read.borrow() {
            Some(Ok((_, build))) => Some(build.facts().clone()),
            _ => None,
        }
    }

    fn load(&self) -> LocalBoxFuture<'static, Result<HostBuild, String>> {
        let state = Rc::clone(&self.state);
        Box::pin(async move {
            let Some(Ok((target, build))) = state.read.borrow().clone() else {
                return Err("this Studio has no build of its own".to_string());
            };
            let dir = format!("{}/{target}", state.base);
            let image = fetch_file(&state.fetch, &format!("{dir}/{}", build.image_file())).await?;
            let mut encoded = Vec::new();
            for file in build.encoded_files() {
                let bytes = fetch_file(&state.fetch, &format!("{dir}/{OTA_DIR}/{file}")).await?;
                encoded.push((file, bytes));
            }
            build.build(&image, &encoded)
        })
    }
}

/// One target's two manifests, read and checked.
async fn read_target<F: FirmwareFetch>(
    state: &SourceState<F>,
    target: &str,
) -> Result<BundledOwnBuild, String> {
    let dir = format!("{}/{target}", state.base);
    let ota = match state
        .fetch
        .get(&format!("{dir}/{OTA_DIR}/ota-manifest.json"))
        .await
    {
        Ok(Some(bytes)) => bytes,
        Ok(None) => return Err("no update files in this Studio (a single image)".to_string()),
        Err(e) => return Err(format!("ota-manifest.json: {e:?}")),
    };
    let package = fetch_file(&state.fetch, &format!("{dir}/{PACKAGE_MANIFEST}")).await?;
    BundledOwnBuild::read(&package, &ota)
}

async fn fetch_file<F: FirmwareFetch>(fetch: &F, url: &str) -> Result<Vec<u8>, String> {
    match fetch.get(url).await {
        Ok(Some(bytes)) => Ok(bytes),
        Ok(None) => Err(mismatch(format!("{url} is missing"))),
        Err(e) => Err(format!("{url}: {e:?}")),
    }
}

fn mismatch(why: String) -> String {
    format!("{OWN_BUILD_MISMATCH}: {why}")
}

fn string_at(value: &serde_json::Value, pointer: &str) -> Result<String, String> {
    value
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| mismatch(format!("{PACKAGE_MANIFEST}: no {pointer}")))
}

/// A split piece's place in the merged image (which is flashed at `0x0`,
/// so a piece's flash offset is its offset in the image).
fn slice_at(package: &serde_json::Value, piece: &str) -> Result<Slice, String> {
    let offset = package
        .pointer(&format!("/split/{piece}/offset"))
        .and_then(serde_json::Value::as_str)
        .and_then(|hex| usize::from_str_radix(hex.trim_start_matches("0x"), 16).ok());
    let len = package
        .pointer(&format!("/split/{piece}/sizeBytes"))
        .and_then(serde_json::Value::as_u64)
        .and_then(|len| usize::try_from(len).ok());
    match (offset, len) {
        (Some(offset), Some(len)) => Ok(Slice { offset, len }),
        _ => Err(mismatch(format!(
            "{PACKAGE_MANIFEST}: no split {piece} (not a split image)"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use lpa_firmware_store::FetchError;
    use lpc_firmware_release::{
        EncodedPieceFile, Encoding1, EncodingEntry, PackageRef, PieceFile, Requires, sha256_hex,
    };

    use super::*;

    #[test]
    fn a_split_package_with_its_update_files_is_this_studios_build() {
        let bundle = Bundle::split();
        let source = bundle.source();
        assert_eq!(source.facts(), None, "nothing until it is read");
        let facts = block_on(source.read_facts()).expect("an own build");
        assert_eq!(facts.identity.version, "2026.10.06-1");
        assert_eq!(source.facts(), Some(facts.clone()));

        let build = block_on(source.load()).expect("it loads");
        assert_eq!(build.facts(), facts);
        assert!(
            build.core.encoded.is_some() && build.engine.encoded.is_some(),
            "encoding 1 rides along"
        );
    }

    /// The fast local build: one image, no `ota/` — no build of its own.
    #[test]
    fn a_single_image_studio_has_no_build_of_its_own() {
        let mut bundle = Bundle::split();
        bundle.files.retain(|path, _| !path.contains("/ota/"));
        let source = bundle.source();
        let why = block_on(source.read_facts()).unwrap_err();
        assert!(why.contains("single image"), "{why}");
        assert_eq!(source.facts(), None);
        assert!(block_on(source.load()).is_err());
    }

    /// A stale `ota/` beside a newer package (a single image packaged over
    /// a split one, say) names another package: refused.
    #[test]
    fn update_files_for_another_package_are_refused() {
        let mut bundle = Bundle::split();
        let manifest = bundle
            .files
            .get_mut("fw/esp32c6-4mb/manifest.json")
            .unwrap();
        manifest.extend_from_slice(b"\n");
        let why = block_on(bundle.source().read_facts()).unwrap_err();
        assert!(why.contains(OWN_BUILD_MISMATCH), "{why}");
    }

    /// The image's own manifest core must say it updates over the air.
    #[test]
    fn a_manifest_core_without_ota_is_not_update_capable() {
        let bundle = Bundle::with_core_ota(None);
        let why = block_on(bundle.source().read_facts()).unwrap_err();
        assert!(why.contains("ota layout None"), "{why}");
    }

    /// Bytes that are not what the manifests say fail the load, by name.
    #[test]
    fn a_damaged_file_fails_the_load_and_names_it() {
        let mut bundle = Bundle::split();
        let source = bundle.source();
        block_on(source.read_facts()).expect("read");
        bundle.files.get_mut("fw/esp32c6-4mb/ota/engine.z").unwrap()[0] ^= 0xFF;
        let source = BundledOwnBuildSource {
            state: Rc::new(SourceState {
                fetch: MemoryFetch(bundle.files.clone()),
                base: "fw".to_string(),
                targets: vec!["esp32c6-4mb".to_string()],
                read: RefCell::new(source.state.read.borrow().clone()),
            }),
        };
        let why = block_on(source.load()).unwrap_err();
        assert!(
            why.contains(OWN_BUILD_MISMATCH) && why.contains("engine.z"),
            "{why}"
        );
    }

    // ---- The bundle ------------------------------------------------------

    struct Bundle {
        files: HashMap<String, Vec<u8>>,
    }

    impl Bundle {
        fn split() -> Self {
            Self::with_core_ota(Some(1))
        }

        /// A split package (loader, core, engine at their offsets) and its
        /// update files, with the manifest core's `ota.layout`.
        fn with_core_ota(layout: Option<u16>) -> Self {
            let core_off = 0x18000;
            let core: Vec<u8> = (0..9000u32).map(|i| (i % 251) as u8).collect();
            let engine_off = 0x20000;
            let engine: Vec<u8> = (0..13000u32).map(|i| (i % 241) as u8).collect();
            let mut image = vec![0xFFu8; engine_off + engine.len()];
            image[core_off..core_off + core.len()].copy_from_slice(&core);
            image[engine_off..].copy_from_slice(&engine);

            // One byte per raw 4 KiB chunk: never decoded here, only checked.
            let core_z = vec![7u8; core.len().div_ceil(4096)];
            let engine_z = vec![9u8; engine.len().div_ceil(4096)];
            let mut package = serde_json::json!({
                "schemaVersion": 2,
                "firmwareId": "esp32c6-4mb",
                "core": { "target": "esp32c6-4mb" },
                "images": [{ "path": "fw-esp32c6-merged.bin", "address": "0x0",
                    "sizeBytes": image.len(), "sha256": sha256_hex(&image) }],
                "split": {
                    "core": { "offset": format!("{core_off:#x}"), "sizeBytes": core.len() },
                    "engine": { "offset": format!("{engine_off:#x}"), "sizeBytes": engine.len() }
                }
            });
            if let Some(layout) = layout {
                package["core"]["ota"] = serde_json::json!({ "layout": layout });
            }
            let package = serde_json::to_vec_pretty(&package).unwrap();

            let ota = OtaManifest {
                format: lpc_firmware_release::OTA_MANIFEST_FORMAT,
                target: "esp32c6-4mb".to_string(),
                chip: "esp32c6".to_string(),
                version: "2026.10.06-1".to_string(),
                commit: "e6775ad537f6039afabc35952488312141962ccd".to_string(),
                wire_proto: 38,
                requires: Requires {
                    layout: 1,
                    loader: 1,
                },
                core: piece("core.bin", &core),
                engine: piece("engine.bin", &engine),
                encodings: vec![EncodingEntry::deflate1(Encoding1 {
                    id: 1,
                    codec: "deflate-raw".to_string(),
                    chunk_bytes: 4096,
                    window_bytes: 32768,
                    core: encoded("core.z", &core_z),
                    engine: encoded("engine.z", &engine_z),
                })],
                package: PackageRef {
                    file: "package.json".to_string(),
                    length: package.len() as u64,
                    sha256: sha256_hex(&package),
                    image: piece("fw-esp32c6-merged.bin", &image),
                },
            };
            let mut files = HashMap::new();
            files.insert("fw/esp32c6-4mb/manifest.json".to_string(), package);
            files.insert("fw/esp32c6-4mb/fw-esp32c6-merged.bin".to_string(), image);
            files.insert(
                "fw/esp32c6-4mb/ota/ota-manifest.json".to_string(),
                ota.to_json_bytes(),
            );
            files.insert("fw/esp32c6-4mb/ota/core.z".to_string(), core_z);
            files.insert("fw/esp32c6-4mb/ota/engine.z".to_string(), engine_z);
            Self { files }
        }

        fn source(&self) -> BundledOwnBuildSource<MemoryFetch> {
            BundledOwnBuildSource::new(
                MemoryFetch(self.files.clone()),
                "fw/",
                vec!["esp32c6-4mb".to_string()],
            )
        }
    }

    fn piece(file: &str, bytes: &[u8]) -> PieceFile {
        PieceFile {
            file: file.to_string(),
            length: bytes.len() as u64,
            sha256: sha256_hex(bytes),
        }
    }

    /// A `.z` file whose every chunk is one byte.
    fn encoded(file: &str, bytes: &[u8]) -> EncodedPieceFile {
        EncodedPieceFile {
            file: file.to_string(),
            length: bytes.len() as u64,
            sha256: sha256_hex(bytes),
            chunks: vec![1; bytes.len()],
        }
    }

    struct MemoryFetch(HashMap<String, Vec<u8>>);

    impl FirmwareFetch for MemoryFetch {
        fn get(
            &self,
            url: &str,
        ) -> lpa_firmware_store::LocalBoxFuture<'_, Result<Option<Vec<u8>>, FetchError>> {
            let found = self.0.get(url).cloned();
            Box::pin(core::future::ready(Ok(found)))
        }
    }

    fn block_on<T>(future: LocalBoxFuture<'static, T>) -> T {
        let mut future = future;
        let mut cx = core::task::Context::from_waker(core::task::Waker::noop());
        match future.as_mut().poll(&mut cx) {
            core::task::Poll::Ready(value) => value,
            core::task::Poll::Pending => panic!("a memory fetch is immediately ready"),
        }
    }
}
