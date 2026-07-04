use std::cell::RefCell;
use std::rc::Rc;

use js_sys::Promise;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{ErrorEvent, MessageEvent, Url, Worker, WorkerOptions, WorkerType};

use crate::LinkError;
use crate::providers::browser_worker::{
    BrowserInputEnvelope, BrowserOutputEnvelope, BrowserWorkerOptions,
};

pub struct BrowserWorkerHandle {
    worker: Worker,
    outputs: Rc<RefCell<Vec<BrowserOutputEnvelope>>>,
}

impl BrowserWorkerHandle {
    pub fn new(worker_script_path: &str) -> Result<Self, LinkError> {
        let options = WorkerOptions::new();
        options.set_type(WorkerType::Module);
        let worker = Worker::new_with_options(worker_script_path, &options)
            .map_err(|error| LinkError::other(format!("{error:?}")))?;
        let outputs = Rc::new(RefCell::new(Vec::new()));
        let output_ref = Rc::clone(&outputs);
        let on_message = Closure::<dyn FnMut(MessageEvent)>::new(move |event: MessageEvent| {
            match serde_wasm_bindgen::from_value::<BrowserOutputEnvelope>(event.data()) {
                Ok(envelope) => output_ref.borrow_mut().push(envelope),
                Err(error) => output_ref.borrow_mut().push(BrowserOutputEnvelope::Log {
                    runtime_id: 0,
                    level: "error".to_string(),
                    target: "lpa-link".to_string(),
                    message: format!("failed to parse worker message: {error}"),
                }),
            }
        });
        worker.set_onmessage(Some(on_message.as_ref().unchecked_ref()));
        on_message.forget();

        let output_ref = Rc::clone(&outputs);
        let on_error = Closure::<dyn FnMut(ErrorEvent)>::new(move |event: ErrorEvent| {
            output_ref.borrow_mut().push(BrowserOutputEnvelope::Status {
                runtime_id: None,
                status: "error".to_string(),
                message: Some(format!(
                    "worker error: {} at {}:{}:{}",
                    event.message(),
                    event.filename(),
                    event.lineno(),
                    event.colno()
                )),
            });
        });
        worker.set_onerror(Some(on_error.as_ref().unchecked_ref()));
        on_error.forget();

        let output_ref = Rc::clone(&outputs);
        let on_message_error =
            Closure::<dyn FnMut(MessageEvent)>::new(move |_event: MessageEvent| {
                output_ref.borrow_mut().push(BrowserOutputEnvelope::Status {
                    runtime_id: None,
                    status: "error".to_string(),
                    message: Some("worker message could not be deserialized".to_string()),
                });
            });
        worker.set_onmessageerror(Some(on_message_error.as_ref().unchecked_ref()));
        on_message_error.forget();
        Ok(Self { worker, outputs })
    }

    pub async fn boot(
        &mut self,
        label: &str,
        options: &BrowserWorkerOptions,
    ) -> Result<Vec<BrowserOutputEnvelope>, LinkError> {
        let fw_browser_module_path = resolve_page_url(&options.fw_browser_module_path)?;
        let fw_browser_wasm_path = resolve_page_url(&options.fw_browser_wasm_path)?;
        self.post(&BrowserInputEnvelope::Boot {
            label: label.to_string(),
            fw_browser_module_path,
            fw_browser_wasm_path,
            tick_mode: options.tick_mode,
        })?;
        let mut outputs = Vec::new();
        for _ in 0..200 {
            sleep_ms(25).await?;
            for output in self.take_outputs() {
                let ready = matches!(
                    &output,
                    BrowserOutputEnvelope::Status { status, .. } if status == "ready"
                );
                let error = boot_error_message(&output);
                outputs.push(output);
                if let Some(message) = error {
                    return Err(LinkError::other(format!(
                        "browser worker boot failed: {message}"
                    )));
                }
                if ready {
                    return Ok(outputs);
                }
            }
        }
        Err(LinkError::other(format!(
            "timed out waiting for browser worker boot{}",
            boot_output_summary(&outputs)
        )))
    }

    pub fn post(&self, envelope: &BrowserInputEnvelope) -> Result<(), LinkError> {
        let value = serde_wasm_bindgen::to_value(envelope)
            .map_err(|error| LinkError::other(error.to_string()))?;
        self.worker
            .post_message(&value)
            .map_err(|error| LinkError::other(format!("{error:?}")))
    }

    pub fn take_outputs(&mut self) -> Vec<BrowserOutputEnvelope> {
        core::mem::take(&mut *self.outputs.borrow_mut())
    }

    pub fn terminate(&self) {
        self.worker.terminate();
    }
}

fn resolve_page_url(path: &str) -> Result<String, LinkError> {
    if path.is_empty() {
        return Ok(String::new());
    }
    if Url::new(path).is_ok() {
        return Ok(path.to_string());
    }

    let Some(window) = web_sys::window() else {
        return Err(LinkError::other(format!(
            "cannot resolve browser worker asset path {path:?}: missing window"
        )));
    };
    let base = if let Some(document) = window.document() {
        document
            .url()
            .map_err(|error| LinkError::other(format!("{error:?}")))?
    } else {
        window
            .location()
            .href()
            .map_err(|error| LinkError::other(format!("{error:?}")))?
    };
    Url::new_with_base(path, &base)
        .map(|url| url.href())
        .map_err(|error| {
            LinkError::other(format!(
                "cannot resolve browser worker asset path {path:?} from {base:?}: {error:?}"
            ))
        })
}

fn boot_error_message(output: &BrowserOutputEnvelope) -> Option<String> {
    match output {
        BrowserOutputEnvelope::Status {
            status, message, ..
        } if status == "error" => Some(
            message
                .clone()
                .unwrap_or_else(|| "worker reported error status".to_string()),
        ),
        BrowserOutputEnvelope::Log { level, message, .. } if level == "error" => {
            Some(message.clone())
        }
        _ => None,
    }
}

fn boot_output_summary(outputs: &[BrowserOutputEnvelope]) -> String {
    let Some(last) = outputs.last() else {
        return "; no worker output was received".to_string();
    };
    match last {
        BrowserOutputEnvelope::Status {
            status, message, ..
        } => match message {
            Some(message) => format!("; last worker status was {status}: {message}"),
            None => format!("; last worker status was {status}"),
        },
        BrowserOutputEnvelope::Log {
            level,
            target,
            message,
            ..
        } => format!("; last worker log was {level} from {target}: {message}"),
        BrowserOutputEnvelope::ProtocolOut { .. } => {
            "; last worker output was a protocol frame".to_string()
        }
    }
}

async fn sleep_ms(ms: i32) -> Result<(), LinkError> {
    let promise = Promise::new(&mut |resolve: js_sys::Function, reject: js_sys::Function| {
        let Some(window) = web_sys::window() else {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("missing window"));
            return;
        };
        if let Err(error) =
            window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
        {
            let _ = reject.call1(&JsValue::NULL, &error);
        }
    });
    JsFuture::from(promise)
        .await
        .map(|_| ())
        .map_err(|error| LinkError::other(format!("{error:?}")))
}
