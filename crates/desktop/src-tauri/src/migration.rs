mod output;
mod patch;

use self::output::{create_zip, finish_zip, write_patch_to_zip, write_zip_entry};
use self::patch::{LoadedPatch, PatchDescriptor, load_patch};
use crate::command_error::CommandError;
use crate::task::TaskRegistry;
use hd2_migrator_io::io::NativeDataSource;
use hd2_migrator_io::web::{
    self, UnitRepatchOptions, VariantPatchCallbacks, VariantPatchOutput, WebEquipmentInspection,
    WebEquipmentOption, WebMigrationSummary, WebProgress, WebUnifiedMigrateOptions,
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
    progress: Option<&dyn WebProgress>,
) -> Result<WebMigrationSummary, String> {
    validate_output_request(&request.data_dir, &request.output_path)?;
    let patch = load_patch(&request.patch_paths)?;
    let source = NativeDataSource::new(request.data_dir);
    let mut zip = create_zip(&request.output_path)?;
    let callbacks = VariantPatchCallbacks::new(progress, |mut output: VariantPatchOutput| {
        write_patch_to_zip(
            &mut zip,
            &mut output.patch,
            &output.directory,
            &output.suffix,
        )
    });
    let summary = web::migrate_variants_to_patch_sink(
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
    progress: Option<&dyn WebProgress>,
) -> Result<web::UnitRepatchSummary, String> {
    validate_output_request(&request.data_dir, &request.output_path)?;
    let patch = load_patch(&request.patch_paths)?;
    let result = web::repatch_units_with_progress(
        patch.name(),
        &patch.bytes().toc,
        request.options,
        &NativeDataSource::new(request.data_dir),
        progress,
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
            target_name: name.to_owned(),
            target_hash: hash.to_owned(),
            stage: stage.to_owned(),
            kind,
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
