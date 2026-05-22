use crate::error::{WasmResult, js_error};
use crate::zip_store::zip_store;
use hd2_migrator_io::{
    archive::dsar,
    constants::{DSAR_MAGIC, LEGACY_MAGIC},
    index::ArchiveIndex,
    web::{
        self, PatchBytes, WebArchiveMetadata, WebGameMetadata, WebMigrateOptions,
        WebMigrationSummary,
    },
};
use js_sys::{Object, Reflect, Uint8Array};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BrowserArchiveInput {
    hash: String,
    name: String,
    toc: Vec<u8>,
}

#[wasm_bindgen]
pub fn builtin_target_options(category: Option<String>) -> WasmResult<JsValue> {
    let category = category.unwrap_or_else(|| "Armor".to_string());
    let targets = ArchiveIndex::builtin()
        .category(&category)
        .ok_or_else(|| js_error(format!("category {category:?} not found")))?;
    let options = targets
        .iter()
        .map(|target| web::WebTargetOption {
            hash: target.hash.clone(),
            name: target.name.clone(),
        })
        .collect::<Vec<_>>();
    serde_wasm_bindgen::to_value(&options).map_err(js_error)
}

#[wasm_bindgen]
pub fn build_metadata(category: String, archives: JsValue) -> WasmResult<String> {
    let inputs: Vec<BrowserArchiveInput> =
        serde_wasm_bindgen::from_value(archives).map_err(js_error)?;
    let targets = inputs
        .into_iter()
        .map(web_archive_from_input)
        .collect::<WasmResult<Vec<_>>>()?;
    serde_json::to_string(&WebGameMetadata::new(category, targets)).map_err(js_error)
}

#[wasm_bindgen]
pub fn list_targets(metadata_json: &str) -> WasmResult<JsValue> {
    let metadata = parse_metadata(metadata_json)?;
    serde_wasm_bindgen::to_value(&web::list_target_options(&metadata)).map_err(js_error)
}

#[wasm_bindgen]
pub fn detect_source(
    metadata_json: &str,
    patch_name: String,
    toc: Vec<u8>,
    gpu: Vec<u8>,
    stream: Vec<u8>,
) -> WasmResult<JsValue> {
    let metadata = parse_metadata(metadata_json)?;
    let patch = PatchBytes {
        name: patch_name,
        toc,
        gpu,
        stream,
    };
    let source = web::detect_source_archive(&metadata, &patch).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&source).map_err(js_error)
}

#[wasm_bindgen]
pub fn migrate_one(
    metadata_json: &str,
    patch_name: String,
    toc: Vec<u8>,
    gpu: Vec<u8>,
    stream: Vec<u8>,
    options: JsValue,
) -> WasmResult<JsValue> {
    let request = parse_options(options)?;
    if request.target_hashes.len() != 1 {
        return Err(js_error("migrate_one requires exactly one target"));
    }
    run_migration(metadata_json, patch_name, toc, gpu, stream, request, true)
}

#[wasm_bindgen]
pub fn migrate_many(
    metadata_json: &str,
    patch_name: String,
    toc: Vec<u8>,
    gpu: Vec<u8>,
    stream: Vec<u8>,
    options: JsValue,
) -> WasmResult<JsValue> {
    let request = parse_options(options)?;
    run_migration(metadata_json, patch_name, toc, gpu, stream, request, false)
}

fn run_migration(
    metadata_json: &str,
    patch_name: String,
    toc: Vec<u8>,
    gpu: Vec<u8>,
    stream: Vec<u8>,
    options: WebMigrateOptions,
    single_target: bool,
) -> WasmResult<JsValue> {
    let metadata = parse_metadata(metadata_json)?;
    let patch = PatchBytes {
        name: patch_name,
        toc,
        gpu,
        stream,
    };
    let bundle = if single_target {
        web::migrate_one(&metadata, patch, options)
    } else {
        web::migrate_many(&metadata, patch, options)
    }
    .map_err(js_error)?;
    migration_result(zip_store(&bundle.files), bundle.summary)
}

fn parse_metadata(metadata_json: &str) -> WasmResult<WebGameMetadata> {
    serde_json::from_str(metadata_json).map_err(js_error)
}

fn parse_options(value: JsValue) -> WasmResult<WebMigrateOptions> {
    serde_wasm_bindgen::from_value(value).map_err(js_error)
}

fn web_archive_from_input(input: BrowserArchiveInput) -> WasmResult<WebArchiveMetadata> {
    let toc = decode_archive_member(&input.toc)?;
    WebArchiveMetadata::from_toc_bytes(input.hash, input.name, &toc).map_err(js_error)
}

fn decode_archive_member(bytes: &[u8]) -> WasmResult<Vec<u8>> {
    match magic(bytes) {
        Some(LEGACY_MAGIC) => Ok(bytes.to_vec()),
        Some(DSAR_MAGIC) => dsar::decompress(bytes).map_err(js_error),
        Some(value) => Err(js_error(format!("unsupported archive magic 0x{value:08X}"))),
        None => Err(js_error("archive member is too small")),
    }
}

fn magic(bytes: &[u8]) -> Option<u32> {
    let head = bytes.get(0..4)?;
    Some(u32::from_le_bytes([head[0], head[1], head[2], head[3]]))
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
