//! JS-callback-backed [`WebProgress`] for streaming migration progress to UI.
//!
//! Constructed from a JS object with three optional methods:
//! - `onTargetStart(targetName: string, targetHash: string)`
//! - `onStage(targetName: string, stage: string)`
//! - `onTargetFinish(targetName: string)`
//!
//! Missing methods are silently ignored (no-op progress).

use hd2_migrator_io::web::WebProgress;
use js_sys::{Array, Function};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;

pub struct JsProgress {
    target: JsValue,
    on_target_start: Option<Function>,
    on_stage: Option<Function>,
    on_target_finish: Option<Function>,
}

impl JsProgress {
    pub fn from_js(value: JsValue) -> Result<Self, JsValue> {
        if value.is_null() || value.is_undefined() {
            return Ok(Self {
                target: JsValue::NULL,
                on_target_start: None,
                on_stage: None,
                on_target_finish: None,
            });
        }
        if !value.is_object() {
            return Err(crate::error::js_error("progress must be an object or null"));
        }
        Ok(Self {
            on_target_start: optional_function(&value, "onTargetStart")?,
            on_stage: optional_function(&value, "onStage")?,
            on_target_finish: optional_function(&value, "onTargetFinish")?,
            target: value,
        })
    }
}

impl WebProgress for JsProgress {
    fn target_started(&self, target_name: &str, target_hash: &str) -> hd2_migrator_io::Result<()> {
        let Some(fun) = self.on_target_start.as_ref() else {
            return Ok(());
        };
        let args = Array::new();
        args.push(&JsValue::from_str(target_name));
        args.push(&JsValue::from_str(target_hash));
        fun.apply(&self.target, &args).map_err(callback_error)?;
        Ok(())
    }

    fn stage(&self, target_name: &str, stage: &str) -> hd2_migrator_io::Result<()> {
        let Some(fun) = self.on_stage.as_ref() else {
            return Ok(());
        };
        let args = Array::new();
        args.push(&JsValue::from_str(target_name));
        args.push(&JsValue::from_str(stage));
        fun.apply(&self.target, &args).map_err(callback_error)?;
        Ok(())
    }

    fn target_finished(&self, target_name: &str) -> hd2_migrator_io::Result<()> {
        let Some(fun) = self.on_target_finish.as_ref() else {
            return Ok(());
        };
        fun.call1(&self.target, &JsValue::from_str(target_name))
            .map_err(callback_error)?;
        Ok(())
    }
}

fn callback_error(error: JsValue) -> eyre::Report {
    let message = error.as_string().or_else(|| {
        js_sys::Reflect::get(&error, &JsValue::from_str("message"))
            .ok()
            .and_then(|value| value.as_string())
    });
    eyre::eyre!(message.unwrap_or_else(|| "progress callback failed".to_owned()))
}

fn optional_function(obj: &JsValue, key: &str) -> Result<Option<Function>, JsValue> {
    let value = js_sys::Reflect::get(obj, &JsValue::from_str(key))?;
    if value.is_undefined() || value.is_null() {
        return Ok(None);
    }
    value
        .dyn_into::<Function>()
        .map(Some)
        .map_err(|_| crate::error::js_error(format!("progress.{key} must be a function")))
}
