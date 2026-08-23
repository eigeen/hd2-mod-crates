mod output;
mod patch;

use self::output::{
    PatchZipContext, RepatchTocSource, create_zip, finish_zip, write_patch_to_zip,
    write_repatch_toc_to_zip, write_zip_entry_with_progress,
};
use self::patch::{LoadedPatch, PatchDescriptor, load_patch};
use crate::command_error::CommandError;
use crate::task::TaskRegistry;
use hd2_migrator_io::io::NativeDataSource;
use hd2_migrator_io::web::{
    self, ParallelVariantPatchCallbacks, UnitRepatchOptions, VariantPatchOutput,
    WebEquipmentInspection, WebEquipmentMappingPreview, WebEquipmentOption, WebEquipmentPartGraph,
    WebEquipmentPatchAnalysis, WebMigrationMapping, WebMigrationSummary, WebProgress,
    WebUnifiedMigrateOptions,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::State;
use tauri::ipc::Channel;

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
    equipment_graph: WebEquipmentPartGraph,
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
pub struct PreviewMappingRequest {
    patch_paths: Vec<PathBuf>,
    mapping: WebMigrationMapping,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewMappingsRequest {
    patch_paths: Vec<PathBuf>,
    mappings: Vec<WebMigrationMapping>,
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
    completed_bytes: u64,
    target_name: String,
    target_hash: String,
    stage: String,
    kind: ProgressKind,
    total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
enum ProgressKind {
    OutputProgress,
    TargetStart,
    Stage,
    TargetFinish,
}

#[tauri::command]
pub fn load_equipment_options() -> Result<Vec<WebEquipmentOption>, CommandError> {
    web::list_equipment_options()
        .map_err(|error| CommandError::from_display("equipment.loadFailed", error))
}

#[tauri::command]
pub async fn inspect_patch(
    request: InspectPatchRequest,
) -> Result<InspectPatchResult, CommandError> {
    tauri::async_runtime::spawn_blocking(move || inspect_patch_blocking(request))
        .await
        .map_err(|error| CommandError::from_display("task.joinFailed", error))?
        .map_err(|error| CommandError::new("patch.inspectFailed", error))
}

#[tauri::command]
pub async fn preview_equipment_mapping(
    request: PreviewMappingRequest,
) -> Result<WebEquipmentMappingPreview, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let patch = load_patch(&request.patch_paths)?;
        web::preview_equipment_mapping(patch.bytes(), &request.mapping).map_err(display_error)
    })
    .await
    .map_err(|error| CommandError::from_display("task.joinFailed", error))?
    .map_err(|error| CommandError::new("migration.failed", error))
}

#[tauri::command]
pub async fn preview_equipment_mappings(
    request: PreviewMappingsRequest,
) -> Result<Vec<WebEquipmentMappingPreview>, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let patch = load_patch(&request.patch_paths)?;
        web::preview_equipment_mappings(patch.bytes(), &request.mappings).map_err(display_error)
    })
    .await
    .map_err(|error| CommandError::from_display("task.joinFailed", error))?
    .map_err(|error| CommandError::new("migration.failed", error))
}

#[tauri::command]
pub async fn migrate_equipment(
    request: MigrateRequest,
    task_id: String,
    on_progress: Channel<MigrationProgressEvent>,
    tasks: State<'_, TaskRegistry>,
) -> Result<WebMigrationSummary, CommandError> {
    let lease = tasks.register(task_id)?;
    let cancellation = lease.cancellation();
    let task_cancellation = Arc::clone(&cancellation);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let progress = DesktopProgress::new(on_progress, task_cancellation);
        pollster::block_on(migrate_equipment_blocking(request, Some(&progress)))
    })
    .await
    .map_err(|error| CommandError::from_display("task.joinFailed", error))?;
    task_result(result, &cancellation, "migration.failed")
}

#[tauri::command]
pub async fn repatch_mod(
    request: RepatchRequest,
    task_id: String,
    on_progress: Channel<MigrationProgressEvent>,
    tasks: State<'_, TaskRegistry>,
) -> Result<web::UnitRepatchSummary, CommandError> {
    let lease = tasks.register(task_id)?;
    let cancellation = lease.cancellation();
    let task_cancellation = Arc::clone(&cancellation);
    let result = tauri::async_runtime::spawn_blocking(move || {
        let _lease = lease;
        let progress = DesktopProgress::new(on_progress, task_cancellation);
        pollster::block_on(repatch_mod_blocking(request, Some(&progress)))
    })
    .await
    .map_err(|error| CommandError::from_display("task.joinFailed", error))?;
    task_result(result, &cancellation, "repatch.failed")
}

#[tauri::command]
pub fn cancel_task(task_id: String, tasks: State<'_, TaskRegistry>) -> bool {
    tasks.cancel(&task_id)
}

fn inspect_patch_blocking(request: InspectPatchRequest) -> Result<InspectPatchResult, String> {
    let patch = load_patch(&request.paths)?;
    let analysis = analyze_with_optional_source(patch.bytes(), request.data_dir)?;
    Ok(InspectPatchResult {
        patch: patch.descriptor(),
        inspection: analysis.inspection,
        equipment_graph: analysis.equipment_graph,
    })
}

fn analyze_with_optional_source(
    patch: &web::PatchBytes,
    data_dir: Option<PathBuf>,
) -> Result<WebEquipmentPatchAnalysis, String> {
    let result = match data_dir {
        Some(path) => pollster::block_on(web::analyze_equipment_patch_with_source(
            patch,
            &NativeDataSource::new(path),
        )),
        None => web::analyze_equipment_patch(patch),
    };
    result.map_err(display_error)
}

async fn migrate_equipment_blocking(
    request: MigrateRequest,
    progress: Option<&DesktopProgress>,
) -> Result<WebMigrationSummary, String> {
    validate_output_request(&request.data_dir, &request.output_path)?;
    let patch = load_patch(&request.patch_paths)?;
    let source = NativeDataSource::new(request.data_dir);
    let mut zip = create_zip(&request.output_path)?;
    let parallel_progress = progress.map(|value| value as &(dyn WebProgress + Sync));
    let callbacks =
        ParallelVariantPatchCallbacks::new(parallel_progress, |mut output: VariantPatchOutput| {
            write_patch_to_zip(
                &mut zip,
                &mut output.patch,
                PatchZipContext {
                    directory: &output.directory,
                    progress: progress.map(|value| value as &dyn output::OutputProgress),
                    suffix: &output.suffix,
                },
            )
        });
    let summary = web::migrate_variants_to_patch_sink_parallel(
        patch.into_bytes(),
        request.options,
        &source,
        callbacks,
    )
    .await
    .map_err(display_error)?;
    finish_zip(zip)?;
    Ok(summary)
}

async fn repatch_mod_blocking(
    request: RepatchRequest,
    progress: Option<&DesktopProgress>,
) -> Result<web::UnitRepatchSummary, String> {
    validate_output_request(&request.data_dir, &request.output_path)?;
    let patch = load_patch(&request.patch_paths)?;
    let result = web::repatch_patch_with_progress(
        patch.bytes().clone(),
        request.options,
        &NativeDataSource::new(request.data_dir),
        progress.map(|value| value as &dyn WebProgress),
    )
    .await
    .map_err(display_error)?;
    write_repatched_zip(&request.output_path, &patch, &result, progress)?;
    Ok(result.summary)
}

fn write_repatched_zip(
    output_path: &Path,
    patch: &LoadedPatch,
    updated: &web::UnitRepatchResult,
    progress: Option<&DesktopProgress>,
) -> Result<(), String> {
    let mut zip = create_zip(output_path)?;
    let output_progress = progress.map(|value| value as &dyn output::OutputProgress);
    let toc = RepatchTocSource::Original(&updated.toc);
    write_repatch_toc_to_zip(&mut zip, patch.name(), toc, output_progress)
        .map_err(display_error)?;
    write_sidecar(
        &mut zip,
        &format!("{}.gpu_resources", patch.name()),
        updated.gpu.as_deref().unwrap_or(&patch.bytes().gpu),
        output_progress,
    )?;
    write_sidecar(
        &mut zip,
        &format!("{}.stream", patch.name()),
        updated.stream.as_deref().unwrap_or(&patch.bytes().stream),
        output_progress,
    )?;
    finish_zip(zip)
}

fn write_sidecar(
    zip: &mut output::OutputZip,
    path: &str,
    bytes: &[u8],
    progress: Option<&dyn output::OutputProgress>,
) -> Result<(), String> {
    write_zip_entry_with_progress(zip, path, bytes, progress).map_err(display_error)
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

struct DesktopProgress {
    channel: Channel<MigrationProgressEvent>,
    cancellation: Arc<AtomicBool>,
}

impl DesktopProgress {
    fn new(channel: Channel<MigrationProgressEvent>, cancellation: Arc<AtomicBool>) -> Self {
        Self {
            channel,
            cancellation,
        }
    }

    fn emit(
        &self,
        kind: ProgressKind,
        name: &str,
        hash: &str,
        stage: &str,
    ) -> hd2_migrator_io::Result<()> {
        self.ensure_active()?;
        let event = MigrationProgressEvent {
            completed_bytes: 0,
            target_name: name.to_owned(),
            target_hash: hash.to_owned(),
            stage: stage.to_owned(),
            kind,
            total_bytes: 0,
        };
        self.channel
            .send(event)
            .map_err(|error| eyre::eyre!("send task progress: {error}"))
    }

    fn ensure_active(&self) -> hd2_migrator_io::Result<()> {
        if self.cancellation.load(Ordering::Acquire) {
            eyre::bail!("task cancelled");
        }
        Ok(())
    }

    fn report_output_bytes(&self, completed: u64, total: u64) -> std::io::Result<()> {
        self.channel
            .send(MigrationProgressEvent {
                completed_bytes: completed,
                target_name: String::new(),
                target_hash: String::new(),
                stage: String::new(),
                kind: ProgressKind::OutputProgress,
                total_bytes: total,
            })
            .map_err(std::io::Error::other)
    }
}

impl output::OutputProgress for DesktopProgress {
    fn ensure_active(&self) -> std::io::Result<()> {
        DesktopProgress::ensure_active(self).map_err(std::io::Error::other)
    }

    fn report_bytes(&self, completed: u64, total: u64) -> std::io::Result<()> {
        self.report_output_bytes(completed, total)
    }
}

impl WebProgress for DesktopProgress {
    fn target_started(&self, name: &str, hash: &str) -> hd2_migrator_io::Result<()> {
        self.emit(ProgressKind::TargetStart, name, hash, "")
    }

    fn stage(&self, name: &str, stage: &str) -> hd2_migrator_io::Result<()> {
        self.emit(ProgressKind::Stage, name, "", stage)
    }

    fn target_finished(&self, name: &str) -> hd2_migrator_io::Result<()> {
        self.emit(ProgressKind::TargetFinish, name, "", "")
    }
}

fn task_result<T>(
    result: Result<T, String>,
    cancellation: &AtomicBool,
    failure_code: &'static str,
) -> Result<T, CommandError> {
    match result {
        Ok(value) => Ok(value),
        Err(_) if cancellation.load(Ordering::Acquire) => {
            Err(CommandError::new("task.cancelled", "Task was cancelled"))
        }
        Err(error) => Err(CommandError::new(failure_code, error)),
    }
}

fn display_error(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod task_result_tests {
    use super::*;

    #[test]
    fn cancellation_code_takes_priority_over_worker_errors() {
        let cancellation = AtomicBool::new(true);
        let error = task_result::<()>(
            Err("worker stopped".to_owned()),
            &cancellation,
            "migration.failed",
        )
        .expect_err("cancelled task");

        assert_eq!(error.code, "task.cancelled");
    }

    #[test]
    fn completed_output_takes_priority_over_late_cancellation() {
        let cancellation = AtomicBool::new(true);

        let result = task_result(Ok("committed"), &cancellation, "migration.failed");

        assert_eq!(result.expect("completed task"), "committed");
    }
}

#[cfg(test)]
mod real_data_tests {
    use super::*;
    use hd2_migrator_io::web::{UnmatchedUnitPolicy, WebMigrationMapping, WebMigrationVariant};

    #[test]
    #[ignore = "requires HD2_DESKTOP_TEST_PATCH and HD2_DESKTOP_TEST_DATA"]
    fn inspects_real_patch_with_installed_game_data() {
        let patch_path = required_path("HD2_DESKTOP_TEST_PATCH");
        let data_dir = required_path("HD2_DESKTOP_TEST_DATA");
        let result = inspect_patch_blocking(InspectPatchRequest {
            paths: vec![patch_path],
            data_dir: Some(data_dir),
        })
        .expect("inspect real patch");

        eprintln!(
            "{}",
            serde_json::to_string_pretty(&result.inspection).expect("serialize inspection")
        );
        assert!(!result.inspection.sources.is_empty());
    }

    #[test]
    #[ignore = "requires HD2_DESKTOP_TEST_PATCH, HD2_DESKTOP_TEST_DATA, and HD2_DESKTOP_TEST_OUTPUT"]
    fn migrates_real_patch_to_a_different_equipment_target() {
        let patch_path = required_path("HD2_DESKTOP_TEST_PATCH");
        let data_dir = required_path("HD2_DESKTOP_TEST_DATA");
        let output_path = output_path("real-migration.zip");
        let inspection = inspect_patch_blocking(InspectPatchRequest {
            paths: vec![patch_path.clone()],
            data_dir: Some(data_dir.clone()),
        })
        .expect("inspect real patch");
        let mapping = first_available_mapping(&inspection.inspection);
        let request = MigrateRequest {
            patch_paths: vec![patch_path],
            data_dir,
            output_path: output_path.clone(),
            options: WebUnifiedMigrateOptions {
                variants: vec![WebMigrationVariant {
                    mappings: vec![mapping],
                }],
                patch_suffix: Some(web::migration::DEFAULT_PATCH_SUFFIX.to_owned()),
                no_padding: false,
                unmatched_unit_policy: UnmatchedUnitPolicy::Keep,
                unit_behavior: Default::default(),
            },
        };

        let summary = pollster::block_on(migrate_equipment_blocking(request, None))
            .expect("migrate real patch");

        assert_eq!(summary.migrated_count, 1);
        assert!(output_path.is_file());
        assert_zip_has_entries(&output_path);
    }

    #[test]
    #[ignore = "requires HD2_DESKTOP_TEST_PATCH, HD2_DESKTOP_TEST_DATA, and HD2_DESKTOP_TEST_OUTPUT"]
    fn migrates_all_real_equipment_sources_into_one_patch() {
        let patch_path = required_path("HD2_DESKTOP_TEST_PATCH");
        let data_dir = required_path("HD2_DESKTOP_TEST_DATA");
        let output_path = output_path("real-combined-migration.zip");
        let inspection = inspect_patch_blocking(InspectPatchRequest {
            paths: vec![patch_path.clone()],
            data_dir: Some(data_dir.clone()),
        })
        .expect("inspect real patch");
        let mappings = available_mappings(&inspection.inspection);
        assert!(
            mappings.len() > 1,
            "fixture must contain mixed equipment sources"
        );
        let request = MigrateRequest {
            patch_paths: vec![patch_path],
            data_dir,
            output_path: output_path.clone(),
            options: WebUnifiedMigrateOptions {
                variants: vec![WebMigrationVariant { mappings }],
                patch_suffix: Some(web::migration::DEFAULT_PATCH_SUFFIX.to_owned()),
                no_padding: false,
                unmatched_unit_policy: UnmatchedUnitPolicy::Keep,
                unit_behavior: Default::default(),
            },
        };

        let summary = pollster::block_on(migrate_equipment_blocking(request, None))
            .expect("migrate mixed real patch");

        eprintln!(
            "{}",
            serde_json::to_string_pretty(&summary).expect("serialize migration summary")
        );
        assert_eq!(summary.migrated_count, 1);
        assert_eq!(summary.reports[0].mappings.len(), 2);
        assert_eq!(summary.reports[0].unmatched_units, 0);
        assert!(summary.reports[0].warnings.is_empty());
        assert!(output_path.is_file());
        assert_zip_has_entries(&output_path);
    }

    #[test]
    #[ignore = "requires HD2_DESKTOP_TEST_PATCH, HD2_DESKTOP_TEST_DATA, and HD2_DESKTOP_TEST_OUTPUT"]
    fn repatches_real_patch_from_installed_game_data() {
        let output_path = output_path("real-repatch.zip");
        let request = RepatchRequest {
            patch_paths: vec![required_path("HD2_DESKTOP_TEST_PATCH")],
            data_dir: required_path("HD2_DESKTOP_TEST_DATA"),
            output_path: output_path.clone(),
            options: UnitRepatchOptions::default(),
        };

        let summary =
            pollster::block_on(repatch_mod_blocking(request, None)).expect("repatch real mod");

        assert!(summary.unit_count > 0);
        assert!(output_path.is_file());
        assert_zip_has_entries(&output_path);
    }

    fn first_available_mapping(inspection: &WebEquipmentInspection) -> WebMigrationMapping {
        let source = inspection
            .sources
            .iter()
            .find(|source| source.resolved_hash.is_some())
            .expect("resolved equipment source");
        mapping_for_source(source)
    }

    fn available_mappings(inspection: &WebEquipmentInspection) -> Vec<WebMigrationMapping> {
        inspection
            .sources
            .iter()
            .filter(|source| source.resolved_hash.is_some())
            .map(mapping_for_source)
            .collect()
    }

    fn mapping_for_source(source: &web::WebDetectedSource) -> WebMigrationMapping {
        let source_hash = source.resolved_hash.clone().expect("source hash");
        let target = web::list_equipment_options()
            .expect("equipment options")
            .into_iter()
            .find(|target| {
                target.category == source.category && target.hash != source_hash && !target.excluded
            })
            .expect("different migration target");
        eprintln!(
            "real migration: {source_hash} -> {} ({})",
            target.hash, target.name
        );
        WebMigrationMapping {
            category: source.category,
            source_hash,
            target_hash: target.hash,
        }
    }

    fn output_path(filename: &str) -> PathBuf {
        let directory = required_path("HD2_DESKTOP_TEST_OUTPUT");
        std::fs::create_dir_all(&directory).expect("create real-data output directory");
        directory.join(filename)
    }

    fn assert_zip_has_entries(path: &Path) {
        let file = std::fs::File::open(path).expect("open output ZIP");
        let archive = zip::ZipArchive::new(file).expect("parse output ZIP");
        assert!(!archive.is_empty());
    }

    fn required_path(name: &str) -> PathBuf {
        std::env::var_os(name)
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("{name} must be set"))
    }
}
