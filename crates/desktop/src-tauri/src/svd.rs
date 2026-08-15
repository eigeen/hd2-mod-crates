use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use svd_core::export::{ExportMode, ExportOptions, export, load_export_package_summary};
use svd_core::pack::{PackOptions, pack};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SvdPackRequest {
    input_dir: PathBuf,
    base_variant: String,
    output_dir: PathBuf,
    package_path: Option<PathBuf>,
    compression_level: i32,
    jobs: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SvdPackSummary {
    output_dir: String,
    package_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SvdExportRequest {
    package_path: PathBuf,
    output_zip: PathBuf,
    all_variants: bool,
    variants: Vec<String>,
    jobs: Option<usize>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SvdExportSummary {
    output_zip: String,
    variant_count: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SvdPackageSummary {
    mod_name: Option<String>,
    base_variant: String,
    variants: Vec<String>,
}

/// Build an SVD package without blocking the webview thread.
#[tauri::command]
pub async fn run_svd_pack(request: SvdPackRequest) -> Result<SvdPackSummary, String> {
    validate_pack_request(&request)?;
    tauri::async_runtime::spawn_blocking(move || execute_pack(request))
        .await
        .map_err(|error| format!("SVD pack task failed: {error}"))?
}

/// Read package metadata for variant selection in the UI.
#[tauri::command]
pub async fn load_svd_package_summary(package_path: PathBuf) -> Result<SvdPackageSummary, String> {
    validate_path(&package_path, "SVD package")?;
    tauri::async_runtime::spawn_blocking(move || execute_summary(package_path))
        .await
        .map_err(|error| format!("SVD summary task failed: {error}"))?
}

/// Export selected SVD variants to a mod-manager zip.
#[tauri::command]
pub async fn run_svd_export(request: SvdExportRequest) -> Result<SvdExportSummary, String> {
    validate_export_request(&request)?;
    tauri::async_runtime::spawn_blocking(move || execute_export(request))
        .await
        .map_err(|error| format!("SVD export task failed: {error}"))?
}

fn execute_pack(request: SvdPackRequest) -> Result<SvdPackSummary, String> {
    let summary = pack_summary(&request);
    let options = PackOptions {
        input: request.input_dir,
        base: request.base_variant.trim().to_owned(),
        output: request.output_dir.clone(),
        zip_output: request.package_path.clone(),
        level: request.compression_level,
        jobs: request.jobs,
    };
    pack(&options).map_err(display_error)?;
    Ok(summary)
}

fn execute_summary(package_path: PathBuf) -> Result<SvdPackageSummary, String> {
    let summary = load_export_package_summary(&package_path).map_err(display_error)?;
    Ok(SvdPackageSummary {
        mod_name: summary.mod_info.name,
        base_variant: summary.base_variant,
        variants: summary.variants,
    })
}

fn execute_export(request: SvdExportRequest) -> Result<SvdExportSummary, String> {
    let mode = export_mode(&request);
    let selected_count = selected_variant_count(&request);
    let options = ExportOptions {
        package: request.package_path,
        output: Some(request.output_zip.clone()),
        mode,
        jobs: request.jobs,
    };
    export(&options).map_err(display_error)?;
    Ok(SvdExportSummary {
        output_zip: request.output_zip.display().to_string(),
        variant_count: selected_count,
    })
}

fn validate_pack_request(request: &SvdPackRequest) -> Result<(), String> {
    validate_path(&request.input_dir, "Source mod directory")?;
    validate_path(&request.output_dir, "Package output directory")?;
    validate_text(&request.base_variant, "Base variant")?;
    validate_compression_level(request.compression_level)?;
    validate_jobs(request.jobs)?;
    validate_optional_path(request.package_path.as_deref(), "SVD package")
}

fn validate_export_request(request: &SvdExportRequest) -> Result<(), String> {
    validate_path(&request.package_path, "SVD package")?;
    validate_path(&request.output_zip, "Export zip")?;
    validate_jobs(request.jobs)?;
    if request.all_variants || !request.variants.is_empty() {
        return Ok(());
    }
    Err("Select at least one variant to export".to_owned())
}

fn validate_path(path: &Path, label: &str) -> Result<(), String> {
    if path.as_os_str().is_empty() {
        return Err(format!("{label} is required"));
    }
    Ok(())
}

fn validate_optional_path(path: Option<&Path>, label: &str) -> Result<(), String> {
    if let Some(value) = path {
        validate_path(value, label)?;
    }
    Ok(())
}

fn validate_text(value: &str, label: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("{label} is required"));
    }
    Ok(())
}

fn validate_compression_level(level: i32) -> Result<(), String> {
    if (-7..=22).contains(&level) {
        return Ok(());
    }
    Err("Compression level must be between -7 and 22".to_owned())
}

fn validate_jobs(jobs: Option<usize>) -> Result<(), String> {
    if jobs == Some(0) {
        return Err("Jobs must be greater than 0".to_owned());
    }
    Ok(())
}

fn export_mode(request: &SvdExportRequest) -> ExportMode {
    if request.all_variants {
        return ExportMode::All;
    }
    ExportMode::Variants(request.variants.clone())
}

fn selected_variant_count(request: &SvdExportRequest) -> usize {
    if request.all_variants {
        return 0;
    }
    request.variants.len()
}

fn pack_summary(request: &SvdPackRequest) -> SvdPackSummary {
    SvdPackSummary {
        output_dir: request.output_dir.display().to_string(),
        package_path: request
            .package_path
            .as_ref()
            .map(|path| path.display().to_string()),
    }
}

fn display_error(error: svd_core::error::Error) -> String {
    error.to_string()
}
