use crate::archive::{StreamToc, TocEntry};
use crate::constants::UNIT_ID;
use crate::io::DataSource;
use crate::migrator::{mode_a_common, mode_a_web, source_selection};
use crate::unit::authority::ArmorMappingTable;
use crate::unit::helmet_authority::HelmetMappingTable;
use crate::web::equipment::{EquipmentCategory, WebMigrationMapping};
use crate::web::migration::{
    PatchBytes, UnmatchedUnitPolicy, WebMigrationBundle, WebMigrationReportRow,
    WebMigrationSummary, WebOutputFile,
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[cfg(not(target_family = "wasm"))]
mod parallel;
mod prepared;
mod unit_plan;

#[cfg(not(target_family = "wasm"))]
pub use parallel::{ParallelVariantPatchCallbacks, migrate_variants_to_patch_sink_parallel};
use prepared::MigrationExecutor;
use unit_plan::UnitMappingEdge;

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

pub struct VariantMigrationCallbacks<'a, F> {
    progress: Option<&'a dyn mode_a_web::WebProgress>,
    write_file: F,
}

pub struct VariantPatchOutput {
    pub patch: StreamToc,
    pub directory: String,
    pub suffix: String,
}

pub struct VariantPatchCallbacks<'a, F> {
    progress: Option<&'a dyn mode_a_web::WebProgress>,
    write_patch: F,
}

impl<'a, F> VariantPatchCallbacks<'a, F> {
    pub fn new(progress: Option<&'a dyn mode_a_web::WebProgress>, write_patch: F) -> Self {
        Self {
            progress,
            write_patch,
        }
    }
}

impl<'a, F> VariantMigrationCallbacks<'a, F> {
    pub fn new(progress: Option<&'a dyn mode_a_web::WebProgress>, write_file: F) -> Self {
        Self {
            progress,
            write_file,
        }
    }
}

/// Migrate every mapping from the original patch, then merge their independent outputs.
pub async fn migrate_variants_with_source<S: DataSource + ?Sized>(
    patch_bytes: PatchBytes,
    options: WebUnifiedMigrateOptions,
    source: &S,
    progress: Option<&dyn mode_a_web::WebProgress>,
) -> crate::Result<WebMigrationBundle> {
    let mut files = Vec::new();
    let callbacks = VariantMigrationCallbacks::new(progress, |file| {
        files.push(file);
        Ok(())
    });
    let summary = migrate_variants_to_sink(patch_bytes, options, source, callbacks).await?;
    Ok(WebMigrationBundle { files, summary })
}

/// Migrate variants and release each serialized output after the sink consumes it.
pub async fn migrate_variants_to_sink<S, F>(
    patch_bytes: PatchBytes,
    options: WebUnifiedMigrateOptions,
    source: &S,
    mut callbacks: VariantMigrationCallbacks<'_, F>,
) -> crate::Result<WebMigrationSummary>
where
    S: DataSource + ?Sized,
    F: FnMut(WebOutputFile) -> crate::Result<()>,
{
    let patch_callbacks =
        VariantPatchCallbacks::new(callbacks.progress, |output: VariantPatchOutput| {
            for file in output_files(output.patch, &output.directory, &output.suffix) {
                (callbacks.write_file)(file)?;
            }
            Ok(())
        });
    migrate_variants_to_patch_sink(patch_bytes, options, source, patch_callbacks).await
}

/// Migrate variants and hand each native archive to a sink before processing the next variant.
pub async fn migrate_variants_to_patch_sink<S, F>(
    patch_bytes: PatchBytes,
    options: WebUnifiedMigrateOptions,
    source: &S,
    mut callbacks: VariantPatchCallbacks<'_, F>,
) -> crate::Result<WebMigrationSummary>
where
    S: DataSource + ?Sized,
    F: FnMut(VariantPatchOutput) -> crate::Result<()>,
{
    validate_variants(&options.variants)?;
    let unit_plans = unit_plan::build_variant_plans(&options.variants)?;
    let suffix = options
        .patch_suffix
        .as_deref()
        .unwrap_or(super::migration::DEFAULT_PATCH_SUFFIX);
    let mut reports = Vec::new();
    let original = parse_patch(&patch_bytes)?;
    let executor =
        MigrationExecutor::new(&original, source, callbacks.progress, options.no_padding).await?;
    let mut context = VariantMigrationContext {
        executor,
        original: &original,
        unmatched_unit_policy: options.unmatched_unit_policy,
    };
    for (variant_index, (variant, unit_plan)) in
        options.variants.iter().zip(unit_plans.iter()).enumerate()
    {
        let result = migrate_variant(&mut context, variant, unit_plan).await?;
        let directory = variant_directory(
            variant,
            &result.report.target_name,
            variant_index,
            options.variants.len(),
        );
        (callbacks.write_patch)(VariantPatchOutput {
            patch: result.patch,
            directory,
            suffix: suffix.to_owned(),
        })?;
        reports.push(result.report);
    }
    Ok(WebMigrationSummary {
        migrated_count: reports.len(),
        warning_count: reports.iter().map(|report| report.warnings.len()).sum(),
        reports,
    })
}

struct VariantResult {
    patch: StreamToc,
    report: WebMigrationReportRow,
}

struct VariantMigrationContext<'a, S: DataSource + ?Sized> {
    executor: MigrationExecutor<'a, S>,
    original: &'a StreamToc,
    unmatched_unit_policy: UnmatchedUnitPolicy,
}

async fn migrate_variant<S: DataSource + ?Sized>(
    context: &mut VariantMigrationContext<'_, S>,
    variant: &WebMigrationVariant,
    unit_plan: &unit_plan::VariantUnitPlan,
) -> crate::Result<VariantResult> {
    if variant.mappings.len() == 1 {
        return migrate_single_mapping_variant(context, variant, unit_plan).await;
    }
    let mut assembly =
        VariantAssembly::new(context.original, variant, context.unmatched_unit_policy);
    for (mapping, authoritative_edges) in variant.mappings.iter().zip(&unit_plan.mapping_edges) {
        let result = migrate_mapping(context, mapping).await?;
        assembly.merge(mapping, authoritative_edges, result)?;
    }
    Ok(assembly.finish())
}

struct VariantAssembly<'a> {
    builder: VariantPatchBuilder,
    original: &'a StreamToc,
    policy: UnmatchedUnitPolicy,
    report: WebMigrationReportRow,
    variant: &'a WebMigrationVariant,
}

impl<'a> VariantAssembly<'a> {
    fn new(
        original: &'a StreamToc,
        variant: &'a WebMigrationVariant,
        policy: UnmatchedUnitPolicy,
    ) -> Self {
        let mut report = empty_report(variant);
        report.unmatched_unit_policy = policy;
        Self {
            builder: VariantPatchBuilder::new(original),
            original,
            policy,
            report,
            variant,
        }
    }

    fn merge(
        &mut self,
        mapping: &WebMigrationMapping,
        mapping_edges: &[UnitMappingEdge],
        mut result: mode_a_web::WebTargetResult,
    ) -> crate::Result<()> {
        self.builder
            .merge_mapping(self.original, mapping, mapping_edges, &mut result)?;
        merge_report_totals(&mut self.report, result.report);
        Ok(())
    }

    fn finish(mut self) -> VariantResult {
        self.report.warnings = combined_variant_warnings(&self.builder, self.original, self.policy);
        self.report.unmatched_units = self.builder.unconverted_original_units(self.original).len();
        self.report.mappings = self.variant.mappings.clone();
        VariantResult {
            patch: self.builder.finish(self.original, self.policy),
            report: self.report,
        }
    }
}

async fn migrate_single_mapping_variant<S: DataSource + ?Sized>(
    context: &mut VariantMigrationContext<'_, S>,
    variant: &WebMigrationVariant,
    unit_plan: &unit_plan::VariantUnitPlan,
) -> crate::Result<VariantResult> {
    let [mapping] = variant.mappings.as_slice() else {
        eyre::bail!("single-mapping migration received multiple mappings");
    };
    let [mapping_edges] = unit_plan.mapping_edges.as_slice() else {
        eyre::bail!("single-mapping migration received multiple Unit plans");
    };
    let result = migrate_mapping(context, mapping).await?;
    assemble_single_mapping(
        SingleMappingAssembly {
            mapping,
            mapping_edges,
            original: context.original,
            policy: context.unmatched_unit_policy,
            variant,
        },
        result,
    )
}

struct SingleMappingAssembly<'a> {
    mapping: &'a WebMigrationMapping,
    mapping_edges: &'a [UnitMappingEdge],
    original: &'a StreamToc,
    policy: UnmatchedUnitPolicy,
    variant: &'a WebMigrationVariant,
}

fn assemble_single_mapping(
    context: SingleMappingAssembly<'_>,
    mut result: mode_a_web::WebTargetResult,
) -> crate::Result<VariantResult> {
    let mut builder = VariantPatchBuilder::new(context.original);
    builder.merge_mapping(
        context.original,
        context.mapping,
        context.mapping_edges,
        &mut result,
    )?;
    let mut report = empty_report(context.variant);
    report.unmatched_unit_policy = context.policy;
    merge_report(&mut report, result.report);
    report.unmatched_units = builder.unconverted_original_units(context.original).len();
    report.mappings = context.variant.mappings.clone();
    let patch = builder.finish(context.original, context.policy);
    Ok(VariantResult { patch, report })
}

async fn migrate_mapping<S: DataSource + ?Sized>(
    context: &mut VariantMigrationContext<'_, S>,
    mapping: &WebMigrationMapping,
) -> crate::Result<mode_a_web::WebTargetResult> {
    context.executor.migrate(mapping).await
}

fn parse_patch(patch: &PatchBytes) -> crate::Result<StreamToc> {
    StreamToc::from_buffers(&patch.toc, &patch.gpu, &patch.stream, patch.name.clone())
}

struct VariantPatchBuilder {
    output: StreamToc,
    claimed_source_units: HashSet<u64>,
    claimed_target_units: HashMap<u64, UnitClaim>,
    preserved_source_units: HashSet<u64>,
}

struct UnitClaim {
    source_file_id: u64,
    category: EquipmentCategory,
    source_hash: String,
    target_hash: String,
}

impl UnitClaim {
    fn new(mapping: &WebMigrationMapping, source_file_id: u64) -> Self {
        Self {
            source_file_id,
            category: mapping.category,
            source_hash: mapping.source_hash.clone(),
            target_hash: mapping.target_hash.clone(),
        }
    }
}

impl VariantPatchBuilder {
    fn new(original: &StreamToc) -> Self {
        Self {
            output: StreamToc {
                types: original.types.clone(),
                entries: Vec::new(),
                unknown: original.unknown,
                unk4_data: original.unk4_data,
                name: original.name.clone(),
            },
            claimed_source_units: HashSet::new(),
            claimed_target_units: HashMap::new(),
            preserved_source_units: HashSet::new(),
        }
    }

    fn merge_mapping(
        &mut self,
        original: &StreamToc,
        mapping: &WebMigrationMapping,
        authoritative_edges: &[UnitMappingEdge],
        result: &mut mode_a_web::WebTargetResult,
    ) -> crate::Result<()> {
        let mut output_units = target_unit_ids(mapping, &result.patch)?;
        output_units.extend(changed_unit_ids(original, &result.patch));
        let unit_edges = effective_unit_edges(authoritative_edges, mapping, result, &output_units);
        self.remove_redundant_unit_outputs(mapping, &mut output_units, &unit_edges)?;
        let output = std::mem::take(&mut result.patch);
        self.merge_selected_output(&result.source_unit_ids, &output_units, output)
    }

    fn remove_redundant_unit_outputs(
        &mut self,
        mapping: &WebMigrationMapping,
        output_units: &mut HashSet<u64>,
        unit_edges: &[UnitMappingEdge],
    ) -> crate::Result<()> {
        for edge in unit_edges {
            if output_units.contains(&edge.target_file_id)
                && !self.claim_unit_edge(mapping, edge)?
            {
                output_units.remove(&edge.target_file_id);
            }
        }
        Ok(())
    }

    fn claim_unit_edge(
        &mut self,
        mapping: &WebMigrationMapping,
        edge: &UnitMappingEdge,
    ) -> crate::Result<bool> {
        let Some(previous) = self.claimed_target_units.get(&edge.target_file_id) else {
            self.claimed_target_units.insert(
                edge.target_file_id,
                UnitClaim::new(mapping, edge.source_file_id),
            );
            return Ok(true);
        };
        if claims_are_compatible(previous, mapping, edge) {
            return Ok(false);
        }
        eyre::bail!(
            "combined migration maps different source Units to target FileID 0x{:016x}: source 0x{:016x} conflicts with source 0x{:016x}",
            edge.target_file_id,
            previous.source_file_id,
            edge.source_file_id,
        )
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

    fn unconverted_original_units(&self, original: &StreamToc) -> HashSet<u64> {
        let original_units = crate::web::migration::unit_file_ids(original);
        let mut units = original_units
            .difference(&self.claimed_source_units)
            .copied()
            .collect::<HashSet<_>>();
        units.extend(self.preserved_source_units.iter().copied());
        units
    }
}

fn claims_are_compatible(
    previous: &UnitClaim,
    mapping: &WebMigrationMapping,
    edge: &UnitMappingEdge,
) -> bool {
    previous.source_file_id == edge.source_file_id
        || (previous.category == mapping.category
            && previous.source_hash == mapping.source_hash
            && previous.target_hash != mapping.target_hash)
}

fn validate_variants(variants: &[WebMigrationVariant]) -> crate::Result<()> {
    if variants.is_empty() {
        eyre::bail!("select at least one migration variant");
    }
    for variant in variants {
        if variant.mappings.is_empty() {
            eyre::bail!("each migration variant requires at least one mapping");
        }
        let mut mappings = HashSet::new();
        for mapping in &variant.mappings {
            if mapping.source_hash == mapping.target_hash {
                eyre::bail!("source archive cannot also be a migration target");
            }
            if !mappings.insert((
                mapping.category,
                mapping.source_hash.as_str(),
                mapping.target_hash.as_str(),
            )) {
                eyre::bail!("a migration variant cannot contain duplicate mappings");
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

fn effective_unit_edges(
    authoritative_edges: &[UnitMappingEdge],
    mapping: &WebMigrationMapping,
    result: &mode_a_web::WebTargetResult,
    output_units: &HashSet<u64>,
) -> Vec<UnitMappingEdge> {
    let mut edges = authoritative_edges.to_vec();
    edges.extend(runtime_unit_edges(authoritative_edges, mapping, result));
    edges.extend(retained_unit_edges(mapping, output_units, &edges));
    edges
}

fn runtime_unit_edges(
    authoritative_edges: &[UnitMappingEdge],
    mapping: &WebMigrationMapping,
    result: &mode_a_web::WebTargetResult,
) -> Vec<UnitMappingEdge> {
    let covered_targets = unit_edge_target_ids(authoritative_edges);
    result
        .unit_mappings
        .iter()
        .filter(|(_, target_file_id)| !covered_targets.contains(target_file_id))
        .map(|(source_file_id, target_file_id)| {
            UnitMappingEdge::described(
                *source_file_id,
                *target_file_id,
                format!(
                    "{} {} -> {}",
                    mapping.category.as_str(),
                    mapping.source_hash,
                    result.target_name,
                ),
            )
        })
        .collect()
}

fn retained_unit_edges(
    mapping: &WebMigrationMapping,
    output_units: &HashSet<u64>,
    covered_edges: &[UnitMappingEdge],
) -> Vec<UnitMappingEdge> {
    let covered_targets = unit_edge_target_ids(covered_edges);
    output_units
        .difference(&covered_targets)
        .map(|file_id| identity_unit_edge(mapping, *file_id))
        .collect()
}

fn unit_edge_target_ids(edges: &[UnitMappingEdge]) -> HashSet<u64> {
    edges.iter().map(|edge| edge.target_file_id).collect()
}

fn identity_unit_edge(mapping: &WebMigrationMapping, file_id: u64) -> UnitMappingEdge {
    UnitMappingEdge::described(
        file_id,
        file_id,
        format!(
            "{} {} retained Unit",
            mapping.category.as_str(),
            mapping.source_hash,
        ),
    )
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
        unmatched_units: 0,
        unmatched_unit_policy: UnmatchedUnitPolicy::Keep,
        warnings: Vec::new(),
        mappings: variant.mappings.clone(),
    }
}

fn merge_report(report: &mut WebMigrationReportRow, next: crate::migrator::MigrationReport) {
    let warnings = next.warnings.clone();
    merge_report_totals(report, next);
    report.warnings.extend(warnings);
}

fn merge_report_totals(report: &mut WebMigrationReportRow, next: crate::migrator::MigrationReport) {
    if report.target_name.is_empty() {
        report.target_name = next.target_name;
    }
    report.file_id_remapped += next.file_id_remapped;
    report.slot_id_remapped += next.slot_id_remapped;
    report.padded_units += next.padded_units;
    report.skipped_entries += next.skipped_entries;
}

fn combined_variant_warnings(
    builder: &VariantPatchBuilder,
    original: &StreamToc,
    policy: UnmatchedUnitPolicy,
) -> Vec<String> {
    let count = builder.unconverted_original_units(original).len();
    if count == 0 {
        return Vec::new();
    }
    let action = match policy {
        UnmatchedUnitPolicy::Keep => "kept",
        UnmatchedUnitPolicy::Drop => "dropped",
    };
    let noun = if count == 1 { "part" } else { "parts" };
    vec![format!(
        "{action} {count} {noun} not covered by the configured equipment mappings"
    )]
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
    fn allows_one_source_to_map_to_multiple_targets_in_one_variant() {
        let mapping = WebMigrationMapping {
            category: EquipmentCategory::Helmet,
            source_hash: "13f9269d08e52cf2".to_string(),
            target_hash: "a856edff49cfdd95".to_string(),
        };
        let variant = WebMigrationVariant {
            mappings: vec![
                mapping,
                WebMigrationMapping {
                    category: EquipmentCategory::Helmet,
                    source_hash: "13f9269d08e52cf2".to_string(),
                    target_hash: "1a2fc86abd27bf5b".to_string(),
                },
            ],
        };
        assert!(validate_variants(&[variant]).is_ok());
    }

    #[test]
    fn core_accepts_more_variants_than_the_web_ui_limit() {
        let variants = (0..64)
            .map(|_| WebMigrationVariant {
                mappings: vec![WebMigrationMapping {
                    category: EquipmentCategory::Helmet,
                    source_hash: "13f9269d08e52cf2".to_string(),
                    target_hash: "a856edff49cfdd95".to_string(),
                }],
            })
            .collect::<Vec<_>>();

        assert!(validate_variants(&variants).is_ok());
    }

    #[test]
    fn rejects_duplicate_mappings_in_one_variant() {
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
    fn repeated_unit_mapping_edge_is_emitted_only_once() {
        let mut builder = VariantPatchBuilder::new(&archive(&[]));
        let first_mapping = mapping_with_hashes(EquipmentCategory::Armor, "source", "target-a");
        let repeated_mapping = mapping_with_hashes(EquipmentCategory::Armor, "source", "target-b");
        let first_edge = UnitMappingEdge::test_edge(1, 9);
        let repeated_edge = UnitMappingEdge::test_edge(2, 9);
        let mut first_output = HashSet::from([9]);
        let mut repeated_output = HashSet::from([9]);
        let first = archive(&[entry(9, vec![1])]);
        let repeated = archive(&[entry(9, vec![2])]);

        builder
            .remove_redundant_unit_outputs(
                &first_mapping,
                &mut first_output,
                std::slice::from_ref(&first_edge),
            )
            .unwrap();
        builder
            .merge_selected_output(&HashSet::from([1]), &first_output, first)
            .unwrap();
        builder
            .remove_redundant_unit_outputs(
                &repeated_mapping,
                &mut repeated_output,
                &[repeated_edge],
            )
            .unwrap();
        builder
            .merge_selected_output(&HashSet::from([1]), &repeated_output, repeated)
            .unwrap();

        assert_eq!(first_output, HashSet::from([9]));
        assert!(repeated_output.is_empty());
        assert_eq!(builder.output.find(9, UNIT_ID).unwrap().toc_data, vec![1]);
    }

    #[test]
    fn different_source_equipment_unit_edges_conflict() {
        let mut builder = VariantPatchBuilder::new(&archive(&[]));
        let first_mapping = mapping_with_hashes(EquipmentCategory::Armor, "source-a", "target-a");
        let second_mapping = mapping_with_hashes(EquipmentCategory::Armor, "source-b", "target-b");
        let mut first_output = HashSet::from([9]);
        let mut second_output = HashSet::from([9]);

        builder
            .remove_redundant_unit_outputs(
                &first_mapping,
                &mut first_output,
                &[UnitMappingEdge::test_edge(1, 9)],
            )
            .unwrap();
        let error = builder
            .remove_redundant_unit_outputs(
                &second_mapping,
                &mut second_output,
                &[UnitMappingEdge::test_edge(2, 9)],
            )
            .unwrap_err();

        assert!(error.to_string().contains("different source Units"));
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
    fn combined_warning_is_empty_when_every_original_unit_is_mapped() {
        let original = archive(&[entry(1, vec![1]), entry(2, vec![2])]);
        let mut builder = VariantPatchBuilder::new(&original);
        builder
            .merge_selected_output(
                &HashSet::from([1, 2]),
                &HashSet::from([10, 20]),
                archive(&[entry(10, vec![1]), entry(20, vec![2])]),
            )
            .unwrap();

        assert!(
            combined_variant_warnings(&builder, &original, UnmatchedUnitPolicy::Keep).is_empty()
        );
        assert_eq!(builder.unconverted_original_units(&original).len(), 0);
    }

    #[test]
    fn combined_warning_reports_units_outside_configured_mappings() {
        let original = archive(&[entry(1, vec![1]), entry(2, vec![2])]);
        let mut builder = VariantPatchBuilder::new(&original);
        builder
            .merge_selected_output(
                &HashSet::from([1]),
                &HashSet::from([10]),
                archive(&[entry(10, vec![1])]),
            )
            .unwrap();

        let keep = combined_variant_warnings(&builder, &original, UnmatchedUnitPolicy::Keep);
        let drop = combined_variant_warnings(&builder, &original, UnmatchedUnitPolicy::Drop);
        assert_eq!(builder.unconverted_original_units(&original).len(), 1);
        assert_eq!(
            keep,
            ["kept 1 part not covered by the configured equipment mappings"]
        );
        assert_eq!(
            drop,
            ["dropped 1 part not covered by the configured equipment mappings"]
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
        mapping_with_hashes(category, "source", "target")
    }

    fn mapping_with_hashes(
        category: EquipmentCategory,
        source_hash: &str,
        target_hash: &str,
    ) -> WebMigrationMapping {
        WebMigrationMapping {
            category,
            source_hash: source_hash.to_string(),
            target_hash: target_hash.to_string(),
        }
    }
}
