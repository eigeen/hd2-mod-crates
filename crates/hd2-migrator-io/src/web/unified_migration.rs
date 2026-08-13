use crate::archive::{StreamToc, TocEntry};
use crate::constants::UNIT_ID;
use crate::io::DataSource;
use crate::migrator::{mode_a_common, mode_a_web, source_selection};
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

/// Migrate every mapping from the original patch, then merge their independent outputs.
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
    let context = VariantMigrationContext {
        original: &patch_bytes,
        options: &options,
        source,
        progress,
    };
    for (variant_index, variant) in options.variants.iter().enumerate() {
        let result = migrate_variant(&context, variant).await?;
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

struct VariantMigrationContext<'a, S: DataSource + ?Sized> {
    original: &'a PatchBytes,
    options: &'a WebUnifiedMigrateOptions,
    source: &'a S,
    progress: Option<&'a dyn mode_a_web::WebProgress>,
}

async fn migrate_variant<S: DataSource + ?Sized>(
    context: &VariantMigrationContext<'_, S>,
    variant: &WebMigrationVariant,
) -> crate::Result<VariantResult> {
    let original_patch = parse_patch(context.original)?;
    let mut builder = VariantPatchBuilder::new(&original_patch);
    let mut report = empty_report(variant);
    for mapping in &variant.mappings {
        let mut result = migrate_mapping(context, mapping).await?;
        builder.merge_mapping(&original_patch, mapping, &mut result)?;
        merge_report(&mut report, result.report);
    }
    report.mappings = variant.mappings.clone();
    let patch = builder.finish(&original_patch, context.options.unmatched_unit_policy);
    Ok(VariantResult { patch, report })
}

async fn migrate_mapping<S: DataSource + ?Sized>(
    context: &VariantMigrationContext<'_, S>,
    mapping: &WebMigrationMapping,
) -> crate::Result<mode_a_web::WebTargetResult> {
    let request = WebMigrateOptions {
        source_hash: Some(mapping.source_hash.clone()),
        target_hashes: vec![mapping.target_hash.clone()],
        patch_suffix: context.options.patch_suffix.clone(),
        no_padding: context.options.no_padding,
        unmatched_unit_policy: UnmatchedUnitPolicy::Keep,
    };
    let mut results = mode_a_web::run(
        context.original,
        &request,
        context.source,
        mapping.category.as_str(),
        context.progress,
    )
    .await?;
    results
        .pop()
        .ok_or_else(|| eyre::eyre!("mapping produced no target"))
}

fn parse_patch(patch: &PatchBytes) -> crate::Result<StreamToc> {
    StreamToc::from_buffers(&patch.toc, &patch.gpu, &patch.stream, patch.name.clone())
}

struct VariantPatchBuilder {
    output: StreamToc,
    claimed_source_units: HashSet<u64>,
    preserved_source_units: HashSet<u64>,
}

impl VariantPatchBuilder {
    fn new(original: &StreamToc) -> Self {
        Self {
            output: StreamToc {
                entries: Vec::new(),
                ..original.clone()
            },
            claimed_source_units: HashSet::new(),
            preserved_source_units: HashSet::new(),
        }
    }

    fn merge_mapping(
        &mut self,
        original: &StreamToc,
        mapping: &WebMigrationMapping,
        result: &mut mode_a_web::WebTargetResult,
    ) -> crate::Result<()> {
        let mut output_units = target_unit_ids(mapping, &result.patch)?;
        output_units.extend(changed_unit_ids(original, &result.patch));
        let output = std::mem::take(&mut result.patch);
        self.merge_selected_output(&result.source_unit_ids, &output_units, output)
    }

    fn merge_selected_output(
        &mut self,
        source_units: &HashSet<u64>,
        output_units: &HashSet<u64>,
        output: StreamToc,
    ) -> crate::Result<()> {
        self.record_source_units(source_units, output_units, &output);
        let selected_output = filter_to_units(output, output_units);
        merge_output_entries(&mut self.output, &selected_output)
    }

    fn record_source_units(
        &mut self,
        source_units: &HashSet<u64>,
        output_units: &HashSet<u64>,
        output: &StreamToc,
    ) {
        preserve_unmapped_source_units(
            &mut self.preserved_source_units,
            source_units,
            output_units,
            output,
        );
        self.claimed_source_units.extend(source_units);
    }

    fn finish(mut self, original: &StreamToc, policy: UnmatchedUnitPolicy) -> StreamToc {
        if policy == UnmatchedUnitPolicy::Keep {
            preserve_original_units(
                &mut self.output,
                original,
                &self.claimed_source_units,
                &self.preserved_source_units,
            );
        }
        self.output
    }
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

fn merge_output_entries(output: &mut StreamToc, candidate: &StreamToc) -> crate::Result<()> {
    let mut positions = output
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| ((entry.type_id, entry.file_id), index))
        .collect::<HashMap<_, _>>();
    for entry in &candidate.entries {
        let key = (entry.type_id, entry.file_id);
        if let Some(index) = positions.get(&key).copied() {
            if entries_equal(&output.entries[index], entry) {
                continue;
            }
            eyre::bail!(
                "combined migration produced conflicting resources for FileID 0x{:016x}, TypeID 0x{:016x}",
                entry.file_id,
                entry.type_id
            );
        }
        positions.insert(key, output.entries.len());
        output.entries.push(entry.clone());
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

fn preserve_unmapped_source_units(
    preserved: &mut HashSet<u64>,
    source_units: &HashSet<u64>,
    output_units: &HashSet<u64>,
    output: &StreamToc,
) {
    let remaining_units = crate::web::migration::unit_file_ids(output);
    preserved.extend(
        source_units
            .intersection(&remaining_units)
            .filter(|file_id| !output_units.contains(file_id))
            .copied(),
    );
}

fn preserve_original_units(
    output: &mut StreamToc,
    original: &StreamToc,
    claimed_source_units: &HashSet<u64>,
    preserved_source_units: &HashSet<u64>,
) {
    let original_units = crate::web::migration::unit_file_ids(original);
    let mut retained_units = original_units
        .difference(claimed_source_units)
        .copied()
        .collect::<HashSet<_>>();
    retained_units.extend(preserved_source_units);
    let entries = source_selection::unit_dependency_entries(original, &retained_units);
    mode_a_common::merge_preserved_entries(output, &entries);
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
        let mut output = archive(&[]);
        let first = archive(&[entry(7, vec![1])]);
        let same = archive(&[entry(7, vec![1])]);
        let conflict = archive(&[entry(7, vec![2])]);

        merge_output_entries(&mut output, &first).unwrap();
        merge_output_entries(&mut output, &same).unwrap();
        assert!(merge_output_entries(&mut output, &conflict).is_err());
    }

    #[test]
    fn combines_independent_outputs_from_sources_that_share_units() {
        let original = archive(&[
            entry(1, vec![1]),
            entry(2, vec![2]),
            entry(3, vec![3]),
            entry(4, vec![4]),
        ]);
        let first = archive(&[entry(10, vec![1]), entry(11, vec![2])]);
        let second = archive(&[entry(20, vec![1]), entry(21, vec![3])]);
        let mut builder = VariantPatchBuilder::new(&original);

        builder
            .merge_selected_output(&HashSet::from([1, 2]), &HashSet::from([10, 11]), first)
            .unwrap();
        builder
            .merge_selected_output(&HashSet::from([1, 3]), &HashSet::from([20, 21]), second)
            .unwrap();
        let combined = builder.finish(&original, UnmatchedUnitPolicy::Keep);

        assert_eq!(
            crate::web::migration::unit_file_ids(&combined),
            HashSet::from([4, 10, 11, 20, 21]),
        );
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
