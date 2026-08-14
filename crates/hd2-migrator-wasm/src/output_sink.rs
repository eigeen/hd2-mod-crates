//! JS-backed output sink for moving serialized files out of WASM one at a time.

use hd2_migrator_io::web::WebOutputFile;
use js_sys::Function;
use wasm_bindgen::{JsCast, prelude::*};

pub struct JsOutputSink {
    target: JsValue,
    on_file: Function,
}

impl JsOutputSink {
    pub fn from_js(value: JsValue) -> Result<Self, JsValue> {
        if !value.is_object() {
            return Err(crate::error::js_error("output sink must be an object"));
        }
        let on_file = js_sys::Reflect::get(&value, &JsValue::from_str("onFile"))?
            .dyn_into::<Function>()
            .map_err(|_| crate::error::js_error("outputSink.onFile must be a function"))?;
        Ok(Self {
            target: value,
            on_file,
        })
    }

    pub fn write(&self, file: WebOutputFile) -> Result<(), JsValue> {
        let bytes = js_sys::Uint8Array::from(file.bytes.as_slice());
        self.on_file
            .call2(&self.target, &JsValue::from_str(&file.path), &bytes)
            .map(|_| ())
    }
}
