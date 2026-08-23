use crate::archive::StreamToc;
use crate::constants::UNIT_ID;
use crate::index::{ArchiveIndex, ArmorEntry};
use crate::io::DataSource;
use crate::migrator::mode_a_web::{self, WebProgress};
use crate::migrator::safe_filename;
use crate::target_exclusions::is_default_excluded_target;
use crate::unit::authority::ArmorMappingTable;
use crate::unit::helmet_authority::HelmetMappingTable;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

#[cfg(test)]
mod tests;

pub const DEFAULT_PATCH_SUFFIX: &str = "9ba626afa44a3aa3.patch_0";

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum UnmatchedUnitPolicy {
    Drop,
    #[default]
    Keep,
}

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
    pub excluded: bool,
}

/// A logical armor or helmet object uniquely referenced by Units in a patch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WebDetectedModel {
    pub category: String,
    pub name: String,
    pub unit_hits: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebPatchInspection {
    pub source: Option<WebTargetOption>,
    pub models: Vec<WebDetectedModel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebMigrateOptions {
    pub source_hash: Option<String>,
    pub target_hashes: Vec<String>,
    pub patch_suffix: Option<String>,
    pub no_padding: bool,
    #[serde(default)]
    pub unmatched_unit_policy: UnmatchedUnitPolicy,
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
    pub unmatched_units: usize,
    pub unmatched_unit_policy: UnmatchedUnitPolicy,
    pub warnings: Vec<String>,
    #[serde(default)]
    pub mappings: Vec<super::equipment::WebMigrationMapping>,
}

pub fn list_target_options(category: &str) -> crate::Result<Vec<WebTargetOption>> {
    Ok(selectable_archive_entries(category)?
        .into_iter()
        .map(|entry| WebTargetOption {
            excluded: category == "Armor" && is_default_excluded_target(&entry.hash, &entry.name),
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

/// Reverse-look up patch Units across every authoritative model table.
///
/// A Unit shared by multiple logical models is deliberately ignored because
/// it cannot identify one model without guessing.
pub fn detect_patch_models(patch_bytes: &PatchBytes) -> crate::Result<Vec<WebDetectedModel>> {
    let patch_unit_ids = unit_file_ids_from_toc(&patch_bytes.toc)?;
    detect_models_via_authority(&patch_unit_ids)
}

/// Detects the selected source and every uniquely identifiable model with one TOC scan.
pub fn inspect_patch(
    category: &str,
    patch_bytes: &PatchBytes,
) -> crate::Result<WebPatchInspection> {
    let patch_unit_ids = unit_file_ids_from_toc(&patch_bytes.toc)?;
    Ok(WebPatchInspection {
        source: detect_source_via_authority(category, &patch_unit_ids),
        models: detect_models_via_authority(&patch_unit_ids)?,
    })
}

/// Full cross-archive migration via an async [`DataSource`].
pub async fn migrate_many_with_source<S: DataSource + ?Sized>(
    category: &str,
    patch_bytes: PatchBytes,
    options: WebMigrateOptions,
    source: &S,
    progress: Option<&dyn WebProgress>,
) -> crate::Result<WebMigrationBundle> {
    validate_targets(&options)?;
    let patch_suffix = options
        .patch_suffix
        .clone()
        .unwrap_or_else(|| DEFAULT_PATCH_SUFFIX.to_string());
    let results = mode_a_web::run(&patch_bytes, &options, source, category, progress).await?;

    let mut files = Vec::new();
    let mut reports = Vec::new();
    for result in results {
        let report_row = WebMigrationReportRow {
            target_hash: result.target_hash.clone(),
            target_name: result.target_name.clone(),
            file_id_remapped: result.report.file_id_remapped,
            slot_id_remapped: result.report.slot_id_remapped,
            padded_units: result.report.padded_units,
            skipped_entries: result.report.skipped_entries,
            unmatched_units: 0,
            unmatched_unit_policy: options.unmatched_unit_policy,
            warnings: result.report.warnings.clone(),
            mappings: vec![super::equipment::WebMigrationMapping {
                category: match category {
                    "Helmet" => super::equipment::EquipmentCategory::Helmet,
                    _ => super::equipment::EquipmentCategory::Armor,
                },
                source_hash: options.source_hash.clone().unwrap_or_default(),
                target_hash: result.target_hash.clone(),
            }],
        };
        files.extend(output_files(
            result.patch,
            &result.target_name,
            &patch_suffix,
        ));
        reports.push(report_row);
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

/// Return only actionable logical helmet options while preserving all Armor archives.
pub(crate) fn selectable_archive_entries(
    category: &str,
) -> crate::Result<Vec<&'static ArmorEntry>> {
    let entries = ArchiveIndex::builtin()
        .category(category)
        .ok_or_else(|| eyre::eyre!("category {:?} not found in builtin index", category))?;
    if category != "Helmet" {
        return Ok(entries.iter().collect());
    }

    let mapping = HelmetMappingTable::bundled()?;
    let mut seen_names = HashSet::new();
    Ok(entries
        .iter()
        .filter(|entry| {
            mapping.unit_id(&entry.name).is_some() && seen_names.insert(entry.name.as_str())
        })
        .collect())
}

pub(crate) fn detect_source_via_authority(
    category: &str,
    patch_unit_ids: &HashSet<u64>,
) -> Option<WebTargetOption> {
    if patch_unit_ids.is_empty() {
        return None;
    }
    match category {
        "Armor" => detect_armor_source(patch_unit_ids),
        "Helmet" => detect_helmet_source(patch_unit_ids),
        _ => None,
    }
}

pub(crate) fn detect_models_via_authority(
    patch_unit_ids: &HashSet<u64>,
) -> crate::Result<Vec<WebDetectedModel>> {
    let armor = ArmorMappingTable::bundled()?;
    let helmet = HelmetMappingTable::bundled()?;
    let mut owners_by_unit = HashMap::<u64, HashSet<ModelKey>>::new();
    for (name, parts) in armor.entries() {
        add_model_units(&mut owners_by_unit, "Armor", name, parts.all_file_ids());
    }
    for (name, unit_id) in helmet.entries() {
        add_model_units(&mut owners_by_unit, "Helmet", name, [unit_id]);
    }
    Ok(unique_model_hits(&owners_by_unit, patch_unit_ids))
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ModelKey {
    category: String,
    name: String,
}

fn add_model_units(
    owners_by_unit: &mut HashMap<u64, HashSet<ModelKey>>,
    category: &str,
    name: &str,
    unit_ids: impl IntoIterator<Item = u64>,
) {
    let model = ModelKey {
        category: category.to_string(),
        name: name.to_string(),
    };
    for unit_id in unit_ids {
        owners_by_unit
            .entry(unit_id)
            .or_default()
            .insert(model.clone());
    }
}

fn unique_model_hits(
    owners_by_unit: &HashMap<u64, HashSet<ModelKey>>,
    patch_unit_ids: &HashSet<u64>,
) -> Vec<WebDetectedModel> {
    let mut hit_counts = HashMap::<ModelKey, usize>::new();
    for unit_id in patch_unit_ids {
        let Some(owners) = owners_by_unit.get(unit_id) else {
            continue;
        };
        if owners.len() != 1 {
            continue;
        }
        let owner = owners.iter().next().expect("one owner after length check");
        *hit_counts.entry(owner.clone()).or_default() += 1;
    }
    let mut models = hit_counts
        .into_iter()
        .map(|(model, unit_hits)| WebDetectedModel {
            category: model.category,
            name: model.name,
            unit_hits,
        })
        .collect::<Vec<_>>();
    models.sort_by(|left, right| {
        right
            .unit_hits
            .cmp(&left.unit_hits)
            .then_with(|| left.category.cmp(&right.category))
            .then_with(|| left.name.cmp(&right.name))
    });
    models
}

fn detect_armor_source(patch_unit_ids: &HashSet<u64>) -> Option<WebTargetOption> {
    let table = ArmorMappingTable::bundled().ok()?;
    let index = ArchiveIndex::builtin();
    let mut candidates: Vec<SourceCandidate> = selectable_archive_entries("Armor")
        .ok()?
        .into_iter()
        .filter(|entry| is_preferred_entry(index, "Armor", entry))
        .filter_map(|entry| {
            let parts = table.armor(&entry.name)?;
            let unit_hits = parts
                .all_file_ids()
                .into_iter()
                .filter(|id| patch_unit_ids.contains(id))
                .count();
            (unit_hits > 0).then_some(SourceCandidate {
                option: WebTargetOption {
                    excluded: is_default_excluded_target(&entry.hash, &entry.name),
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

fn is_preferred_entry(index: &ArchiveIndex, category: &str, entry: &ArmorEntry) -> bool {
    index
        .preferred_hash(category, &entry.name)
        .is_none_or(|hash| entry.hash.eq_ignore_ascii_case(hash))
}

fn detect_helmet_source(patch_unit_ids: &HashSet<u64>) -> Option<WebTargetOption> {
    let table = HelmetMappingTable::bundled().ok()?;
    let mut candidates: Vec<SourceCandidate> = selectable_archive_entries("Helmet")
        .ok()?
        .into_iter()
        .filter_map(|entry| {
            let unit_id = table.unit_id(&entry.name)?;
            patch_unit_ids
                .contains(&unit_id)
                .then_some(SourceCandidate {
                    option: WebTargetOption {
                        excluded: false,
                        hash: entry.hash.clone(),
                        name: entry.name.clone(),
                    },
                    unit_hits: 1,
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

pub(crate) fn unit_file_ids(archive: &StreamToc) -> HashSet<u64> {
    archive
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .map(|entry| entry.file_id)
        .collect()
}
