use crate::archive::{StreamToc, TocEntry};
use crate::constants::UNIT_ID;
use crate::io::DataSource;
use crate::migrator::{mode_a_common, mode_a_web, source_selection};
use crate::operations::equipment::{EquipmentCategory, WebMigrationMapping};
use crate::operations::migration::{
    PatchBytes, UnmatchedUnitPolicy, WebMigrationBundle, WebMigrationReportRow,
    WebMigrationSummary, WebOutputFile,
};
use crate::unit::authority::ArmorMappingTable;
use crate::unit::helmet_authority::HelmetMappingTable;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::archive_output::PreparedArchiveOutput;

#[cfg(not(target_family = "wasm"))]
mod parallel;
mod prepared;
mod unit_behavior;
mod unit_plan;

#[cfg(not(target_family = "wasm"))]
pub use parallel::{ParallelVariantPatchCallbacks, migrate_variants_to_patch_sink_parallel};
use prepared::MigrationExecutor;
use unit_behavior::CompiledUnitBehavior;
pub use unit_behavior::{
    WebUnitBehaviorOptions, WebUnitConflictResolution, WebUnitExportOverride,
    WebUnitMappingBehaviorKey,
};
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
    #[serde(default)]
    pub unit_behavior: WebUnitBehaviorOptions,
}

struct CompiledMigrationPlan {
    suffix: String,
    unit_behavior: CompiledUnitBehavior,
    unit_plans: Vec<unit_plan::VariantUnitPlan>,
}

impl CompiledMigrationPlan {
    fn compile(options: &WebUnifiedMigrateOptions) -> crate::Result<Self> {
        validate_variants(&options.variants)?;
        let unit_behavior = CompiledUnitBehavior::compile(&options.unit_behavior)?;
        let unit_plans = unit_plan::build_variant_plans(&options.variants, &unit_behavior)?;
        let suffix = options
            .patch_suffix
            .clone()
            .unwrap_or_else(|| super::migration::DEFAULT_PATCH_SUFFIX.to_owned());
        Ok(Self {
            suffix,
            unit_behavior,
            unit_plans,
        })
    }
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

impl VariantPatchOutput {
    pub fn into_prepared_output(self) -> PreparedArchiveOutput {
        let toc_path = format!("{}/{}", self.directory, self.suffix);
        PreparedArchiveOutput::new(self.patch, toc_path)
    }
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
            output
                .into_prepared_output()
                .write_files(&mut callbacks.write_file)
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
    let plan = CompiledMigrationPlan::compile(&options)?;
    let mut reports = Vec::new();
    let original = parse_patch(&patch_bytes)?;
    let executor =
        MigrationExecutor::new(&original, source, callbacks.progress, options.no_padding).await?;
    let mut context = VariantMigrationContext {
        executor,
        original: &original,
        unmatched_unit_policy: options.unmatched_unit_policy,
        unit_behavior: plan.unit_behavior,
    };
    for (variant_index, (variant, unit_plan)) in options
        .variants
        .iter()
        .zip(plan.unit_plans.iter())
        .enumerate()
    {
        let result = migrate_variant(&mut context, variant, unit_plan).await?;
        let (output, report) = finish_variant_result(&options, &plan.suffix, variant_index, result);
        (callbacks.write_patch)(output)?;
        reports.push(report);
    }
    Ok(summary_from_reports(reports))
}

fn summary_from_reports(reports: Vec<WebMigrationReportRow>) -> WebMigrationSummary {
    WebMigrationSummary {
        migrated_count: reports.len(),
        warning_count: reports.iter().map(|report| report.warnings.len()).sum(),
        reports,
    }
}

fn finish_variant_result(
    options: &WebUnifiedMigrateOptions,
    suffix: &str,
    variant_index: usize,
    result: VariantResult,
) -> (VariantPatchOutput, WebMigrationReportRow) {
    let directory = variant_directory(
        &options.variants[variant_index],
        &result.report.target_name,
        variant_index,
        options.variants.len(),
    );
    (
        VariantPatchOutput {
            patch: result.patch,
            directory,
            suffix: suffix.to_owned(),
        },
        result.report,
    )
}

struct VariantResult {
    patch: StreamToc,
    report: WebMigrationReportRow,
}

struct VariantMigrationContext<'a, S: DataSource + ?Sized> {
    executor: MigrationExecutor<'a, S>,
    original: &'a StreamToc,
    unmatched_unit_policy: UnmatchedUnitPolicy,
    unit_behavior: CompiledUnitBehavior,
}

async fn migrate_variant<S: DataSource + ?Sized>(
    context: &mut VariantMigrationContext<'_, S>,
    variant: &WebMigrationVariant,
    unit_plan: &unit_plan::VariantUnitPlan,
) -> crate::Result<VariantResult> {
    if variant.mappings.len() == 1 {
        return migrate_single_mapping_variant(context, variant, unit_plan).await;
    }
    let mut assembly = VariantAssembly::new(
        context.original,
        variant,
        context.unmatched_unit_policy,
        context.unit_behavior.clone(),
    );
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
        unit_behavior: CompiledUnitBehavior,
    ) -> Self {
        let mut report = empty_report(variant);
        report.unmatched_unit_policy = policy;
        Self {
            builder: VariantPatchBuilder::new(original, unit_behavior),
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
            unit_behavior: context.unit_behavior.clone(),
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
    unit_behavior: CompiledUnitBehavior,
}

fn assemble_single_mapping(
    context: SingleMappingAssembly<'_>,
    mut result: mode_a_web::WebTargetResult,
) -> crate::Result<VariantResult> {
    let mut builder = VariantPatchBuilder::new(context.original, context.unit_behavior);
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
    unit_behavior: CompiledUnitBehavior,
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
    fn new(original: &StreamToc, unit_behavior: CompiledUnitBehavior) -> Self {
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
            unit_behavior,
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
        let selected_edges = unit_edges
            .iter()
            .filter(|edge| self.unit_behavior.selects_edge(edge))
            .collect::<Vec<_>>();
        let selected_targets = selected_edges
            .iter()
            .map(|edge| edge.target_file_id)
            .collect::<HashSet<_>>();
        for target in unit_edge_target_ids(unit_edges).difference(&selected_targets) {
            output_units.remove(target);
        }
        for edge in selected_edges {
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
        self.preserve_selected_original_units(original, policy);
        self.output
    }

    fn preserve_selected_original_units(
        &mut self,
        original: &StreamToc,
        policy: UnmatchedUnitPolicy,
    ) {
        let retained_units = crate::operations::migration::unit_file_ids(original)
            .into_iter()
            .filter(|file_id| self.should_preserve_original_unit(*file_id, policy))
            .collect::<HashSet<_>>();
        let entries = source_selection::unit_dependency_entries(original, &retained_units);
        mode_a_common::merge_preserved_entries(&mut self.output, &entries);
    }

    fn should_preserve_original_unit(&self, file_id: u64, policy: UnmatchedUnitPolicy) -> bool {
        let unclaimed = !self.claimed_source_units.contains(&file_id)
            || self.preserved_source_units.contains(&file_id);
        match self.unit_behavior.export_override(file_id) {
            Some(false) => false,
            Some(true) => unclaimed,
            None => policy == UnmatchedUnitPolicy::Keep && unclaimed,
        }
    }

    fn unconverted_original_units(&self, original: &StreamToc) -> HashSet<u64> {
        let original_units = crate::operations::migration::unit_file_ids(original);
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
    let remaining_units = crate::operations::migration::unit_file_ids(output);
    preserved.extend(
        source_units
            .intersection(&remaining_units)
            .filter(|file_id| !output_units.contains(file_id))
            .copied(),
    );
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
        target_name: if variant.mappings.len() > 1 {
            "combined".to_string()
        } else {
            String::new()
        },
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

#[cfg(test)]
mod tests;
