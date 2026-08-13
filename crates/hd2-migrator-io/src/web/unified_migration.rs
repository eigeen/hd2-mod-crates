use crate::archive::{StreamToc, TocEntry};
use crate::constants::UNIT_ID;
use crate::io::DataSource;
use crate::migrator::{mode_a_web, source_selection};
use crate::unit::authority::ArmorMappingTable;
use crate::unit::helmet_authority::HelmetMappingTable;
use crate::web::equipment::{EquipmentCategory, WebMigrationMapping};
use crate::web::migration::{
    PatchBytes, UnmatchedUnitPolicy, WebMigrateOptions, WebMigrationBundle, WebMigrationReportRow,
    WebMigrationSummary, WebOutputFile,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebMigrationVariant {
    pub mappings: Vec<WebMigrationMapping>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebUnifiedMigrateOptions {
    pub variants: Vec<WebMigrationVariant>,
    pub patch_suffix: Option<String>,
    pub no_padding: bool,
    #[serde(default)]
    pub unmatched_unit_policy: UnmatchedUnitPolicy,
}

/// Apply every mapping in a variant to one patch, preserving mapped results between steps.
pub async fn migrate_variants_with_source<S: DataSource + ?Sized>(
    patch_bytes: PatchBytes,
    options: WebUnifiedMigrateOptions,
    source: &S,
    progress: Option<&dyn mode_a_web::WebProgress>,
) -> crate::Result<WebMigrationBundle> {
    validate_variants(&options.variants)?;
    let suffix = options
        .patch_suffix
        .as_deref()
        .unwrap_or(super::migration::DEFAULT_PATCH_SUFFIX);
    let mut files = Vec::new();
    let mut reports = Vec::new();
    for (variant_index, variant) in options.variants.iter().enumerate() {
        let result = migrate_variant(&patch_bytes, variant, &options, source, progress).await?;
        let directory = variant_directory(
            variant,
            &result.report.target_name,
            variant_index,
            options.variants.len(),
        );
        files.extend(output_files(result.patch, &directory, suffix));
        reports.push(result.report);
    }
    Ok(WebMigrationBundle {
        files,
        summary: WebMigrationSummary {
            migrated_count: reports.len(),
            warning_count: reports.iter().map(|report| report.warnings.len()).sum(),
            reports,
        },
    })
}

struct VariantResult {
    patch: StreamToc,
    report: WebMigrationReportRow,
}

async fn migrate_variant<S: DataSource + ?Sized>(
    original: &PatchBytes,
    variant: &WebMigrationVariant,
    options: &WebUnifiedMigrateOptions,
    source: &S,
    progress: Option<&dyn mode_a_web::WebProgress>,
) -> crate::Result<VariantResult> {
    let mut patch = StreamToc::from_buffers(
        &original.toc,
        &original.gpu,
        &original.stream,
        original.name.clone(),
    )?;
    let mut writes = HashMap::<(u64, u64), TocEntry>::new();
    let mut report = empty_report(variant);
    let mut allowed_units = HashSet::new();

    for mapping in &variant.mappings {
        let before = patch.clone();
        let request = WebMigrateOptions {
            source_hash: Some(mapping.source_hash.clone()),
            target_hashes: vec![mapping.target_hash.clone()],
            patch_suffix: options.patch_suffix.clone(),
            no_padding: options.no_padding,
            unmatched_unit_policy: UnmatchedUnitPolicy::Keep,
        };
        let current = patch_bytes_from_toc(patch);
        let mut results = mode_a_web::run(
            &current,
            &request,
            source,
            mapping.category.as_str(),
            progress,
        )
        .await?;
        let result = results
            .pop()
            .ok_or_else(|| eyre::eyre!("mapping produced no target"))?;
        allowed_units.extend(changed_unit_ids(&before, &result.patch));
        record_changed_entries(&before, &result.patch, &mut writes)?;
        allowed_units.extend(target_unit_ids(mapping, &result.patch)?);
        merge_report(&mut report, result.report);
        patch = result.patch;
    }

    if options.unmatched_unit_policy == UnmatchedUnitPolicy::Drop {
        patch = filter_to_units(patch, &allowed_units);
    }
    report.mappings = variant.mappings.clone();
    Ok(VariantResult { patch, report })
}

fn validate_variants(variants: &[WebMigrationVariant]) -> crate::Result<()> {
    if variants.is_empty() {
        eyre::bail!("select at least one migration variant");
    }
    for variant in variants {
        if variant.mappings.is_empty() {
            eyre::bail!("each migration variant requires at least one mapping");
        }
        let mut sources = HashSet::new();
        for mapping in &variant.mappings {
            if mapping.source_hash == mapping.target_hash {
                eyre::bail!("source archive cannot also be a migration target");
            }
            if !sources.insert((mapping.category, mapping.source_hash.as_str())) {
                eyre::bail!("a migration variant cannot map the same source more than once");
            }
            ensure_hash_category(mapping.category, &mapping.source_hash)?;
            ensure_hash_category(mapping.category, &mapping.target_hash)?;
        }
    }
    Ok(())
}

fn ensure_hash_category(category: EquipmentCategory, hash: &str) -> crate::Result<()> {
    let found = super::migration::selectable_archive_entries(category.as_str())?
        .into_iter()
        .any(|entry| entry.hash == hash);
    if !found {
        eyre::bail!("archive {hash} does not belong to {}", category.as_str());
    }
    Ok(())
}

fn record_changed_entries(
    before: &StreamToc,
    after: &StreamToc,
    writes: &mut HashMap<(u64, u64), TocEntry>,
) -> crate::Result<()> {
    let before_by_key = entries_by_key(before);
    for entry in &after.entries {
        let key = (entry.type_id, entry.file_id);
        if before_by_key
            .get(&key)
            .is_some_and(|previous| entries_equal(previous, entry))
        {
            continue;
        }
        if let Some(previous) = writes.get(&key)
            && !entries_equal(previous, entry)
        {
            eyre::bail!(
                "combined migration produced conflicting resources for FileID 0x{:016x}, TypeID 0x{:016x}",
                entry.file_id,
                entry.type_id
            );
        }
        writes.insert(key, entry.clone());
    }
    Ok(())
}

fn changed_unit_ids(before: &StreamToc, after: &StreamToc) -> HashSet<u64> {
    let before_by_key = entries_by_key(before);
    after
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .filter(|entry| {
            before_by_key
                .get(&(entry.type_id, entry.file_id))
                .is_none_or(|previous| !entries_equal(previous, entry))
        })
        .map(|entry| entry.file_id)
        .collect()
}

fn target_unit_ids(
    mapping: &WebMigrationMapping,
    output: &StreamToc,
) -> crate::Result<HashSet<u64>> {
    let target_name = archive_name(mapping.category, &mapping.target_hash)?;
    let authoritative: HashSet<u64> = match mapping.category {
        EquipmentCategory::Armor => ArmorMappingTable::bundled()?
            .armor(&target_name)
            .map(|parts| parts.all_file_ids().into_iter().collect())
            .unwrap_or_default(),
        EquipmentCategory::Helmet => HelmetMappingTable::bundled()?
            .unit_id(&target_name)
            .into_iter()
            .collect(),
    };
    let output_units = output
        .entries
        .iter()
        .filter(|entry| entry.type_id == UNIT_ID)
        .map(|entry| entry.file_id)
        .collect::<HashSet<_>>();
    Ok(authoritative.intersection(&output_units).copied().collect())
}

fn archive_name(category: EquipmentCategory, hash: &str) -> crate::Result<String> {
    super::migration::selectable_archive_entries(category.as_str())?
        .into_iter()
        .find(|entry| entry.hash == hash)
        .map(|entry| entry.name.clone())
        .ok_or_else(|| eyre::eyre!("archive {hash} not found in {}", category.as_str()))
}

fn filter_to_units(patch: StreamToc, allowed_units: &HashSet<u64>) -> StreamToc {
    let source = StreamToc {
        entries: allowed_units
            .iter()
            .map(|file_id| TocEntry::new(*file_id, UNIT_ID))
            .collect(),
        ..Default::default()
    };
    source_selection::filter_patch_to_source_archive_units(&patch, &source).patch
}

fn patch_bytes_from_toc(mut patch: StreamToc) -> PatchBytes {
    let name = patch.name.clone();
    let (toc, gpu, stream) = patch.serialize();
    PatchBytes {
        name,
        toc,
        gpu,
        stream,
    }
}

fn entries_by_key(patch: &StreamToc) -> HashMap<(u64, u64), &TocEntry> {
    patch
        .entries
        .iter()
        .map(|entry| ((entry.type_id, entry.file_id), entry))
        .collect()
}

fn entries_equal(left: &TocEntry, right: &TocEntry) -> bool {
    left.toc_data == right.toc_data
        && left.gpu_data == right.gpu_data
        && left.stream_data == right.stream_data
}

fn empty_report(variant: &WebMigrationVariant) -> WebMigrationReportRow {
    WebMigrationReportRow {
        target_hash: variant
            .mappings
            .iter()
            .map(|mapping| mapping.target_hash.as_str())
            .collect::<Vec<_>>()
            .join(","),
        target_name: (variant.mappings.len() > 1)
            .then(|| "combined".to_string())
            .unwrap_or_default(),
        file_id_remapped: 0,
        slot_id_remapped: 0,
        padded_units: 0,
        skipped_entries: 0,
        warnings: Vec::new(),
        mappings: variant.mappings.clone(),
    }
}

fn merge_report(report: &mut WebMigrationReportRow, next: crate::migrator::MigrationReport) {
    if report.target_name.is_empty() {
        report.target_name = next.target_name;
    }
    report.file_id_remapped += next.file_id_remapped;
    report.slot_id_remapped += next.slot_id_remapped;
    report.padded_units += next.padded_units;
    report.skipped_entries += next.skipped_entries;
    report.warnings.extend(next.warnings);
}

fn variant_directory(
    variant: &WebMigrationVariant,
    target_name: &str,
    variant_index: usize,
    variant_count: usize,
) -> String {
    if variant.mappings.len() > 1 {
        return match variant_count {
            1 => "combined".to_string(),
            _ => format!("combined-{:03}", variant_index + 1),
        };
    }
    crate::migrator::safe_filename(target_name)
}

fn output_files(mut patch: StreamToc, directory: &str, suffix: &str) -> Vec<WebOutputFile> {
    let (toc, gpu, stream) = patch.serialize();
    [
        (suffix.to_string(), toc),
        (format!("{suffix}.gpu_resources"), gpu),
        (format!("{suffix}.stream"), stream),
    ]
    .into_iter()
    .map(|(name, bytes)| WebOutputFile {
        path: format!("{directory}/{name}"),
        bytes,
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_sources_in_one_variant() {
        let mapping = WebMigrationMapping {
            category: EquipmentCategory::Helmet,
            source_hash: "13f9269d08e52cf2".to_string(),
            target_hash: "a856edff49cfdd95".to_string(),
        };
        let variant = WebMigrationVariant {
            mappings: vec![mapping.clone(), mapping],
        };
        assert!(validate_variants(&[variant]).is_err());
    }

    #[test]
    fn rejects_source_as_its_own_target() {
        let variant = WebMigrationVariant {
            mappings: vec![WebMigrationMapping {
                category: EquipmentCategory::Helmet,
                source_hash: "13f9269d08e52cf2".to_string(),
                target_hash: "13f9269d08e52cf2".to_string(),
            }],
        };
        assert!(validate_variants(&[variant]).is_err());
    }

    #[test]
    fn numbers_combined_directories_when_a_batch_contains_multiple_variants() {
        let variant = WebMigrationVariant {
            mappings: vec![
                mapping(EquipmentCategory::Armor),
                mapping(EquipmentCategory::Helmet),
            ],
        };

        assert_eq!(
            variant_directory(&variant, "combined", 0, 2),
            "combined-001"
        );
        assert_eq!(
            variant_directory(&variant, "combined", 1, 2),
            "combined-002"
        );
        assert_eq!(variant_directory(&variant, "combined", 0, 1), "combined");
    }

    #[test]
    fn identical_writes_are_deduplicated_but_different_writes_fail() {
        let before = archive(&[]);
        let first = archive(&[entry(7, vec![1])]);
        let same = archive(&[entry(7, vec![1])]);
        let conflict = archive(&[entry(7, vec![2])]);
        let mut writes = HashMap::new();

        record_changed_entries(&before, &first, &mut writes).unwrap();
        record_changed_entries(&before, &same, &mut writes).unwrap();
        assert!(record_changed_entries(&before, &conflict, &mut writes).is_err());
    }

    #[test]
    fn drop_filter_keeps_selected_units_and_their_non_unit_content() {
        let patch = archive(&[entry(1, vec![1]), entry(2, vec![2]), TocEntry::new(9, 1234)]);
        let filtered = filter_to_units(patch, &HashSet::from([1]));

        assert!(filtered.find(1, UNIT_ID).is_some());
        assert!(filtered.find(2, UNIT_ID).is_none());
        assert!(filtered.find(9, 1234).is_some());
    }

    fn archive(entries: &[TocEntry]) -> StreamToc {
        StreamToc {
            entries: entries.to_vec(),
            ..Default::default()
        }
    }

    fn entry(file_id: u64, toc_data: Vec<u8>) -> TocEntry {
        let mut entry = TocEntry::new(file_id, UNIT_ID);
        entry.toc_data = toc_data;
        entry
    }

    fn mapping(category: EquipmentCategory) -> WebMigrationMapping {
        WebMigrationMapping {
            category,
            source_hash: "source".to_string(),
            target_hash: "target".to_string(),
        }
    }
}
