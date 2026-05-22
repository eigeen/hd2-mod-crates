use hd2_migrator_io::{
    ArchiveIndex, EmptyUnitTemplate, MigrateAllOpts, MigrationReport, PaddingMode, ProgressSink,
    builtin_template, migrate_all,
};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRequest {
    patch_path: PathBuf,
    data_dir: PathBuf,
    out_dir: PathBuf,
    target_filter: String,
    no_padding: bool,
    experimental_partial_remap: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationTargetOption {
    hash: String,
    name: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationSummary {
    migrated_count: usize,
    warning_count: usize,
    reports: Vec<MigrationReportRow>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationReportRow {
    target_name: String,
    file_id_remapped: usize,
    slot_id_remapped: usize,
    padded_units: usize,
    warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
struct MigrationProgressEvent {
    status: String,
}

/// Return all armor migration targets for the selection list.
#[tauri::command]
pub fn load_migration_targets() -> Result<Vec<MigrationTargetOption>, String> {
    let targets = ArchiveIndex::builtin()
        .category("Armor")
        .ok_or_else(|| "Armor category not found in archive index".to_owned())?;
    Ok(targets.iter().map(target_option_from_entry).collect())
}

/// Run the migration away from the webview thread and return a compact summary.
#[tauri::command]
pub async fn run_migration(
    request: MigrationRequest,
    app: AppHandle,
) -> Result<MigrationSummary, String> {
    validate_request(&request)?;
    tauri::async_runtime::spawn_blocking(move || execute_migration(request, app))
        .await
        .map_err(|error| format!("Migration task failed: {error}"))?
}

fn validate_request(request: &MigrationRequest) -> Result<(), String> {
    validate_path(&request.patch_path, "Patch path")?;
    validate_path(&request.data_dir, "Game data directory")?;
    validate_path(&request.out_dir, "Output directory")?;
    validate_target_filter(&request.target_filter)
}

fn validate_path(path: &Path, label: &str) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        Err(format!("{label} is required"))
    } else {
        Ok(())
    }
}

fn validate_target_filter(value: &str) -> Result<(), String> {
    if parse_target_filter(value).is_some() {
        return Ok(());
    }
    Err("Select at least one target".to_owned())
}

fn target_option_from_entry(
    entry: &hd2_migrator_io::index::ArmorEntry,
) -> MigrationTargetOption {
    MigrationTargetOption {
        hash: entry.hash.clone(),
        name: entry.name.clone(),
    }
}

fn execute_migration(
    request: MigrationRequest,
    app: AppHandle,
) -> Result<MigrationSummary, String> {
    emit_status(&app, "Preparing migration");
    let reports = run_core_migration(&request, &app).map_err(|error| format!("{error:?}"))?;
    emit_status(&app, "Migration complete");
    Ok(summary_from_reports(reports))
}

fn run_core_migration(
    request: &MigrationRequest,
    app: &AppHandle,
) -> hd2_migrator_io::Result<Vec<MigrationReport>> {
    let targets = parse_target_filter(&request.target_filter);
    let template = padding_template(request.no_padding);
    let progress = TauriProgress::new(app.clone());
    migrate_all(migration_opts(
        request,
        targets.as_deref(),
        template.as_ref(),
        &progress,
    ))
}

fn migration_opts<'a>(
    request: &'a MigrationRequest,
    targets: Option<&'a [String]>,
    template: Option<&'a EmptyUnitTemplate>,
    progress: &'a dyn ProgressSink,
) -> MigrateAllOpts<'a> {
    MigrateAllOpts {
        patch_path: &request.patch_path,
        data_dir: &request.data_dir,
        out_dir: &request.out_dir,
        archive_index: ArchiveIndex::builtin(),
        source_hash: None,
        target_hashes: targets,
        category: "Armor",
        patch_suffix: "9ba626afa44a3aa3.patch_0",
        empty_unit_template: template,
        padding_mode: padding_mode(request.no_padding),
        armor_mapping_json: None,
        experimental_partial_remap: request.experimental_partial_remap,
        progress: Some(progress),
    }
}

fn parse_target_filter(value: &str) -> Option<Vec<String>> {
    let targets: Vec<String> = value
        .split(',')
        .map(str::trim)
        .filter(|target| !target.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    (!targets.is_empty()).then_some(targets)
}

fn padding_template(no_padding: bool) -> Option<EmptyUnitTemplate> {
    (!no_padding).then(builtin_template)
}

fn padding_mode(no_padding: bool) -> PaddingMode {
    if no_padding {
        PaddingMode::Disabled
    } else {
        PaddingMode::Sanitized
    }
}

fn summary_from_reports(reports: Vec<MigrationReport>) -> MigrationSummary {
    let warning_count = reports.iter().map(|report| report.warnings.len()).sum();
    let rows: Vec<MigrationReportRow> = reports.into_iter().map(row_from_report).collect();
    MigrationSummary {
        migrated_count: rows_len(&rows),
        warning_count,
        reports: rows,
    }
}

fn rows_len(rows: &[MigrationReportRow]) -> usize {
    rows.len()
}

fn row_from_report(report: MigrationReport) -> MigrationReportRow {
    MigrationReportRow {
        target_name: report.target_name,
        file_id_remapped: report.file_id_remapped,
        slot_id_remapped: report.slot_id_remapped,
        padded_units: report.padded_units,
        warnings: report.warnings,
    }
}

struct TauriProgress {
    app: AppHandle,
}

impl TauriProgress {
    fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl ProgressSink for TauriProgress {
    fn target_started(&self, name: &str) {
        emit_status(&self.app, &format!("Migrating {name}"));
    }

    fn stage(&self, name: &str, stage: &str) {
        emit_status(&self.app, &format!("{name}: {stage}"));
    }

    fn target_finished(&self, name: &str) {
        emit_status(&self.app, &format!("Finished {name}"));
    }
}

fn emit_status(app: &AppHandle, status: &str) {
    let event = MigrationProgressEvent {
        status: status.to_string(),
    };
    let _ = app.emit("migration://progress", event);
}
