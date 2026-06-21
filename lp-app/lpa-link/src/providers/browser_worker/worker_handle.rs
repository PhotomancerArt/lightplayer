use std::cell::RefCell;
use std::rc::Rc;

use js_sys::Promise;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{MessageEvent, Worker, WorkerOptions, WorkerType};

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
        Ok(Self { worker, outputs })
    }

    pub async fn boot(
        &mut self,
        label: &str,
        options: &BrowserWorkerOptions,
    ) -> Result<Vec<BrowserOutputEnvelope>, LinkError> {
        self.post(&BrowserInputEnvelope::Boot {
            label: label.to_string(),
            fw_browser_module_path: options.fw_browser_module_path.clone(),
            fw_browser_wasm_path: options.fw_browser_wasm_path.clone(),
        })?;
        let mut outputs = Vec::new();
        for _ in 0..200 {
            sleep_ms(25).await?;
            for output in self.take_outputs() {
                let ready = matches!(
                    &output,
                    BrowserOutputEnvelope::Status { status, .. } if status == "ready"
                );
                outputs.push(output);
                if ready {
                    return Ok(outputs);
                }
            }
        }
        Err(LinkError::other(
            "timed out waiting for browser worker boot",
        ))
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
