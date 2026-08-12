//! JS-callback-backed [`DataSource`] for browser cross-archive migration.
//!
//! Constructed from a JS object with five methods:
//! - `readFull(path: string) -> Promise<Uint8Array>`
//! - `readRange(path: string, offset: number, len: number) -> Promise<Uint8Array>`
//! - `exists(path: string) -> Promise<boolean>`
//! - `listBundleChunks() -> Promise<string[]>`
//! - `listPackages() -> Promise<string[]>`
//!
//! Function refs are extracted once via `Reflect::get` at construction; per-call
//! cost is one `Function::call*` + one `JsFuture` await.

use hd2_migrator_io::io::{DataSource, IoFuture};
use js_sys::{Array, Function, Promise, Uint8Array};
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;

pub struct JsDataSource {
    target: JsValue,
    read_full: Function,
    read_range: Function,
    exists: Function,
    list_bundle_chunks: Function,
    list_packages: Function,
}

impl JsDataSource {
    pub fn from_js(value: JsValue) -> Result<Self, JsValue> {
        if !value.is_object() {
            return Err(crate::error::js_error("data_source must be an object"));
        }
        let read_full = get_function(&value, "readFull")?;
        let read_range = get_function(&value, "readRange")?;
        let exists = get_function(&value, "exists")?;
        let list_bundle_chunks = get_function(&value, "listBundleChunks")?;
        let list_packages = get_function(&value, "listPackages")?;
        Ok(Self {
            target: value,
            read_full,
            read_range,
            exists,
            list_bundle_chunks,
            list_packages,
        })
    }
}

impl DataSource for JsDataSource {
    fn read_full<'a>(&'a self, path: &'a str) -> IoFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let raw = self
                .read_full
                .call1(&self.target, &JsValue::from_str(path))
                .map_err(|e| eyre::eyre!("readFull threw: {}", js_err_string(&e)))?;
            let value = await_promise(raw, "readFull").await?;
            uint8_array_to_vec(value, "readFull")
        })
    }

    fn read_range<'a>(&'a self, path: &'a str, offset: u64, len: u64) -> IoFuture<'a, Vec<u8>> {
        Box::pin(async move {
            let args = Array::new();
            args.push(&JsValue::from_str(path));
            args.push(&JsValue::from_f64(offset as f64));
            args.push(&JsValue::from_f64(len as f64));
            let raw = self
                .read_range
                .apply(&self.target, &args)
                .map_err(|e| eyre::eyre!("readRange threw: {}", js_err_string(&e)))?;
            let value = await_promise(raw, "readRange").await?;
            uint8_array_to_vec(value, "readRange")
        })
    }

    fn exists<'a>(&'a self, path: &'a str) -> IoFuture<'a, bool> {
        Box::pin(async move {
            let raw = self
                .exists
                .call1(&self.target, &JsValue::from_str(path))
                .map_err(|e| eyre::eyre!("exists threw: {}", js_err_string(&e)))?;
            let value = await_promise(raw, "exists").await?;
            Ok(value.as_bool().unwrap_or(false))
        })
    }

    fn list_bundle_chunks<'a>(&'a self) -> IoFuture<'a, Vec<String>> {
        Box::pin(async move {
            let raw = self
                .list_bundle_chunks
                .call0(&self.target)
                .map_err(|e| eyre::eyre!("listBundleChunks threw: {}", js_err_string(&e)))?;
            let value = await_promise(raw, "listBundleChunks").await?;
            string_array_to_vec(value, "listBundleChunks")
        })
    }

    fn list_packages<'a>(&'a self) -> IoFuture<'a, Vec<String>> {
        Box::pin(async move {
            let raw = self
                .list_packages
                .call0(&self.target)
                .map_err(|e| eyre::eyre!("listPackages threw: {}", js_err_string(&e)))?;
            let value = await_promise(raw, "listPackages").await?;
            string_array_to_vec(value, "listPackages")
        })
    }
}

async fn await_promise(value: JsValue, label: &str) -> hd2_migrator_io::Result<JsValue> {
    let promise = Promise::from(value);
    JsFuture::from(promise)
        .await
        .map_err(|e| eyre::eyre!("{label} rejected: {}", js_err_string(&e)))
}

fn uint8_array_to_vec(value: JsValue, label: &str) -> hd2_migrator_io::Result<Vec<u8>> {
    let array: Uint8Array = value
        .dyn_into()
        .map_err(|_| eyre::eyre!("{label} must resolve to a Uint8Array"))?;
    Ok(array.to_vec())
}

fn string_array_to_vec(value: JsValue, label: &str) -> hd2_migrator_io::Result<Vec<String>> {
    let array: Array = value
        .dyn_into()
        .map_err(|_| eyre::eyre!("{label} must resolve to a string array"))?;
    (0..array.length())
        .map(|index| {
            array
                .get(index)
                .as_string()
                .ok_or_else(|| eyre::eyre!("{label} entry must be string"))
        })
        .collect()
}

fn get_function(obj: &JsValue, key: &str) -> Result<Function, JsValue> {
    let value = js_sys::Reflect::get(obj, &JsValue::from_str(key))?;
    value
        .dyn_into::<Function>()
        .map_err(|_| crate::error::js_error(format!("data_source.{key} must be a function")))
}

fn js_err_string(value: &JsValue) -> String {
    if let Some(s) = value.as_string() {
        return s;
    }
    if let Ok(message) = js_sys::Reflect::get(value, &JsValue::from_str("message"))
        && let Some(s) = message.as_string()
    {
        return s;
    }
    format!("{value:?}")
}
