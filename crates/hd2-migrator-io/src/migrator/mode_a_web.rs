//! Mode A (async/web): drive a cross-archive migration through the
//! [`crate::io::DataSource`] abstraction.
//!
//! Mirrors the orchestration in `mode_a.rs` but with these differences:
//! - All archive reads happen via an async `DataSource` (browser-friendly).
//! - Targets are processed sequentially (no rayon).
//! - The bundles.nxa Slim-install index is opened via
//!   [`crate::io::BundleSlicer`] which streams chunks instead of mapping the
//!   multi-GB `bundles.NN.nxa` files into memory.
//!
//! The per-target compute is shared with the native driver via
//! [`crate::migrator::mode_a_common`].
//!
//! Note: there is no rayon dependency in this code path — wasm cannot use
//! threads without COOP/COEP, and per-target latency in the browser is
//! dominated by I/O, not CPU.

use super::helmet::{self, HelmetMigrationInputs};
use super::mode_a_common::{
    self, CommonInputs, IncompleteUnitPolicy, TargetBuildArtifact, merge_preserved_entries,
};
use super::source_selection;
use crate::archive::{self, StreamToc, TocEntry};
use crate::constants::UNIT_ID;
use crate::index::{ArchiveIndex, ArmorEntry};
use crate::io::{BundleSlicer, DataSource};
use crate::migrator::report::MigrationReport;
use crate::padding::{self, EmptyUnitTemplate, PaddingMode};
use crate::unit::authority::ArmorMappingTable;
use crate::unit::helmet_authority::HelmetMappingTable;
use crate::web::migration::{
    PatchBytes, UnmatchedUnitPolicy, WebMigrateOptions, detect_models_via_authority,
    detect_source_via_authority, selectable_archive_entries, unit_file_ids,
};
use std::collections::{HashMap, HashSet};

/// Async progress callback. Mirrors `migrator::ProgressSink` but does not
/// require `Sync` (the wasm impl wraps `js_sys::Function` which is `!Send`).
pub trait WebProgress {
    fn target_started(&self, target_name: &str, target_hash: &str);
    fn stage(&self, target_name: &str, stage: &str);
    fn target_finished(&self, target_name: &str);
}

/// Result for one migrated target. The caller assembles the output ZIP /
/// filesystem layout from these.
pub struct WebTargetResult {
    pub target_hash: String,
    pub target_name: String,
    pub patch: StreamToc,
    pub report: MigrationReport,
    pub(crate) source_unit_ids: HashSet<u64>,
}

/// Run the async cross-archive migration. Returns one [`WebTargetResult`]
/// per requested target, in the requested order.
pub async fn run<S: DataSource + ?Sized>(
    patch_bytes: &PatchBytes,
    options: &WebMigrateOptions,
    source: &S,
    category: &str,
    progress: Option<&dyn WebProgress>,
) -> crate::Result<Vec<WebTargetResult>> {
    let archives = ArchiveIndex::builtin()
        .category(category)
        .ok_or_else(|| eyre::eyre!("category {category:?} not found in builtin index"))?;
    let by_hash: HashMap<String, String> = selectable_archive_entries(category)?
        .into_iter()
        .map(|a| (a.hash.clone(), a.name.clone()))
        .collect();

    let patch = StreamToc::from_buffers(
        &patch_bytes.toc,
        &patch_bytes.gpu,
        &patch_bytes.stream,
        patch_bytes.name.clone(),
    )?;

    let bundle = if source.exists("bundles.nxa").await? {
        tracing::info!("loaded Slim bundles.nxa index (async)");
        Some(BundleSlicer::open(source).await?)
    } else {
        None
    };

    let mapping = CategoryMapping::load(category)?;
    let source_hash = resolve_source_hash(&patch, options, category, &by_hash)?;
    ensure_targets_differ_from_source(&options.target_hashes, &source_hash)?;
    let source_name = by_hash
        .get(&source_hash)
        .cloned()
        .ok_or_else(|| eyre::eyre!("source {source_hash} not in builtin index"))?;

    let source_archive =
        load_armor_source_archive(source, bundle.as_ref(), &source_hash, &mapping).await?;
    let model_detection_warning = detect_unclaimed_model_warning(category, &source_name, &patch)?;
    let mut prepared = prepare_patch(
        patch,
        source_archive.as_ref(),
        &mapping,
        options.unmatched_unit_policy,
    );
    prepared.source_unit_ids = selected_source_unit_ids(
        &prepared.migration,
        source_archive.as_ref(),
        &mapping,
        &source_name,
    )?;
    prepared.model_detection_warning = model_detection_warning;

    let empty_unit_template: Option<EmptyUnitTemplate> = if options.no_padding {
        None
    } else {
        Some(padding::builtin_template())
    };
    let padding_mode = if options.no_padding {
        PaddingMode::Disabled
    } else {
        PaddingMode::Sanitized
    };

    let compute_context = MigrationComputeContext {
        patch: &prepared.migration,
        source: source_archive.as_ref(),
        source_name: &source_name,
        mapping: &mapping,
        empty_unit_template: empty_unit_template.as_ref(),
        padding_mode,
        unmatched_unit_policy: options.unmatched_unit_policy,
    };

    let mut results = Vec::with_capacity(options.target_hashes.len());
    for target_hash in &options.target_hashes {
        let target_name = by_hash
            .get(target_hash)
            .cloned()
            .ok_or_else(|| eyre::eyre!("target {target_hash} not in builtin index"))?;
        if let Some(p) = progress {
            p.target_started(&target_name, target_hash);
            p.stage(&target_name, "loading target");
        }
        let load_context = ArchiveLoadContext {
            source,
            bundle: bundle.as_ref(),
            archives,
        };
        let loaded =
            load_migration_target(&load_context, target_hash, &target_name, &mapping).await?;
        let stage_callback = |stage: &str| {
            if let Some(p) = progress {
                p.stage(&target_name, stage);
            }
        };
        let identity = TargetIdentity {
            hash: &loaded.hash,
            name: &target_name,
        };
        let artifact =
            compute_cross_target(&compute_context, &loaded.archive, &identity, stage_callback)?;
        let resolved_hash = loaded.hash;
        let result = finish_target_result(resolved_hash, target_name, artifact, &prepared);
        if let Some(p) = progress {
            p.target_finished(&result.target_name);
        }
        results.push(result);
    }
    Ok(results)
}

fn ensure_targets_differ_from_source(
    target_hashes: &[String],
    source_hash: &str,
) -> crate::Result<()> {
    if target_hashes.iter().any(|hash| hash == source_hash) {
        eyre::bail!("source archive cannot also be a migration target");
    }
    Ok(())
}

async fn load_armor_source_archive<S: DataSource + ?Sized>(
    source: &S,
    bundle: Option<&BundleSlicer>,
    archive_name: &str,
    mapping: &CategoryMapping,
) -> crate::Result<Option<StreamToc>> {
    match mapping {
        CategoryMapping::Armor(_) => Ok(Some(
            load_archive_async(source, bundle, archive_name).await?,
        )),
        CategoryMapping::Helmet(_) => Ok(None),
    }
}

struct PreparedPatch {
    migration: StreamToc,
    preserved_entries: Vec<TocEntry>,
    dropped_entries: usize,
    preserved_units: usize,
    model_detection_warning: Option<String>,
    source_unit_ids: HashSet<u64>,
}

fn prepare_patch(
    patch: StreamToc,
    source: Option<&StreamToc>,
    mapping: &CategoryMapping,
    policy: UnmatchedUnitPolicy,
) -> PreparedPatch {
    match (mapping, source) {
        (CategoryMapping::Armor(_), Some(source)) => prepare_armor_patch(patch, source, policy),
        _ => PreparedPatch {
            migration: patch,
            preserved_entries: Vec::new(),
            dropped_entries: 0,
            preserved_units: 0,
            model_detection_warning: None,
            source_unit_ids: HashSet::new(),
        },
    }
}

fn prepare_armor_patch(
    patch: StreamToc,
    source: &StreamToc,
    policy: UnmatchedUnitPolicy,
) -> PreparedPatch {
    let filter = source_selection::filter_patch_to_source_archive_units(&patch, source);
    if policy == UnmatchedUnitPolicy::Drop {
        return PreparedPatch {
            migration: filter.patch,
            preserved_entries: Vec::new(),
            dropped_entries: filter.dropped_entries,
            preserved_units: 0,
            model_detection_warning: None,
            source_unit_ids: HashSet::new(),
        };
    }

    let selected_units = unit_file_ids(&filter.patch);
    let foreign_units = unit_file_ids(&patch)
        .difference(&selected_units)
        .copied()
        .collect::<HashSet<_>>();
    PreparedPatch {
        migration: filter.patch,
        preserved_entries: source_selection::unit_dependency_entries(&patch, &foreign_units),
        dropped_entries: 0,
        preserved_units: foreign_units.len(),
        model_detection_warning: None,
        source_unit_ids: HashSet::new(),
    }
}

fn selected_source_unit_ids(
    patch: &StreamToc,
    source: Option<&StreamToc>,
    mapping: &CategoryMapping,
    source_name: &str,
) -> crate::Result<HashSet<u64>> {
    match mapping {
        CategoryMapping::Armor(_) => selected_armor_unit_ids(patch, source),
        CategoryMapping::Helmet(table) => {
            let patch_units = unit_file_ids(patch);
            Ok(table
                .unit_id(source_name)
                .filter(|file_id| patch_units.contains(file_id))
                .into_iter()
                .collect())
        }
    }
}

fn selected_armor_unit_ids(
    patch: &StreamToc,
    source: Option<&StreamToc>,
) -> crate::Result<HashSet<u64>> {
    let source =
        source.ok_or_else(|| eyre::eyre!("armor migration is missing its source archive"))?;
    Ok(unit_file_ids(patch)
        .intersection(&unit_file_ids(source))
        .copied()
        .collect())
}

/// Reports uniquely identifiable model objects left after the selected source.
fn detect_unclaimed_model_warning(
    category: &str,
    source_name: &str,
    patch: &StreamToc,
) -> crate::Result<Option<String>> {
    let mut models = detect_models_via_authority(&unit_file_ids(patch))?;
    models.retain(|model| model.category != category || model.name != source_name);
    if models.is_empty() {
        return Ok(None);
    }
    let candidates = models
        .iter()
        .map(|model| {
            format!(
                "{} {} ({} parts found)",
                model.category, model.name, model.unit_hits
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Some(format!(
        "this patch may also contain: {candidates}. This run only converts {category} {source_name}. Import the original patch again to convert each additional item"
    )))
}

fn finish_target_result(
    target_hash: String,
    target_name: String,
    mut artifact: TargetBuildArtifact,
    prepared: &PreparedPatch,
) -> WebTargetResult {
    merge_preserved_entries(&mut artifact.patch, &prepared.preserved_entries);
    artifact.report.skipped_entries += prepared.dropped_entries;
    if prepared.preserved_units > 0 {
        artifact.report.warnings.push(format!(
            "kept {} parts from other equipment in the result without converting them",
            prepared.preserved_units
        ));
    }
    if let Some(warning) = &prepared.model_detection_warning {
        artifact.report.warnings.push(warning.clone());
    }
    WebTargetResult {
        target_hash,
        target_name,
        patch: artifact.patch,
        report: artifact.report,
        source_unit_ids: prepared.source_unit_ids.clone(),
    }
}

struct ArchiveLoadContext<'a, S: DataSource + ?Sized> {
    source: &'a S,
    bundle: Option<&'a BundleSlicer>,
    archives: &'a [ArmorEntry],
}

struct LoadedTarget {
    hash: String,
    archive: StreamToc,
}

async fn load_migration_target<S: DataSource + ?Sized>(
    context: &ArchiveLoadContext<'_, S>,
    requested_hash: &str,
    target_name: &str,
    mapping: &CategoryMapping,
) -> crate::Result<LoadedTarget> {
    match mapping {
        CategoryMapping::Armor(_) => Ok(LoadedTarget {
            hash: requested_hash.to_string(),
            archive: load_archive_async(context.source, context.bundle, requested_hash).await?,
        }),
        CategoryMapping::Helmet(table) => {
            load_helmet_target_candidate(context, target_name, table).await
        }
    }
}

/// Select the current-game archive candidate that actually owns the mapped Helmet Unit.
async fn load_helmet_target_candidate<S: DataSource + ?Sized>(
    context: &ArchiveLoadContext<'_, S>,
    target_name: &str,
    table: &HelmetMappingTable,
) -> crate::Result<LoadedTarget> {
    let target_unit_id = table
        .unit_id(target_name)
        .ok_or_else(|| eyre::eyre!("helmet {target_name:?} is missing from the bundled mapping"))?;
    for candidate in context
        .archives
        .iter()
        .filter(|archive| archive.name == target_name)
    {
        let archive =
            load_unit_index_async(context.source, context.bundle, &candidate.hash).await?;
        if unit_file_ids(&archive).contains(&target_unit_id) {
            return Ok(LoadedTarget {
                hash: candidate.hash.clone(),
                archive,
            });
        }
    }
    eyre::bail!("no archive candidate for {target_name:?} contains Helmet Unit {target_unit_id}")
}

/// Helmet migration only needs Unit IDs, so avoid loading large GPU/stream sidecars.
async fn load_unit_index_async<S: DataSource + ?Sized>(
    source: &S,
    bundle: Option<&BundleSlicer>,
    archive_name: &str,
) -> crate::Result<StreamToc> {
    let toc = load_toc_bytes_async(source, bundle, archive_name).await?;
    let unit_ids = archive::list_file_ids_from_bytes(&toc)?
        .remove(&UNIT_ID)
        .unwrap_or_default();
    Ok(StreamToc {
        name: archive_name.to_string(),
        entries: unit_ids
            .into_iter()
            .map(|file_id| TocEntry::new(file_id, UNIT_ID))
            .collect(),
        ..Default::default()
    })
}

async fn load_toc_bytes_async<S: DataSource + ?Sized>(
    source: &S,
    bundle: Option<&BundleSlicer>,
    archive_name: &str,
) -> crate::Result<Vec<u8>> {
    if !source.exists(archive_name).await?
        && let Some(bundle) = bundle
        && bundle.has_package(archive_name)
    {
        return bundle.load_package(source, archive_name).await;
    }
    source.read_full(archive_name).await
}

enum CategoryMapping {
    Armor(ArmorMappingTable),
    Helmet(HelmetMappingTable),
}

impl CategoryMapping {
    fn load(category: &str) -> crate::Result<Self> {
        match category {
            "Armor" => Ok(Self::Armor(ArmorMappingTable::bundled()?)),
            "Helmet" => Ok(Self::Helmet(HelmetMappingTable::bundled()?)),
            _ => eyre::bail!("unsupported migration category {category:?}"),
        }
    }
}

struct MigrationComputeContext<'a> {
    patch: &'a StreamToc,
    source: Option<&'a StreamToc>,
    source_name: &'a str,
    mapping: &'a CategoryMapping,
    empty_unit_template: Option<&'a EmptyUnitTemplate>,
    padding_mode: PaddingMode,
    unmatched_unit_policy: UnmatchedUnitPolicy,
}

struct TargetIdentity<'a> {
    hash: &'a str,
    name: &'a str,
}

fn compute_cross_target<F: Fn(&str)>(
    context: &MigrationComputeContext<'_>,
    target: &StreamToc,
    identity: &TargetIdentity<'_>,
    on_stage: F,
) -> crate::Result<TargetBuildArtifact> {
    match context.mapping {
        CategoryMapping::Armor(_) => compute_armor_target(context, target, identity, on_stage),
        CategoryMapping::Helmet(table) => {
            on_stage("rewriting helmet Unit");
            let inputs = HelmetMigrationInputs {
                patch: context.patch,
                source_name: context.source_name,
                mapping_table: table,
                empty_unit_template: context.empty_unit_template,
                padding_mode: context.padding_mode,
                unmatched_unit_policy: context.unmatched_unit_policy,
            };
            helmet::compute_migrated_target(&inputs, target, identity.hash, identity.name)
        }
    }
}

fn compute_armor_target<F: Fn(&str)>(
    context: &MigrationComputeContext<'_>,
    target: &StreamToc,
    identity: &TargetIdentity<'_>,
    on_stage: F,
) -> crate::Result<TargetBuildArtifact> {
    let CategoryMapping::Armor(table) = context.mapping else {
        eyre::bail!("armor migration received a non-armor mapping")
    };
    let source = context
        .source
        .ok_or_else(|| eyre::eyre!("armor migration is missing its source archive"))?;
    let common = CommonInputs {
        patch: context.patch,
        source,
        source_name: context.source_name,
        armor_mapping_table: table,
        empty_unit_template: context.empty_unit_template,
        padding_mode: context.padding_mode,
        incomplete_unit_policy: match context.unmatched_unit_policy {
            UnmatchedUnitPolicy::Drop => IncompleteUnitPolicy::Drop,
            UnmatchedUnitPolicy::Keep => IncompleteUnitPolicy::Keep,
        },
    };
    mode_a_common::compute_migrated_target(&common, target, identity.hash, identity.name, on_stage)
}

/// Load one archive's three files (`<name>`, `.gpu_resources`, `.stream`)
/// either directly from `source` (legacy install) or via the [`BundleSlicer`]
/// (Slim install). Falls back to direct file reads when the archive exists on
/// disk as standalone files even if a BundleSlicer is also present.
async fn load_archive_async<S: DataSource + ?Sized>(
    source: &S,
    bundle: Option<&BundleSlicer>,
    archive_name: &str,
) -> crate::Result<StreamToc> {
    let exists_on_disk = source.exists(archive_name).await?;
    let (toc, gpu, stream) = if !exists_on_disk
        && let Some(b) = bundle
        && b.has_package(archive_name)
    {
        b.load_triple(source, archive_name).await?
    } else {
        let toc = source.read_full(archive_name).await?;
        let gpu = read_sidecar(source, &format!("{archive_name}.gpu_resources")).await?;
        let stream = read_sidecar(source, &format!("{archive_name}.stream")).await?;
        (toc, gpu, stream)
    };
    StreamToc::from_buffers(&toc, &gpu, &stream, archive_name.to_string())
}

async fn read_sidecar<S: DataSource + ?Sized>(source: &S, path: &str) -> crate::Result<Vec<u8>> {
    if source.exists(path).await? {
        source.read_full(path).await
    } else {
        Ok(Vec::new())
    }
}

fn resolve_source_hash(
    patch: &StreamToc,
    options: &WebMigrateOptions,
    category: &str,
    by_hash: &HashMap<String, String>,
) -> crate::Result<String> {
    if let Some(hash) = options.source_hash.as_deref() {
        if !by_hash.contains_key(hash) {
            eyre::bail!("archive {hash} not found in builtin index");
        }
        return Ok(hash.to_string());
    }
    let unit_ids = unit_file_ids(patch);
    detect_source_via_authority(category, &unit_ids)
        .map(|option| option.hash)
        .ok_or_else(|| {
            eyre::eyre!("could not auto-detect source archive from authoritative mapping")
        })
}

#[cfg(test)]
mod same_source_tests {
    use super::*;

    #[test]
    fn rejects_source_archive_in_targets() {
        assert!(ensure_targets_differ_from_source(&["source".to_string()], "source").is_err());
    }
}
