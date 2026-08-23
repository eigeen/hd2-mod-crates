use crate::data_source::JsDataSource;
use crate::error::{WasmResult, js_error};
use crate::output_sink::JsOutputSink;
use crate::progress::JsProgress;
use crate::zip_store::zip_store;
use hd2_migrator_io::{
    index::ArchiveIndex,
    target_exclusions::is_default_excluded_target,
    web::{self, PatchBytes, WebMigrateOptions, WebMigrationSummary, WebTargetOption},
};
use js_sys::{Array, Object, Reflect, Uint8Array};
use serde::Serialize;
use wasm_bindgen::prelude::*;

const DEFAULT_CATEGORY: &str = "Armor";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct JsSidecarRequirements {
    gpu: String,
    stream: String,
}

#[wasm_bindgen]
pub fn patch_sidecar_requirements(toc: Vec<u8>) -> WasmResult<JsValue> {
    let required =
        hd2_migrator_io::archive::sidecar::patch_sidecar_requirements(&toc).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&JsSidecarRequirements {
        gpu: required.gpu.to_string(),
        stream: required.stream.to_string(),
    })
    .map_err(js_error)
}

#[wasm_bindgen]
pub fn builtin_target_options(category: Option<String>) -> WasmResult<JsValue> {
    let category = category.unwrap_or_else(|| DEFAULT_CATEGORY.to_string());
    let targets = ArchiveIndex::builtin()
        .category(&category)
        .ok_or_else(|| js_error(format!("category {category:?} not found")))?;
    let options = targets
        .iter()
        .map(|target| WebTargetOption {
            excluded: is_default_excluded_target(&target.hash, &target.name),
            hash: target.hash.clone(),
            name: target.name.clone(),
        })
        .collect::<Vec<_>>();
    serde_wasm_bindgen::to_value(&options).map_err(js_error)
}

#[wasm_bindgen]
pub fn builtin_equipment_options() -> WasmResult<JsValue> {
    let options = web::list_equipment_options().map_err(js_error)?;
    serde_wasm_bindgen::to_value(&options).map_err(js_error)
}

#[wasm_bindgen]
pub fn merge_patches(inputs: JsValue, output_name: String) -> WasmResult<JsValue> {
    if !Array::is_array(&inputs) {
        return Err(js_error("merge inputs must be an array"));
    }
    let inputs = Array::from(&inputs)
        .iter()
        .map(|value| patch_bytes_from_js(&value))
        .collect::<WasmResult<Vec<_>>>()?;
    let result = web::merge_patches(inputs, output_name).map_err(js_error)?;
    patch_merge_result(result)
}

#[wasm_bindgen]
pub fn detect_source(
    patch_name: String,
    toc: Vec<u8>,
    category: Option<String>,
) -> WasmResult<JsValue> {
    let category = category.unwrap_or_else(|| DEFAULT_CATEGORY.to_string());
    // 仅 toc 参与识别；gpu/stream 不需要传入，避免无谓地把数百 MB 拷贝进 WASM 内存导致 OOM。
    let patch = toc_only_patch(patch_name, toc);
    let source = web::detect_source_archive(&category, &patch).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&source).map_err(js_error)
}

#[wasm_bindgen]
pub fn inspect_patch(
    patch_name: String,
    toc: Vec<u8>,
    category: Option<String>,
) -> WasmResult<JsValue> {
    let category = category.unwrap_or_else(|| DEFAULT_CATEGORY.to_string());
    // Inspection only needs the TOC. One combined call avoids copying a large TOC twice.
    let patch = toc_only_patch(patch_name, toc);
    let inspection = web::inspect_patch(&category, &patch).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&inspection).map_err(js_error)
}

#[wasm_bindgen]
pub fn inspect_equipment(patch_name: String, toc: Vec<u8>) -> WasmResult<JsValue> {
    let patch = toc_only_patch(patch_name, toc);
    let inspection = web::inspect_equipment(&patch).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&inspection).map_err(js_error)
}

#[wasm_bindgen]
pub async fn inspect_equipment_with_source(
    patch_name: String,
    toc: Vec<u8>,
    data_source: JsValue,
) -> WasmResult<JsValue> {
    let patch = toc_only_patch(patch_name, toc);
    let source = JsDataSource::from_js(data_source)?;
    let inspection = web::inspect_equipment_with_source(&patch, &source)
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::to_value(&inspection).map_err(js_error)
}

#[wasm_bindgen]
pub fn analyze_equipment_patch(patch_name: String, toc: Vec<u8>) -> WasmResult<JsValue> {
    let patch = toc_only_patch(patch_name, toc);
    let analysis = web::analyze_equipment_patch(&patch).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&analysis).map_err(js_error)
}

#[wasm_bindgen]
pub fn preview_equipment_mapping(
    patch_name: String,
    toc: Vec<u8>,
    mapping: JsValue,
) -> WasmResult<JsValue> {
    let patch = toc_only_patch(patch_name, toc);
    let mapping: web::WebMigrationMapping =
        serde_wasm_bindgen::from_value(mapping).map_err(js_error)?;
    let preview = web::preview_equipment_mapping(&patch, &mapping).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&preview).map_err(js_error)
}

#[wasm_bindgen]
pub fn preview_equipment_mappings(
    patch_name: String,
    toc: Vec<u8>,
    mappings: JsValue,
) -> WasmResult<JsValue> {
    let patch = toc_only_patch(patch_name, toc);
    let mappings: Vec<web::WebMigrationMapping> =
        serde_wasm_bindgen::from_value(mappings).map_err(js_error)?;
    let previews = web::preview_equipment_mappings(&patch, &mappings).map_err(js_error)?;
    serde_wasm_bindgen::to_value(&previews).map_err(js_error)
}

#[wasm_bindgen]
pub async fn analyze_equipment_patch_with_source(
    patch_name: String,
    toc: Vec<u8>,
    data_source: JsValue,
) -> WasmResult<JsValue> {
    let patch = toc_only_patch(patch_name, toc);
    let source = JsDataSource::from_js(data_source)?;
    let analysis = web::analyze_equipment_patch_with_source(&patch, &source)
        .await
        .map_err(js_error)?;
    serde_wasm_bindgen::to_value(&analysis).map_err(js_error)
}

/// Full cross-archive migration backed by a JS-supplied `DataSource`.
///
/// The `data_source` argument is a JS object with `readFull`, `readRange`,
/// `exists`, and `listBundleChunks` methods (see `data_source.rs` for the
/// expected shape). The optional `progress` argument is a JS object with
/// `onTargetStart`/`onStage`/`onTargetFinish` callbacks.
#[wasm_bindgen]
pub async fn migrate_cross_archive(
    patch: JsValue,
    request: JsValue,
    data_source: JsValue,
    progress: JsValue,
) -> WasmResult<JsValue> {
    let category =
        optional_string(&request, "category")?.unwrap_or_else(|| DEFAULT_CATEGORY.to_string());
    let request: WebMigrateOptions = parse_options(property(&request, "options")?)?;
    let source = JsDataSource::from_js(data_source)?;
    let progress_sink = JsProgress::from_js(progress)?;
    let patch = patch_bytes_from_js(&patch)?;
    let bundle =
        web::migrate_many_with_source(&category, patch, request, &source, Some(&progress_sink))
            .await
            .map_err(js_error)?;
    migration_result(zip_store(&bundle.files), bundle.summary)
}

#[wasm_bindgen]
pub async fn migrate_equipment_variants(
    patch: JsValue,
    options: JsValue,
    data_source: JsValue,
    callbacks: JsValue,
) -> WasmResult<JsValue> {
    let request: web::WebUnifiedMigrateOptions =
        serde_wasm_bindgen::from_value(options).map_err(js_error)?;
    let source = JsDataSource::from_js(data_source)?;
    let progress_sink = JsProgress::from_js(callbacks.clone())?;
    let output_sink = JsOutputSink::from_js(callbacks)?;
    let patch = patch_bytes_from_js(&patch)?;
    let callbacks = web::VariantMigrationCallbacks::new(Some(&progress_sink), |file| {
        output_sink
            .write(file)
            .map_err(|_| eyre::eyre!("web output sink rejected a file"))
    });
    let summary = web::migrate_variants_to_sink(patch, request, &source, callbacks)
        .await
        .map_err(js_error)?;
    summary_result(summary)
}

/// Repatch Unit resources using the latest Unit structures from game data.
///
/// Only Unit TOC metadata is updated. GPU and stream sidecars remain unchanged.
#[wasm_bindgen]
pub async fn repatch_units(
    patch: JsValue,
    options: JsValue,
    data_source: JsValue,
    callbacks: JsValue,
) -> WasmResult<JsValue> {
    let options: web::UnitRepatchOptions =
        serde_wasm_bindgen::from_value(options).map_err(js_error)?;
    let source = JsDataSource::from_js(data_source)?;
    let progress = JsProgress::from_js(callbacks)?;
    let patch = patch_bytes_from_js(&patch)?;
    let output = web::repatch_patch_with_progress(patch, options, &source, Some(&progress))
        .await
        .map_err(js_error)?;
    let result = Object::new();
    let toc = Uint8Array::from(output.toc.as_slice());
    Reflect::set(&result, &JsValue::from_str("tocBytes"), &toc)?;
    set_optional_bytes(&result, "gpuBytes", output.gpu.as_deref())?;
    set_optional_bytes(&result, "streamBytes", output.stream.as_deref())?;
    Reflect::set(
        &result,
        &JsValue::from_str("summary"),
        &serde_wasm_bindgen::to_value(&output.summary).map_err(js_error)?,
    )?;
    Ok(result.into())
}

fn set_optional_bytes(result: &Object, key: &str, bytes: Option<&[u8]>) -> WasmResult<()> {
    let value = bytes
        .map(|bytes| Uint8Array::from(bytes).into())
        .unwrap_or(JsValue::NULL);
    Reflect::set(result, &JsValue::from_str(key), &value)?;
    Ok(())
}

fn toc_only_patch(name: String, toc: Vec<u8>) -> PatchBytes {
    PatchBytes {
        name,
        toc,
        gpu: Vec::new(),
        stream: Vec::new(),
    }
}

fn patch_bytes_from_js(patch: &JsValue) -> WasmResult<PatchBytes> {
    Ok(PatchBytes {
        name: required_string(patch, "name")?,
        toc: required_bytes(patch, "toc")?,
        gpu: required_bytes(patch, "gpu")?,
        stream: required_bytes(patch, "stream")?,
    })
}

fn required_string(value: &JsValue, key: &str) -> WasmResult<String> {
    property(value, key)?
        .as_string()
        .ok_or_else(|| js_error(format!("patch.{key} must be a string")))
}

fn optional_string(value: &JsValue, key: &str) -> WasmResult<Option<String>> {
    let value = property(value, key)?;
    if value.is_null() || value.is_undefined() {
        return Ok(None);
    }
    value
        .as_string()
        .map(Some)
        .ok_or_else(|| js_error(format!("{key} must be a string")))
}

fn required_bytes(value: &JsValue, key: &str) -> WasmResult<Vec<u8>> {
    let bytes = property(value, key)?;
    if !bytes.is_instance_of::<Uint8Array>() {
        return Err(js_error(format!("patch.{key} must be a Uint8Array")));
    }
    Ok(Uint8Array::new(&bytes).to_vec())
}

fn property(value: &JsValue, key: &str) -> WasmResult<JsValue> {
    Reflect::get(value, &JsValue::from_str(key))
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

fn summary_result(summary: WebMigrationSummary) -> WasmResult<JsValue> {
    let result = Object::new();
    Reflect::set(
        &result,
        &JsValue::from_str("summary"),
        &serde_wasm_bindgen::to_value(&summary).map_err(js_error)?,
    )?;
    Ok(result.into())
}

fn patch_merge_result(result: web::PatchMergeResult) -> WasmResult<JsValue> {
    let output = Object::new();
    Reflect::set(
        &output,
        &JsValue::from_str("patch"),
        &patch_bytes_to_js(result.patch)?,
    )?;
    Reflect::set(
        &output,
        &JsValue::from_str("summary"),
        &serde_wasm_bindgen::to_value(&result.summary).map_err(js_error)?,
    )?;
    Ok(output.into())
}

fn patch_bytes_to_js(patch: PatchBytes) -> WasmResult<JsValue> {
    let output = Object::new();
    Reflect::set(
        &output,
        &JsValue::from_str("name"),
        &JsValue::from_str(&patch.name),
    )?;
    Reflect::set(
        &output,
        &JsValue::from_str("toc"),
        &Uint8Array::from(patch.toc.as_slice()),
    )?;
    Reflect::set(
        &output,
        &JsValue::from_str("gpu"),
        &Uint8Array::from(patch.gpu.as_slice()),
    )?;
    Reflect::set(
        &output,
        &JsValue::from_str("stream"),
        &Uint8Array::from(patch.stream.as_slice()),
    )?;
    Ok(output.into())
}
