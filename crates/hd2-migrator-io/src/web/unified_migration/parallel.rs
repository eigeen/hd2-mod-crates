use super::prepared::{ParallelMigrationExecutor, PreparedWork};
use super::*;
use rayon::prelude::*;
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};

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
    executor: ParallelMigrationExecutor<'a, S>,
    options: &'a WebUnifiedMigrateOptions,
    original: &'a StreamToc,
    progress: Option<&'a (dyn mode_a_web::WebProgress + Sync)>,
    unit_plans: &'a [unit_plan::VariantUnitPlan],
    unit_behavior: &'a CompiledUnitBehavior,
}

struct ParallelWriteContext<'a> {
    options: &'a WebUnifiedMigrateOptions,
    suffix: &'a str,
}

#[derive(Clone, Copy)]
enum ParallelWorkShape {
    Combined,
    Single,
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
    let unit_behavior = CompiledUnitBehavior::compile(&options.unit_behavior)?;
    let unit_plans = unit_plan::build_variant_plans(&options.variants, &unit_behavior)?;
    let original = parse_patch(&patch_bytes)?;
    let executor = ParallelMigrationExecutor::new(&original, source, options.no_padding).await?;
    let suffix = options
        .patch_suffix
        .as_deref()
        .unwrap_or(super::super::migration::DEFAULT_PATCH_SUFFIX);
    let state = ParallelRunState {
        executor,
        options: &options,
        original: &original,
        progress: callbacks.progress,
        unit_plans: &unit_plans,
        unit_behavior: &unit_behavior,
    };
    let write_context = ParallelWriteContext {
        options: &options,
        suffix,
    };
    let shape = if variants_have_single_mapping(&options) {
        ParallelWorkShape::Single
    } else {
        ParallelWorkShape::Combined
    };
    let reports =
        migrate_variants_pipelined(state, write_context, &mut callbacks.write_patch, shape)?;
    Ok(summary_from_reports(reports))
}

fn variants_have_single_mapping(options: &WebUnifiedMigrateOptions) -> bool {
    options
        .variants
        .iter()
        .all(|variant| variant.mappings.len() == 1)
}

fn migrate_variants_pipelined<S, F>(
    mut state: ParallelRunState<'_, S>,
    write_context: ParallelWriteContext<'_>,
    write_patch: &mut F,
    shape: ParallelWorkShape,
) -> crate::Result<Vec<WebMigrationReportRow>>
where
    S: DataSource + Sync + ?Sized,
    F: FnMut(VariantPatchOutput) -> crate::Result<()>,
{
    let (sender, receiver) = sync_channel(0);
    std::thread::scope(|scope| {
        let producer = scope.spawn(move || produce_results(&mut state, shape, sender));
        let write_result = write_received_results(write_context, write_patch, receiver);
        let producer_result = producer
            .join()
            .map_err(|_| eyre::eyre!("parallel migration producer panicked"))?;
        let reports = write_result?;
        producer_result?;
        Ok(reports)
    })
}

fn produce_results<S: DataSource + Sync + ?Sized>(
    state: &mut ParallelRunState<'_, S>,
    shape: ParallelWorkShape,
    sender: SyncSender<Vec<(usize, VariantResult)>>,
) -> crate::Result<()> {
    match shape {
        ParallelWorkShape::Single => produce_single_variants(state, sender),
        ParallelWorkShape::Combined => produce_combined_variants(state, sender),
    }
}

fn produce_single_variants<S: DataSource + Sync + ?Sized>(
    state: &mut ParallelRunState<'_, S>,
    sender: SyncSender<Vec<(usize, VariantResult)>>,
) -> crate::Result<()> {
    for start in (0..state.options.variants.len()).step_by(parallel_batch_size()) {
        let end = (start + parallel_batch_size()).min(state.options.variants.len());
        let batch = pollster::block_on(prepare_single_batch(state, start..end))?;
        let results = batch
            .into_par_iter()
            .map(|item| compute_single_variant(item, state.progress))
            .collect::<crate::Result<Vec<_>>>()?;
        if sender.send(results).is_err() {
            return Ok(());
        }
    }
    Ok(())
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
                unit_behavior: (*state.unit_behavior).clone(),
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
    context: &ParallelWriteContext<'_>,
    write_patch: &mut F,
    results: Vec<(usize, VariantResult)>,
    reports: &mut Vec<WebMigrationReportRow>,
) -> crate::Result<()>
where
    F: FnMut(VariantPatchOutput) -> crate::Result<()>,
{
    for (index, result) in results {
        let variant = &context.options.variants[index];
        let directory = variant_directory(
            variant,
            &result.report.target_name,
            index,
            context.options.variants.len(),
        );
        write_patch(VariantPatchOutput {
            patch: result.patch,
            directory,
            suffix: context.suffix.to_owned(),
        })?;
        reports.push(result.report);
    }
    Ok(())
}

fn write_received_results<F>(
    context: ParallelWriteContext<'_>,
    write_patch: &mut F,
    receiver: Receiver<Vec<(usize, VariantResult)>>,
) -> crate::Result<Vec<WebMigrationReportRow>>
where
    F: FnMut(VariantPatchOutput) -> crate::Result<()>,
{
    let mut reports = Vec::with_capacity(context.options.variants.len());
    for results in receiver {
        write_parallel_results(&context, write_patch, results, &mut reports)?;
    }
    Ok(reports)
}

fn produce_combined_variants<S>(
    state: &mut ParallelRunState<'_, S>,
    sender: SyncSender<Vec<(usize, VariantResult)>>,
) -> crate::Result<()>
where
    S: DataSource + Sync + ?Sized,
{
    for index in 0..state.options.variants.len() {
        let result = pollster::block_on(migrate_combined_variant(state, index))?;
        if sender.send(vec![(index, result)]).is_err() {
            return Ok(());
        }
    }
    Ok(())
}

async fn migrate_combined_variant<S: DataSource + Sync + ?Sized>(
    state: &mut ParallelRunState<'_, S>,
    index: usize,
) -> crate::Result<VariantResult> {
    let variant = &state.options.variants[index];
    let mut assembly = VariantAssembly::new(
        state.original,
        variant,
        state.options.unmatched_unit_policy,
        (*state.unit_behavior).clone(),
    );
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
