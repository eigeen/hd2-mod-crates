use crate::archive::StreamToc;
use crate::constants::UNIT_ID;
use crate::migrator::safe_filename;
use crate::web::metadata::{WebArchiveMetadata, WebGameMetadata, WebTargetOption};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashSet;

#[cfg(test)]
mod tests;

pub const DEFAULT_PATCH_SUFFIX: &str = "9ba626afa44a3aa3.patch_0";

#[derive(Debug, Clone)]
pub struct PatchBytes {
    pub name: String,
    pub toc: Vec<u8>,
    pub gpu: Vec<u8>,
    pub stream: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebMigrateOptions {
    pub source_hash: Option<String>,
    pub target_hashes: Vec<String>,
    pub patch_suffix: Option<String>,
    pub no_padding: bool,
    pub experimental_partial_remap: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebMigrationBundle {
    pub files: Vec<WebOutputFile>,
    pub summary: WebMigrationSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebOutputFile {
    pub path: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebMigrationSummary {
    pub migrated_count: usize,
    pub warning_count: usize,
    pub reports: Vec<WebMigrationReportRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebMigrationReportRow {
    pub target_hash: String,
    pub target_name: String,
    pub file_id_remapped: usize,
    pub slot_id_remapped: usize,
    pub padded_units: usize,
    pub skipped_entries: usize,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
struct TargetBuild {
    patch: StreamToc,
    report: WebMigrationReportRow,
}

pub fn list_target_options(metadata: &WebGameMetadata) -> Vec<WebTargetOption> {
    metadata.target_options()
}

pub fn detect_source_archive(
    metadata: &WebGameMetadata,
    patch_bytes: &PatchBytes,
) -> crate::Result<Option<WebTargetOption>> {
    let patch_unit_ids = unit_file_ids_from_toc(&patch_bytes.toc)?;
    Ok(detect_source_from_unit_ids(metadata, &patch_unit_ids))
}

pub fn migrate_one(
    metadata: &WebGameMetadata,
    patch_bytes: PatchBytes,
    options: WebMigrateOptions,
) -> crate::Result<WebMigrationBundle> {
    if options.target_hashes.len() != 1 {
        eyre::bail!("migrate_one requires exactly one target");
    }
    migrate_many(metadata, patch_bytes, options)
}

pub fn migrate_many(
    metadata: &WebGameMetadata,
    patch_bytes: PatchBytes,
    options: WebMigrateOptions,
) -> crate::Result<WebMigrationBundle> {
    validate_targets(&options)?;
    let patch = patch_from_bytes(&patch_bytes)?;
    let source_hash = resolve_source_hash(metadata, &patch, options.source_hash.as_deref())?;
    let patch_suffix = options
        .patch_suffix
        .as_deref()
        .unwrap_or(DEFAULT_PATCH_SUFFIX);
    let mut files = Vec::new();
    let mut reports = Vec::new();
    for target_hash in &options.target_hashes {
        let build = build_target(BuildTargetOptions {
            metadata,
            patch: &patch,
            source_hash: &source_hash,
            target_hash,
        })?;
        files.extend(output_files(
            build.patch,
            &build.report.target_name,
            patch_suffix,
        ));
        reports.push(build.report);
    }
    Ok(WebMigrationBundle {
        files,
        summary: summary_from_reports(reports),
    })
}

fn validate_targets(options: &WebMigrateOptions) -> crate::Result<()> {
    if options.target_hashes.is_empty() {
        eyre::bail!("select at least one target");
    }
    Ok(())
}

fn patch_from_bytes(bytes: &PatchBytes) -> crate::Result<StreamToc> {
    StreamToc::from_buffers(&bytes.toc, &bytes.gpu, &bytes.stream, bytes.name.clone())
}

fn resolve_source_hash(
    metadata: &WebGameMetadata,
    patch: &StreamToc,
    source_hash: Option<&str>,
) -> crate::Result<String> {
    if let Some(hash) = source_hash {
        ensure_archive(metadata, hash)?;
        return Ok(hash.to_string());
    }
    detect_source_from_patch(metadata, patch)
        .map(|option| option.hash)
        .ok_or_else(|| eyre::eyre!("could not auto-detect source archive"))
}

fn ensure_archive(metadata: &WebGameMetadata, hash: &str) -> crate::Result<()> {
    if metadata.archive(hash).is_some() {
        return Ok(());
    }
    eyre::bail!("archive {hash} not found in web metadata")
}

fn detect_source_from_patch(
    metadata: &WebGameMetadata,
    patch: &StreamToc,
) -> Option<WebTargetOption> {
    let patch_unit_ids = unit_file_ids(patch);
    detect_source_from_unit_ids(metadata, &patch_unit_ids)
}

fn detect_source_from_unit_ids(
    metadata: &WebGameMetadata,
    patch_unit_ids: &HashSet<u64>,
) -> Option<WebTargetOption> {
    let mut candidates = metadata
        .targets
        .iter()
        .filter_map(|target| source_candidate(target, &patch_unit_ids))
        .collect::<Vec<_>>();
    candidates.sort_by(compare_source_candidates);
    candidates.pop().map(|candidate| candidate.option)
}

fn unit_file_ids_from_toc(toc: &[u8]) -> crate::Result<HashSet<u64>> {
    Ok(crate::archive::list_file_ids_from_bytes(toc)?
        .remove(&UNIT_ID)
        .unwrap_or_default()
        .into_iter()
        .collect())
}

struct SourceCandidate {
    option: WebTargetOption,
    unit_hits: usize,
}

fn source_candidate(
    target: &WebArchiveMetadata,
    patch_unit_ids: &HashSet<u64>,
) -> Option<SourceCandidate> {
    let archive_unit_ids = target
        .archive
        .unit_file_ids()
        .into_iter()
        .collect::<HashSet<_>>();
    let unit_hits = patch_unit_ids.intersection(&archive_unit_ids).count();
    (unit_hits > 0).then(|| SourceCandidate {
        option: WebTargetOption {
            hash: target.hash.clone(),
            name: target.name.clone(),
        },
        unit_hits,
    })
}

fn compare_source_candidates(left: &SourceCandidate, right: &SourceCandidate) -> Ordering {
    left.unit_hits
        .cmp(&right.unit_hits)
        .then_with(|| right.option.hash.cmp(&left.option.hash))
}

struct BuildTargetOptions<'a> {
    metadata: &'a WebGameMetadata,
    patch: &'a StreamToc,
    source_hash: &'a str,
    target_hash: &'a str,
}

fn build_target(options: BuildTargetOptions<'_>) -> crate::Result<TargetBuild> {
    let target_meta = options
        .metadata
        .archive(options.target_hash)
        .ok_or_else(|| eyre::eyre!("target {} not found in metadata", options.target_hash))?;
    if options.target_hash == options.source_hash {
        return build_source_target(options.patch, target_meta);
    }
    build_migrated_target(options, target_meta)
}

fn build_source_target(
    patch: &StreamToc,
    target_meta: &WebArchiveMetadata,
) -> crate::Result<TargetBuild> {
    Ok(TargetBuild {
        patch: patch.clone(),
        report: WebMigrationReportRow {
            target_hash: target_meta.hash.clone(),
            target_name: target_meta.name.clone(),
            file_id_remapped: patch.entries.len(),
            slot_id_remapped: 0,
            padded_units: 0,
            skipped_entries: 0,
            warnings: Vec::new(),
        },
    })
}

fn build_migrated_target(
    _options: BuildTargetOptions<'_>,
    target_meta: &WebArchiveMetadata,
) -> crate::Result<TargetBuild> {
    eyre::bail!(
        "web metadata for target {} ({}) is an index only and cannot be used as target archive data",
        target_meta.name,
        target_meta.hash
    )
}

fn output_files(mut patch: StreamToc, target_name: &str, patch_suffix: &str) -> Vec<WebOutputFile> {
    let (toc, gpu, stream) = patch.serialize();
    let directory = safe_filename(target_name);
    vec![
        output_file(&directory, patch_suffix, toc),
        output_file(&directory, &format!("{patch_suffix}.gpu_resources"), gpu),
        output_file(&directory, &format!("{patch_suffix}.stream"), stream),
    ]
}

fn output_file(directory: &str, filename: &str, bytes: Vec<u8>) -> WebOutputFile {
    WebOutputFile {
        path: format!("{directory}/{filename}"),
        bytes,
    }
}

fn summary_from_reports(reports: Vec<WebMigrationReportRow>) -> WebMigrationSummary {
    WebMigrationSummary {
        migrated_count: reports.len(),
        warning_count: reports.iter().map(|report| report.warnings.len()).sum(),
        reports,
    }
}

fn unit_file_ids(archive: &StreamToc) -> HashSet<u64> {
    archive
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .map(|entry| entry.file_id)
        .collect()
}
