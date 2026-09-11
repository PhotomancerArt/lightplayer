//! The Rust side of `emulator_tab_bridge.js`: one emulated board in this
//! tab, behind a handle that is a plain integer.
//!
//! Nothing here decides anything. The port's identity is a `u32` because a
//! [`DeviceByteStream`](lpa_client::stream::DeviceByteStream) is `Send` and
//! synchronous and a `JsValue` is neither — the same reason
//! `browser_serial.rs` holds port ids rather than `SerialPort` objects. The
//! policy (what a reset means, when a write is applied, which drainer gets
//! the bytes) lives in the JS beside it, where the awaits are.

use js_sys::{Array, Promise, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

use crate::LinkError;

#[wasm_bindgen(module = "/src/providers/emulator_tab/emulator_tab_bridge.js")]
extern "C" {
    #[wasm_bindgen(js_name = openEmuPort, catch)]
    fn js_open_emu_port(options: &JsValue) -> Result<u32, JsValue>;

    #[wasm_bindgen(js_name = reopenEmuPort, catch)]
    fn js_reopen(id: u32) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = closeEmuPort, catch)]
    fn js_close(id: u32) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = writeEmuPort, catch)]
    fn js_write(id: u32, bytes: &[u8]) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = signalsEmuPort, catch)]
    fn js_signals(id: u32, dtr: Option<bool>, rts: Option<bool>) -> Result<(), JsValue>;

    #[wasm_bindgen(js_name = takeEmuBytes, catch)]
    fn js_take_bytes(id: u32) -> Result<Uint8Array, JsValue>;

    #[wasm_bindgen(js_name = takeEmuLines, catch)]
    fn js_take_lines(id: u32) -> Result<Array, JsValue>;

    #[wasm_bindgen(js_name = takeEmuError, catch)]
    fn js_take_error(id: u32) -> Result<Option<String>, JsValue>;

    #[wasm_bindgen(js_name = isEmuStarting, catch)]
    fn js_is_starting(id: u32) -> Result<bool, JsValue>;

    #[wasm_bindgen(js_name = emuDilation, catch)]
    fn js_dilation(id: u32) -> Result<Option<f64>, JsValue>;

    #[wasm_bindgen(js_name = resetEmuPort)]
    fn js_reset(id: u32) -> Promise;

    #[wasm_bindgen(js_name = getEmuFlash)]
    fn js_get_flash(id: u32) -> Promise;

    #[wasm_bindgen(js_name = eraseEmuFlash)]
    fn js_erase_flash(id: u32) -> Promise;

    #[wasm_bindgen(js_name = flashEmuPackage)]
    fn js_flash_package(id: u32, manifest_url: &str) -> Promise;

    #[wasm_bindgen(js_name = emuProbes)]
    fn js_probes(id: u32) -> Promise;

    #[wasm_bindgen(js_name = disposeEmuPort)]
    fn js_dispose(id: u32) -> Promise;

    #[wasm_bindgen(js_name = deleteEmuFlash)]
    fn js_delete_flash(persist_key: &str) -> Promise;
}

/// What one emulated board in this tab is created as.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EmulatorTabOptions {
    /// The record's uid. Also the board id and, by default, the key its
    /// persisted 4 MiB image is stored under.
    pub uid: String,
    /// The minted base MAC the board wears in efuse and reports in its
    /// hello — the record's identity, not a second one.
    pub mac: String,
    /// The emulator module this page serves (`emu_esp32c6_wasm` in the
    /// engine manifest).
    pub module_url: String,
    /// The packaged build a blank chip is born flashed with (D22), or
    /// `None` to come up blank.
    pub manifest_url: Option<String>,
    /// Where the image is persisted. `None` means "under the uid".
    pub persist_key: Option<String>,
}

/// One emulated board in this tab.
///
/// `Copy` and `Send` because it is an integer: the port object itself lives
/// in the page (see the module docs), which is what lets a
/// `DeviceByteStream` hold one.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EmulatorTabPort {
    id: u32,
}

impl EmulatorTabPort {
    /// Open a board. The board is NOT up when this returns — see
    /// [`Self::is_starting`].
    pub fn open(options: &EmulatorTabOptions) -> Result<Self, LinkError> {
        let describe = js_sys::Object::new();
        let set = |key: &str, value: JsValue| {
            let _ = js_sys::Reflect::set(&describe, &JsValue::from_str(key), &value);
        };
        set("uid", JsValue::from_str(&options.uid));
        set("mac", JsValue::from_str(&options.mac));
        set("moduleUrl", JsValue::from_str(&options.module_url));
        set(
            "manifestUrl",
            match &options.manifest_url {
                Some(url) => JsValue::from_str(url),
                None => JsValue::NULL,
            },
        );
        set(
            "persistKey",
            match &options.persist_key {
                Some(key) => JsValue::from_str(key),
                None => JsValue::from_str(&options.uid),
            },
        );
        let id = js_open_emu_port(&describe).map_err(js_error)?;
        Ok(Self { id })
    }

    /// Attach the cable if it is out, then open the byte channel. Applied
    /// in order behind whatever is already queued.
    pub fn reopen(&self) -> Result<(), LinkError> {
        js_reopen(self.id).map_err(js_error)
    }

    /// Close the byte channel. The board keeps running.
    pub fn close(&self) -> Result<(), LinkError> {
        js_close(self.id).map_err(js_error)
    }

    /// Queue bytes for the board.
    pub fn write(&self, bytes: &[u8]) -> Result<(), LinkError> {
        js_write(self.id, bytes).map_err(js_error)
    }

    /// Queue a DTR/RTS write. `None` leaves that line untouched.
    pub fn signals(&self, dtr: Option<bool>, rts: Option<bool>) -> Result<(), LinkError> {
        js_signals(self.id, dtr, rts).map_err(js_error)
    }

    /// Everything the board has said since the last drain.
    pub fn take_bytes(&self) -> Result<Vec<u8>, LinkError> {
        Ok(js_take_bytes(self.id).map_err(js_error)?.to_vec())
    }

    /// Whole lines the board has said; the trailing partial stays buffered
    /// for the next drainer.
    pub fn take_lines(&self) -> Result<Vec<String>, LinkError> {
        Ok(js_take_lines(self.id)
            .map_err(js_error)?
            .iter()
            .filter_map(|line| line.as_string())
            .collect())
    }

    /// The first failure the queued work hit since the last ask.
    pub fn take_error(&self) -> Option<String> {
        js_take_error(self.id).ok().flatten()
    }

    /// Whether the worker, the module fetch and the cold boot are still in
    /// flight.
    pub fn is_starting(&self) -> bool {
        js_is_starting(self.id).unwrap_or(false)
    }

    /// How fast the board runs against wall time, as the worker last said.
    pub fn dilation(&self) -> Option<f64> {
        js_dilation(self.id).ok().flatten()
    }

    /// Reset the chip (not a replug).
    pub async fn reset(&self) -> Result<(), LinkError> {
        JsFuture::from(js_reset(self.id))
            .await
            .map(|_| ())
            .map_err(js_error)
    }

    /// The whole chip, as bytes.
    pub async fn get_flash(&self) -> Result<Vec<u8>, LinkError> {
        let bytes = JsFuture::from(js_get_flash(self.id))
            .await
            .map_err(js_error)?;
        Ok(Uint8Array::new(&bytes).to_vec())
    }

    /// Erase the chip.
    pub async fn erase_flash(&self) -> Result<(), LinkError> {
        JsFuture::from(js_erase_flash(self.id))
            .await
            .map(|_| ())
            .map_err(js_error)
    }

    /// Fetch `manifest_url`'s packaged build, write it into the chip and
    /// reset. Answers the build's display name.
    pub async fn flash_package(&self, manifest_url: &str) -> Result<String, LinkError> {
        let name = JsFuture::from(js_flash_package(self.id, manifest_url))
            .await
            .map_err(js_error)?;
        Ok(name.as_string().unwrap_or_else(|| "the firmware".to_string()))
    }

    /// `state` + `pins` + the tab's own counters, as the JSON the page
    /// answers with.
    pub async fn probes(&self) -> Result<String, LinkError> {
        let value = JsFuture::from(js_probes(self.id)).await.map_err(js_error)?;
        Ok(js_sys::JSON::stringify(&value)
            .ok()
            .and_then(|text| text.as_string())
            .unwrap_or_default())
    }

    /// End the board and its worker. The persisted image is untouched.
    pub async fn dispose(&self) -> Result<(), LinkError> {
        JsFuture::from(js_dispose(self.id))
            .await
            .map(|_| ())
            .map_err(js_error)
    }
}

/// Forget a board's persisted 4 MiB image.
///
/// Separate from the port on purpose: Forget reaches a board that is not
/// running, and the image outlives every port that ever held it (D15).
pub async fn delete_emu_flash(persist_key: &str) -> Result<(), LinkError> {
    JsFuture::from(js_delete_flash(persist_key))
        .await
        .map(|_| ())
        .map_err(js_error)
}

fn js_error(value: JsValue) -> LinkError {
    LinkError::other(
        value
            .as_string()
            .or_else(|| {
                js_sys::Reflect::get(&value, &JsValue::from_str("message"))
                    .ok()
                    .and_then(|message| message.as_string())
            })
            .unwrap_or_else(|| format!("{value:?}")),
    )
}
