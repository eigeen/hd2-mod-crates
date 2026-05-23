use crate::error::{WasmResult, js_error};
use crate::zip_store::zip_store;
use hd2_migrator_io::{
    index::ArchiveIndex,
    web::{self, PatchBytes, WebMigrateOptions, WebMigrationSummary, WebTargetOption},
};
use js_sys::{Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;

const DEFAULT_CATEGORY: &str = "Armor";

#[wasm_bindgen]
pub fn builtin_target_options(category: Option<String>) -> WasmResult<JsValue> {
    let category = category.unwrap_or_else(|| DEFAULT_CATEGORY.to_string());
    let targets = ArchiveIndex::builtin()
        .category(&category)
        .ok_or_else(|| js_error(format!("category {category:?} not found")))?;
    let options = targets
        .iter()
        .map(|target| WebTargetOption {
            hash: target.hash.clone(),
            name: target.name.clone(),
        })
        .collect::<Vec<_>>();
    serde_wasm_bindgen::to_value(&options).map_err(js_error)
}

#[wasm_bindgen]
pub fn detect_source(
    patch_name: String,
    toc: Vec<u8>,
    gpu: Vec<u8>,
    stream: Vec<u8>,
    category: Option<String>,
) -> WasmResult<JsValue> {
    let category = category.unwrap_or_else(|| DEFAULT_CATEGORY.to_string());
    let patch = PatchBytes {
        name: patch_name,
        toc,
        gpu,
        stream,
    };
    let source = web::detect_source_archive(&category, &patch).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&source).map_err(js_error)
}

#[wasm_bindgen]
pub fn migrate_one(
    patch_name: String,
    toc: Vec<u8>,
    gpu: Vec<u8>,
    stream: Vec<u8>,
    options: JsValue,
    category: Option<String>,
) -> WasmResult<JsValue> {
    let request = parse_options(options)?;
    if request.target_hashes.len() != 1 {
        return Err(js_error("migrate_one requires exactly one target"));
    }
    run_migration(patch_name, toc, gpu, stream, request, category, true)
}

#[wasm_bindgen]
pub fn migrate_many(
    patch_name: String,
    toc: Vec<u8>,
    gpu: Vec<u8>,
    stream: Vec<u8>,
    options: JsValue,
    category: Option<String>,
) -> WasmResult<JsValue> {
    let request = parse_options(options)?;
    run_migration(patch_name, toc, gpu, stream, request, category, false)
}

fn run_migration(
    patch_name: String,
    toc: Vec<u8>,
    gpu: Vec<u8>,
    stream: Vec<u8>,
    options: WebMigrateOptions,
    category: Option<String>,
    single_target: bool,
) -> WasmResult<JsValue> {
    let category = category.unwrap_or_else(|| DEFAULT_CATEGORY.to_string());
    let patch = PatchBytes {
        name: patch_name,
        toc,
        gpu,
        stream,
    };
    let bundle = if single_target {
        web::migrate_one(&category, patch, options)
    } else {
        web::migrate_many(&category, patch, options)
    }
    .map_err(js_error)?;
    migration_result(zip_store(&bundle.files), bundle.summary)
}

fn parse_options(value: JsValue) -> WasmResult<WebMigrateOptions> {
    serde_wasm_bindgen::from_value(value).map_err(js_error)
}

fn migration_result(zip_bytes: Vec<u8>, summary: WebMigrationSummary) -> WasmResult<JsValue> {
    let result = Object::new();
    let zip = Uint8Array::from(zip_bytes.as_slice());
    Reflect::set(&result, &JsValue::from_str("zipBytes"), &zip)?;
    Reflect::set(
        &result,
        &JsValue::from_str("summary"),
        &serde_wasm_bindgen::to_value(&summary).map_err(js_error)?,
    )?;
    Ok(result.into())
}
