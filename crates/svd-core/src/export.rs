use crate::archive::{create_package_zip, open_package_root};
use crate::error::{Result, message};
use crate::export_progress::ExportProgress;
use crate::fs_paths::{join_metadata_path, validate_variant_name};
use crate::hash::content_hash;
use crate::manifest::write_export_manifest;
use crate::metadata::{
    AssetRecord, FileOperation, FileRecord, METADATA_FILE, ModInfo, PackageMetadata,
    VariantMetadata,
};
use crate::patch::{apply_patch, decompress_full};
use rayon::ThreadPoolBuilder;
use rayon::prelude::*;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub package: PathBuf,
    pub output: Option<PathBuf>,
    pub mode: ExportMode,
    pub jobs: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum ExportMode {
    List,
    Variants(Vec<String>),
    All,
}

#[derive(Debug, Clone)]
pub struct ExportPackageSummary {
    pub mod_info: ModInfo,
    pub base_variant: String,
    pub variants: Vec<String>,
}

/// Reads display metadata used by interactive export frontends.
pub fn load_export_package_summary(package: &Path) -> Result<ExportPackageSummary> {
    let package_root = open_package_root(package)?;
    let metadata = read_metadata(package_root.path())?;
    Ok(ExportPackageSummary {
        mod_info: metadata.mod_info,
        base_variant: metadata.base_variant,
        variants: metadata
            .variants
            .into_iter()
            .map(|variant| variant.name)
            .collect(),
    })
}

/// Lists the variant names available in a directory or zip package.
pub fn list_available_variants(package: &Path) -> Result<Vec<String>> {
    let package_root = open_package_root(package)?;
    let metadata = read_metadata(package_root.path())?;
    Ok(metadata
        .variants
        .into_iter()
        .map(|variant| variant.name)
        .collect())
}

/// Restores selected variants from a package into a modpack-format zip.
pub fn export(options: &ExportOptions) -> Result<()> {
    if matches!(options.mode, ExportMode::List) {
        return print_available_variants(&options.package);
    }

    let zip_output = required_output(options)?;
    prepare_output_zip(zip_output)?;
    let package_root = open_package_root(&options.package)?;
    let metadata = read_metadata(package_root.path())?;

    let mut builder = ThreadPoolBuilder::new();
    if let Some(jobs) = options.jobs {
        builder = builder.num_threads(jobs);
    }
    let pool = builder.build()?;
    pool.install(|| export_with_pool(package_root.path(), zip_output, &metadata, &options.mode))
}

fn print_available_variants(package: &Path) -> Result<()> {
    for name in list_available_variants(package)? {
        println!("{name}");
    }
    Ok(())
}

fn required_output(options: &ExportOptions) -> Result<&Path> {
    options
        .output
        .as_deref()
        .ok_or_else(|| message("missing required flag --output"))
}

fn export_with_pool(
    package: &Path,
    zip_output: &Path,
    metadata: &PackageMetadata,
    mode: &ExportMode,
) -> Result<()> {
    let names = selected_variant_names(metadata, mode)?;
    if names.is_empty() {
        return Err(message("no variants selected"));
    }

    let progress = ExportProgress::new(export_work_units(metadata, &names));
    verify_base_records(package, metadata, &progress)?;

    let staging = TempDir::new()?;
    let staging_root = staging.path();
    copy_assets(package, staging_root, &metadata.assets, &progress)?;
    write_variant_subdirs(package, staging_root, metadata, &names, &progress)?;
    write_export_manifest(
        staging_root,
        &metadata.mod_info,
        &metadata.manifest_options,
        &names,
    )?;

    progress.set_message("packing zip");
    create_package_zip(staging_root, zip_output)?;
    progress.inc();
    progress.finish();
    Ok(())
}

fn export_work_units(metadata: &PackageMetadata, selected: &[String]) -> u64 {
    let base_units = metadata
        .variants
        .iter()
        .find(|variant| variant.name == metadata.base_variant)
        .map(|base| base.files.len())
        .unwrap_or(0);
    let asset_units = metadata.assets.len();
    let variant_units: usize = selected
        .iter()
        .filter_map(|name| metadata.variants.iter().find(|v| &v.name == name))
        .map(|variant| variant.files.len())
        .sum();
    (base_units + asset_units + variant_units + 1) as u64
}

fn selected_variant_names(metadata: &PackageMetadata, mode: &ExportMode) -> Result<Vec<String>> {
    match mode {
        ExportMode::List => Ok(Vec::new()),
        ExportMode::All => Ok(metadata.variants.iter().map(|v| v.name.clone()).collect()),
        ExportMode::Variants(names) => {
            let names = unique_names(names);
            for name in &names {
                variant_by_name(metadata, name)?;
            }
            Ok(names)
        }
    }
}

fn write_variant_subdirs(
    package: &Path,
    staging_root: &Path,
    metadata: &PackageMetadata,
    names: &[String],
    progress: &ExportProgress,
) -> Result<()> {
    progress.set_message("restoring variants");
    names.par_iter().try_for_each(|name| {
        validate_variant_name(name)?;
        let variant = variant_by_name(metadata, name)?;
        let out_dir = staging_root.join(name);
        std::fs::create_dir_all(&out_dir)?;
        export_variant(package, &out_dir, variant, progress)
    })
}

fn prepare_output_zip(zip_output: &Path) -> Result<()> {
    if zip_output.exists() {
        return Err(message(format!(
            "output file already exists: {}",
            zip_output.display()
        )));
    }
    if let Some(parent) = zip_output.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

fn read_metadata(package: &Path) -> Result<PackageMetadata> {
    let text = std::fs::read_to_string(package.join(METADATA_FILE))?;
    Ok(serde_json::from_str(&text)?)
}

fn verify_base_records(
    package: &Path,
    metadata: &PackageMetadata,
    progress: &ExportProgress,
) -> Result<()> {
    let base = variant_by_name(metadata, &metadata.base_variant)?;
    progress.set_message("verifying base");
    base.files.par_iter().try_for_each(|record| {
        let result = (|| -> Result<()> {
            if record.operation != FileOperation::Base {
                return Ok(());
            }
            let bytes = read_base_file(package, &record.relative_path)?;
            verify_hash(&bytes, record.target_hash.as_deref(), &record.relative_path)
        })();
        progress.inc();
        result
    })
}

fn unique_names(names: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    names
        .iter()
        .filter(|name| seen.insert((*name).clone()))
        .cloned()
        .collect()
}

fn variant_by_name<'a>(metadata: &'a PackageMetadata, name: &str) -> Result<&'a VariantMetadata> {
    metadata
        .variants
        .iter()
        .find(|variant| variant.name == name)
        .ok_or_else(|| message(format!("variant not found: {name}")))
}

fn copy_assets(
    package: &Path,
    output: &Path,
    assets: &[AssetRecord],
    progress: &ExportProgress,
) -> Result<()> {
    progress.set_message("copying assets");
    assets.par_iter().try_for_each(|asset| {
        let result = (|| -> Result<()> {
            let input = join_metadata_path(&package.join("assets"), &asset.relative_path)?;
            let bytes = std::fs::read(&input)?;
            verify_hash(&bytes, Some(&asset.hash), &asset.relative_path)?;
            write_output_file(output, &asset.relative_path, &bytes)
        })();
        progress.inc();
        result
    })
}

fn export_variant(
    package: &Path,
    output: &Path,
    variant: &VariantMetadata,
    progress: &ExportProgress,
) -> Result<()> {
    variant.files.par_iter().try_for_each(|record| {
        let result = (|| -> Result<()> {
            let Some(bytes) = restore_record(package, record)? else {
                return Ok(());
            };
            verify_hash(&bytes, record.target_hash.as_deref(), &record.relative_path)?;
            write_output_file(output, &record.relative_path, &bytes)
        })();
        progress.inc();
        result
    })
}

fn restore_record(package: &Path, record: &FileRecord) -> Result<Option<Vec<u8>>> {
    match record.operation {
        FileOperation::Base | FileOperation::SameAsBase => {
            let base = read_record_base_file(package, record)?;
            verify_hash(&base, record.base_hash.as_deref(), &record.relative_path)?;
            Ok(Some(base))
        }
        FileOperation::Patch => restore_patch(package, record).map(Some),
        FileOperation::Full => restore_full(package, record).map(Some),
        FileOperation::Delete => {
            verify_deleted_base(package, record)?;
            Ok(None)
        }
        FileOperation::Empty => {
            verify_empty_base_if_present(package, record)?;
            Ok(Some(Vec::new()))
        }
    }
}

fn restore_patch(package: &Path, record: &FileRecord) -> Result<Vec<u8>> {
    let base = read_record_base_file(package, record)?;
    verify_hash(&base, record.base_hash.as_deref(), &record.relative_path)?;
    let patch = read_stored_payload(package, record)?;
    let params = record
        .zstd
        .as_ref()
        .ok_or_else(|| message(format!("missing zstd params: {}", record.relative_path)))?;
    apply_patch(&base, &patch, params)
}

fn restore_full(package: &Path, record: &FileRecord) -> Result<Vec<u8>> {
    let payload = read_stored_payload(package, record)?;
    decompress_full(&payload)
}

fn verify_deleted_base(package: &Path, record: &FileRecord) -> Result<()> {
    let base = read_record_base_file(package, record)?;
    verify_hash(&base, record.base_hash.as_deref(), &record.relative_path)
}

fn verify_empty_base_if_present(package: &Path, record: &FileRecord) -> Result<()> {
    if record.base_hash.is_none() {
        return Ok(());
    }
    let base = read_record_base_file(package, record)?;
    verify_hash(&base, record.base_hash.as_deref(), &record.relative_path)
}

fn read_record_base_file(package: &Path, record: &FileRecord) -> Result<Vec<u8>> {
    let relative_path = record
        .base_relative_path
        .as_deref()
        .unwrap_or(&record.relative_path);
    read_base_file(package, relative_path)
}

fn read_base_file(package: &Path, relative_path: &str) -> Result<Vec<u8>> {
    let path = join_metadata_path(&package.join("base"), relative_path)?;
    Ok(std::fs::read(path)?)
}

fn read_stored_payload(package: &Path, record: &FileRecord) -> Result<Vec<u8>> {
    let stored_path = record
        .stored_path
        .as_ref()
        .ok_or_else(|| message(format!("missing stored path: {}", record.relative_path)))?;
    let path = join_metadata_path(package, stored_path)?;
    Ok(std::fs::read(path)?)
}

fn verify_hash(bytes: &[u8], expected: Option<&str>, label: &str) -> Result<()> {
    if expected.is_some_and(|value| value != content_hash(bytes)) {
        return Err(message(format!("hash mismatch: {label}")));
    }
    Ok(())
}

fn write_output_file(output: &Path, relative_path: &str, bytes: &[u8]) -> Result<()> {
    let path = join_metadata_path(output, relative_path)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, bytes)?;
    Ok(())
}
