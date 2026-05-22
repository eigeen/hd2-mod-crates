use js_sys::{Object, Reflect};
use wasm_bindgen::prelude::*;

pub type WasmResult<T> = Result<T, JsValue>;

pub fn js_error(message: impl ToString) -> JsValue {
    let error = Object::new();
    set_string(&error, "message", &message.to_string());
    error.into()
}

fn set_string(object: &Object, key: &str, value: &str) {
    let _ = Reflect::set(object, &JsValue::from_str(key), &JsValue::from_str(value));
}
