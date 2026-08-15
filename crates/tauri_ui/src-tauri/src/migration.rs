mod output;
mod patch;

use self::output::{create_zip, finish_zip, write_zip_entry};
use self::patch::{LoadedPatch, PatchDescriptor, load_patch};
use hd2_migrator_io::io::NativeDataSource;
use hd2_migrator_io::web::{
    self, UnitRepatchOptions, VariantMigrationCallbacks, WebEquipmentInspection,
    WebEquipmentOption, WebMigrationSummary, WebOutputFile, WebProgress, WebUnifiedMigrateOptions,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPatchRequest {
    paths: Vec<PathBuf>,
    data_dir: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectPatchResult {
    patch: PatchDescriptor,
    inspection: WebEquipmentInspection,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrateRequest {
    patch_paths: Vec<PathBuf>,
    data_dir: PathBuf,
    output_path: PathBuf,
    options: WebUnifiedMigrateOptions,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RepatchRequest {
    patch_paths: Vec<PathBuf>,
    data_dir: PathBuf,
    output_path: PathBuf,
    options: UnitRepatchOptions,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationProgressEvent {
    target_name: String,
    target_hash: String,
    stage: String,
    kind: ProgressKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
enum ProgressKind {
    TargetStart,
    Stage,
    TargetFinish,
}

#[tauri::command]
pub fn load_equipment_options() -> Result<Vec<WebEquipmentOption>, String> {
    web::list_equipment_options().map_err(display_error)
}

#[tauri::command]
pub async fn inspect_patch(request: InspectPatchRequest) -> Result<InspectPatchResult, String> {
    tauri::async_runtime::spawn_blocking(move || inspect_patch_blocking(request))
        .await
        .map_err(|error| format!("Patch inspection task failed: {error}"))?
}

#[tauri::command]
pub async fn migrate_equipment(
    request: MigrateRequest,
    app: AppHandle,
) -> Result<WebMigrationSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        pollster::block_on(migrate_equipment_blocking(request, app))
    })
    .await
    .map_err(|error| format!("Migration task failed: {error}"))?
}

#[tauri::command]
pub async fn repatch_mod(request: RepatchRequest) -> Result<web::UnitRepatchSummary, String> {
    tauri::async_runtime::spawn_blocking(move || pollster::block_on(repatch_mod_blocking(request)))
        .await
        .map_err(|error| format!("Repatch task failed: {error}"))?
}

fn inspect_patch_blocking(request: InspectPatchRequest) -> Result<InspectPatchResult, String> {
    let patch = load_patch(&request.paths)?;
    let inspection = inspect_with_optional_source(patch.bytes(), request.data_dir)?;
    Ok(InspectPatchResult {
        patch: patch.descriptor(),
        inspection,
    })
}

fn inspect_with_optional_source(
    patch: &web::PatchBytes,
    data_dir: Option<PathBuf>,
) -> Result<WebEquipmentInspection, String> {
    let result = match data_dir {
        Some(path) => pollster::block_on(web::inspect_equipment_with_source(
            patch,
            &NativeDataSource::new(path),
        )),
        None => web::inspect_equipment(patch),
    };
    result.map_err(display_error)
}

async fn migrate_equipment_blocking(
    request: MigrateRequest,
    app: AppHandle,
) -> Result<WebMigrationSummary, String> {
    validate_output_request(&request.data_dir, &request.output_path)?;
    let patch = load_patch(&request.patch_paths)?;
    let source = NativeDataSource::new(request.data_dir);
    let progress = TauriProgress::new(app);
    let mut zip = create_zip(&request.output_path)?;
    let callbacks = VariantMigrationCallbacks::new(Some(&progress), |file: WebOutputFile| {
        write_zip_entry(&mut zip, &file.path, &file.bytes)
    });
    let summary =
        web::migrate_variants_to_sink(patch.into_bytes(), request.options, &source, callbacks)
            .await
            .map_err(display_error)?;
    finish_zip(zip)?;
    Ok(summary)
}

async fn repatch_mod_blocking(request: RepatchRequest) -> Result<web::UnitRepatchSummary, String> {
    validate_output_request(&request.data_dir, &request.output_path)?;
    let patch = load_patch(&request.patch_paths)?;
    let result = web::repatch_units(
        patch.name(),
        &patch.bytes().toc,
        request.options,
        &NativeDataSource::new(request.data_dir),
    )
    .await
    .map_err(display_error)?;
    write_repatched_zip(&request.output_path, &patch, result.toc)?;
    Ok(result.summary)
}

fn write_repatched_zip(
    output_path: &Path,
    patch: &LoadedPatch,
    toc: Vec<u8>,
) -> Result<(), String> {
    let mut zip = create_zip(output_path)?;
    write_zip_entry(&mut zip, patch.name(), &toc).map_err(display_error)?;
    write_sidecar(&mut zip, patch, ".gpu_resources", &patch.bytes().gpu)?;
    write_sidecar(&mut zip, patch, ".stream", &patch.bytes().stream)?;
    finish_zip(zip)
}

fn write_sidecar(
    zip: &mut output::OutputZip,
    patch: &LoadedPatch,
    suffix: &str,
    bytes: &[u8],
) -> Result<(), String> {
    if bytes.is_empty() {
        return Ok(());
    }
    write_zip_entry(zip, &format!("{}{suffix}", patch.name()), bytes).map_err(display_error)
}

fn validate_output_request(data_dir: &Path, output_path: &Path) -> Result<(), String> {
    if !data_dir.is_dir() {
        return Err(format!(
            "Game data directory does not exist: {}",
            data_dir.display()
        ));
    }
    if output_path.as_os_str().is_empty() {
        return Err("Output path is required".to_owned());
    }
    Ok(())
}

struct TauriProgress {
    app: AppHandle,
}

impl TauriProgress {
    fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn emit(&self, kind: ProgressKind, name: &str, hash: &str, stage: &str) {
        let event = MigrationProgressEvent {
            target_name: name.to_owned(),
            target_hash: hash.to_owned(),
            stage: stage.to_owned(),
            kind,
        };
        let _ = self.app.emit("migration://progress", event);
    }
}

impl WebProgress for TauriProgress {
    fn target_started(&self, name: &str, hash: &str) {
        self.emit(ProgressKind::TargetStart, name, hash, "");
    }

    fn stage(&self, name: &str, stage: &str) {
        self.emit(ProgressKind::Stage, name, "", stage);
    }

    fn target_finished(&self, name: &str) {
        self.emit(ProgressKind::TargetFinish, name, "", "");
    }
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}
