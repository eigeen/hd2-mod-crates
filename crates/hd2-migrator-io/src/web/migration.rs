use crate::archive::StreamToc;
use crate::constants::UNIT_ID;
use crate::index::ArchiveIndex;
use crate::migrator::safe_filename;
use crate::unit::authority::ArmorMappingTable;
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
pub struct WebTargetOption {
    pub hash: String,
    pub name: String,
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

pub fn list_target_options(category: &str) -> crate::Result<Vec<WebTargetOption>> {
    let entries = ArchiveIndex::builtin()
        .category(category)
        .ok_or_else(|| eyre::eyre!("category {:?} not found in builtin index", category))?;
    Ok(entries
        .iter()
        .map(|entry| WebTargetOption {
            hash: entry.hash.clone(),
            name: entry.name.clone(),
        })
        .collect())
}

pub fn detect_source_archive(
    category: &str,
    patch_bytes: &PatchBytes,
) -> crate::Result<Option<WebTargetOption>> {
    let patch_unit_ids = unit_file_ids_from_toc(&patch_bytes.toc)?;
    Ok(detect_source_via_authority(category, &patch_unit_ids))
}

pub fn migrate_one(
    category: &str,
    patch_bytes: PatchBytes,
    options: WebMigrateOptions,
) -> crate::Result<WebMigrationBundle> {
    if options.target_hashes.len() != 1 {
        eyre::bail!("migrate_one requires exactly one target");
    }
    migrate_many(category, patch_bytes, options)
}

pub fn migrate_many(
    category: &str,
    patch_bytes: PatchBytes,
    options: WebMigrateOptions,
) -> crate::Result<WebMigrationBundle> {
    validate_targets(&options)?;
    let by_hash = archive_name_lookup(category)?;
    let patch = patch_from_bytes(&patch_bytes)?;
    let source_hash = resolve_source_hash(category, &by_hash, &patch, options.source_hash.as_deref())?;
    let patch_suffix = options
        .patch_suffix
        .as_deref()
        .unwrap_or(DEFAULT_PATCH_SUFFIX);
    let mut files = Vec::new();
    let mut reports = Vec::new();
    for target_hash in &options.target_hashes {
        let target_name = by_hash
            .iter()
            .find(|(hash, _)| hash == target_hash)
            .map(|(_, name)| name.clone())
            .ok_or_else(|| eyre::eyre!("target {target_hash} not found in builtin index"))?;
        let build = build_target(&patch, &source_hash, target_hash, &target_name)?;
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

fn archive_name_lookup(category: &str) -> crate::Result<Vec<(String, String)>> {
    let entries = ArchiveIndex::builtin()
        .category(category)
        .ok_or_else(|| eyre::eyre!("category {:?} not found in builtin index", category))?;
    Ok(entries
        .iter()
        .map(|entry| (entry.hash.clone(), entry.name.clone()))
        .collect())
}

fn patch_from_bytes(bytes: &PatchBytes) -> crate::Result<StreamToc> {
    StreamToc::from_buffers(&bytes.toc, &bytes.gpu, &bytes.stream, bytes.name.clone())
}

fn resolve_source_hash(
    category: &str,
    by_hash: &[(String, String)],
    patch: &StreamToc,
    source_hash: Option<&str>,
) -> crate::Result<String> {
    if let Some(hash) = source_hash {
        ensure_archive(by_hash, hash)?;
        return Ok(hash.to_string());
    }
    let patch_unit_ids = unit_file_ids(patch);
    detect_source_via_authority(category, &patch_unit_ids)
        .map(|option| option.hash)
        .ok_or_else(|| eyre::eyre!("could not auto-detect source archive from authoritative mapping"))
}

fn ensure_archive(by_hash: &[(String, String)], hash: &str) -> crate::Result<()> {
    if by_hash.iter().any(|(h, _)| h == hash) {
        return Ok(());
    }
    eyre::bail!("archive {hash} not found in builtin index")
}

fn detect_source_via_authority(
    category: &str,
    patch_unit_ids: &HashSet<u64>,
) -> Option<WebTargetOption> {
    if patch_unit_ids.is_empty() {
        return None;
    }
    let table = ArmorMappingTable::bundled().ok()?;
    let by_hash = ArchiveIndex::builtin().category(category)?;
    let mut candidates: Vec<SourceCandidate> = by_hash
        .iter()
        .filter_map(|entry| {
            let parts = table.armor(&entry.name)?;
            let unit_hits = parts
                .all_file_ids()
                .into_iter()
                .filter(|id| patch_unit_ids.contains(id))
                .count();
            (unit_hits > 0).then_some(SourceCandidate {
                option: WebTargetOption {
                    hash: entry.hash.clone(),
                    name: entry.name.clone(),
                },
                unit_hits,
            })
        })
        .collect();
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

fn compare_source_candidates(left: &SourceCandidate, right: &SourceCandidate) -> Ordering {
    left.unit_hits
        .cmp(&right.unit_hits)
        .then_with(|| right.option.hash.cmp(&left.option.hash))
}

fn build_target(
    patch: &StreamToc,
    source_hash: &str,
    target_hash: &str,
    target_name: &str,
) -> crate::Result<TargetBuild> {
    if target_hash == source_hash {
        return Ok(build_source_target(patch, target_hash, target_name));
    }
    build_migrated_target(target_hash, target_name)
}

fn build_source_target(
    patch: &StreamToc,
    target_hash: &str,
    target_name: &str,
) -> TargetBuild {
    TargetBuild {
        patch: patch.clone(),
        report: WebMigrationReportRow {
            target_hash: target_hash.to_string(),
            target_name: target_name.to_string(),
            file_id_remapped: patch.entries.len(),
            slot_id_remapped: 0,
            padded_units: 0,
            skipped_entries: 0,
            warnings: Vec::new(),
        },
    }
}

fn build_migrated_target(target_hash: &str, target_name: &str) -> crate::Result<TargetBuild> {
    eyre::bail!(
        "cross-archive migration is not available in the browser: pick the source archive as target, \
         or run the CLI/desktop migrator (target {target_name} / {target_hash})"
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
