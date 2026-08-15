use super::prepared::{MigrationExecutor, PreparedWork};
use super::*;
use rayon::prelude::*;

pub struct ParallelVariantPatchCallbacks<'a, F> {
    progress: Option<&'a (dyn mode_a_web::WebProgress + Sync)>,
    write_patch: F,
}

impl<'a, F> ParallelVariantPatchCallbacks<'a, F> {
    pub fn new(progress: Option<&'a (dyn mode_a_web::WebProgress + Sync)>, write_patch: F) -> Self {
        Self {
            progress,
            write_patch,
        }
    }
}

struct ParallelRunState<'a, S: DataSource + ?Sized> {
    executor: MigrationExecutor<'a, S>,
    options: &'a WebUnifiedMigrateOptions,
    original: &'a StreamToc,
    progress: Option<&'a (dyn mode_a_web::WebProgress + Sync)>,
    suffix: &'a str,
    unit_plans: &'a [unit_plan::VariantUnitPlan],
}

struct SingleVariantWork<'a> {
    assembly: SingleMappingAssembly<'a>,
    index: usize,
    work: PreparedWork,
}

struct MappingWork<'a> {
    edges: &'a [UnitMappingEdge],
    mapping: &'a WebMigrationMapping,
    work: PreparedWork,
}

/// Preload bounded batches, compute them with Rayon, then write in selection order.
pub async fn migrate_variants_to_patch_sink_parallel<S, F>(
    patch_bytes: PatchBytes,
    options: WebUnifiedMigrateOptions,
    source: &S,
    mut callbacks: ParallelVariantPatchCallbacks<'_, F>,
) -> crate::Result<WebMigrationSummary>
where
    S: DataSource + Sync + ?Sized,
    F: FnMut(VariantPatchOutput) -> crate::Result<()>,
{
    validate_variants(&options.variants)?;
    let unit_plans = unit_plan::build_variant_plans(&options.variants)?;
    let original = parse_patch(&patch_bytes)?;
    let web_progress = callbacks
        .progress
        .map(|progress| progress as &dyn mode_a_web::WebProgress);
    let executor =
        MigrationExecutor::new(&original, source, web_progress, options.no_padding).await?;
    let suffix = options
        .patch_suffix
        .as_deref()
        .unwrap_or(super::super::migration::DEFAULT_PATCH_SUFFIX);
    let mut state = ParallelRunState {
        executor,
        options: &options,
        original: &original,
        progress: callbacks.progress,
        suffix,
        unit_plans: &unit_plans,
    };
    let reports = if options
        .variants
        .iter()
        .all(|variant| variant.mappings.len() == 1)
    {
        migrate_single_variants(&mut state, &mut callbacks.write_patch).await?
    } else {
        migrate_combined_variants(&mut state, &mut callbacks.write_patch).await?
    };
    Ok(summary_from_reports(reports))
}

async fn migrate_single_variants<S, F>(
    state: &mut ParallelRunState<'_, S>,
    write_patch: &mut F,
) -> crate::Result<Vec<WebMigrationReportRow>>
where
    S: DataSource + Sync + ?Sized,
    F: FnMut(VariantPatchOutput) -> crate::Result<()>,
{
    let mut reports = Vec::with_capacity(state.options.variants.len());
    for start in (0..state.options.variants.len()).step_by(parallel_batch_size()) {
        let end = (start + parallel_batch_size()).min(state.options.variants.len());
        let batch = prepare_single_batch(state, start..end).await?;
        let results = batch
            .into_par_iter()
            .map(|item| compute_single_variant(item, state.progress))
            .collect::<crate::Result<Vec<_>>>()?;
        write_parallel_results(state, write_patch, results, &mut reports)?;
    }
    Ok(reports)
}

async fn prepare_single_batch<'a, S: DataSource + Sync + ?Sized>(
    state: &mut ParallelRunState<'a, S>,
    range: std::ops::Range<usize>,
) -> crate::Result<Vec<SingleVariantWork<'a>>> {
    let mut batch = Vec::with_capacity(range.len());
    for index in range {
        let variant = &state.options.variants[index];
        let [mapping] = variant.mappings.as_slice() else {
            eyre::bail!("parallel single variant requires one mapping");
        };
        let [mapping_edges] = state.unit_plans[index].mapping_edges.as_slice() else {
            eyre::bail!("parallel single variant requires one Unit plan");
        };
        let work = state
            .executor
            .prepare_parallel_work(mapping, state.progress)
            .await?;
        batch.push(SingleVariantWork {
            assembly: SingleMappingAssembly {
                mapping,
                mapping_edges,
                original: state.original,
                policy: state.options.unmatched_unit_policy,
                variant,
            },
            index,
            work,
        });
    }
    Ok(batch)
}

fn compute_single_variant(
    item: SingleVariantWork<'_>,
    progress: Option<&(dyn mode_a_web::WebProgress + Sync)>,
) -> crate::Result<(usize, VariantResult)> {
    let result = item.work.compute(progress)?;
    Ok((item.index, assemble_single_mapping(item.assembly, result)?))
}

fn write_parallel_results<F>(
    state: &ParallelRunState<'_, impl DataSource + Sync + ?Sized>,
    write_patch: &mut F,
    results: Vec<(usize, VariantResult)>,
    reports: &mut Vec<WebMigrationReportRow>,
) -> crate::Result<()>
where
    F: FnMut(VariantPatchOutput) -> crate::Result<()>,
{
    for (index, result) in results {
        let variant = &state.options.variants[index];
        let directory = variant_directory(
            variant,
            &result.report.target_name,
            index,
            state.options.variants.len(),
        );
        write_patch(VariantPatchOutput {
            patch: result.patch,
            directory,
            suffix: state.suffix.to_owned(),
        })?;
        reports.push(result.report);
    }
    Ok(())
}

async fn migrate_combined_variants<S, F>(
    state: &mut ParallelRunState<'_, S>,
    write_patch: &mut F,
) -> crate::Result<Vec<WebMigrationReportRow>>
where
    S: DataSource + Sync + ?Sized,
    F: FnMut(VariantPatchOutput) -> crate::Result<()>,
{
    let mut reports = Vec::with_capacity(state.options.variants.len());
    for index in 0..state.options.variants.len() {
        let result = migrate_combined_variant(state, index).await?;
        write_parallel_results(state, write_patch, vec![(index, result)], &mut reports)?;
    }
    Ok(reports)
}

async fn migrate_combined_variant<S: DataSource + Sync + ?Sized>(
    state: &mut ParallelRunState<'_, S>,
    index: usize,
) -> crate::Result<VariantResult> {
    let variant = &state.options.variants[index];
    let mut assembly =
        VariantAssembly::new(state.original, variant, state.options.unmatched_unit_policy);
    for start in (0..variant.mappings.len()).step_by(parallel_batch_size()) {
        let end = (start + parallel_batch_size()).min(variant.mappings.len());
        let batch = prepare_mapping_batch(state, index, start..end).await?;
        let results = batch
            .into_par_iter()
            .map(|item| compute_mapping(item, state.progress))
            .collect::<crate::Result<Vec<_>>>()?;
        for (mapping, edges, result) in results {
            assembly.merge(mapping, edges, result)?;
        }
    }
    Ok(assembly.finish())
}

async fn prepare_mapping_batch<'a, S: DataSource + Sync + ?Sized>(
    state: &mut ParallelRunState<'a, S>,
    variant_index: usize,
    range: std::ops::Range<usize>,
) -> crate::Result<Vec<MappingWork<'a>>> {
    let mut batch = Vec::with_capacity(range.len());
    for mapping_index in range {
        let mapping = &state.options.variants[variant_index].mappings[mapping_index];
        let work = state
            .executor
            .prepare_parallel_work(mapping, state.progress)
            .await?;
        batch.push(MappingWork {
            edges: &state.unit_plans[variant_index].mapping_edges[mapping_index],
            mapping,
            work,
        });
    }
    Ok(batch)
}

fn compute_mapping<'a>(
    item: MappingWork<'a>,
    progress: Option<&(dyn mode_a_web::WebProgress + Sync)>,
) -> crate::Result<(
    &'a WebMigrationMapping,
    &'a [UnitMappingEdge],
    mode_a_web::WebTargetResult,
)> {
    Ok((item.mapping, item.edges, item.work.compute(progress)?))
}

fn parallel_batch_size() -> usize {
    rayon::current_num_threads().clamp(1, 4)
}

fn summary_from_reports(reports: Vec<WebMigrationReportRow>) -> WebMigrationSummary {
    WebMigrationSummary {
        migrated_count: reports.len(),
        warning_count: reports.iter().map(|report| report.warnings.len()).sum(),
        reports,
    }
}
