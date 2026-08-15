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
use std::sync::Arc;

/// Async progress callback. Mirrors `migrator::ProgressSink` but does not
/// require `Sync` (the wasm impl wraps `js_sys::Function` which is `!Send`).
pub trait WebProgress {
    fn target_started(&self, target_name: &str, target_hash: &str) -> crate::Result<()>;
    fn stage(&self, target_name: &str, stage: &str) -> crate::Result<()>;
    fn target_finished(&self, target_name: &str) -> crate::Result<()>;
}

/// Result for one migrated target. The caller assembles the output ZIP /
/// filesystem layout from these.
pub struct WebTargetResult {
    pub target_hash: String,
    pub target_name: String,
    pub patch: StreamToc,
    pub report: MigrationReport,
    pub(crate) source_unit_ids: HashSet<u64>,
    pub(crate) unit_mappings: Vec<(u64, u64)>,
}

pub(crate) struct MigrationArchiveCache {
    bundle: Option<BundleSlicer>,
    source_archives: HashMap<String, Arc<StreamToc>>,
    unit_indexes: HashMap<String, Arc<StreamToc>>,
}

impl MigrationArchiveCache {
    pub(crate) async fn open<S: DataSource + ?Sized>(source: &S) -> crate::Result<Self> {
        let bundle = if source.exists("bundles.nxa").await? {
            Some(BundleSlicer::open(source).await?)
        } else {
            None
        };
        Ok(Self {
            bundle,
            source_archives: HashMap::new(),
            unit_indexes: HashMap::new(),
        })
    }

    async fn load_source_archive<S: DataSource + ?Sized>(
        &mut self,
        source: &S,
        hash: &str,
    ) -> crate::Result<Arc<StreamToc>> {
        if let Some(archive) = self.source_archives.get(hash) {
            return Ok(Arc::clone(archive));
        }
        let archive = Arc::new(load_archive_async(source, self.bundle.as_ref(), hash).await?);
        self.source_archives
            .insert(hash.to_owned(), Arc::clone(&archive));
        Ok(archive)
    }

    async fn load_target_archive<S: DataSource + ?Sized>(
        &self,
        source: &S,
        hash: &str,
    ) -> crate::Result<Arc<StreamToc>> {
        load_archive_async(source, self.bundle.as_ref(), hash)
            .await
            .map(Arc::new)
    }

    async fn load_unit_index<S: DataSource + ?Sized>(
        &mut self,
        source: &S,
        hash: &str,
    ) -> crate::Result<Arc<StreamToc>> {
        if let Some(archive) = self.unit_indexes.get(hash) {
            return Ok(Arc::clone(archive));
        }
        let archive = Arc::new(load_unit_index_async(source, self.bundle.as_ref(), hash).await?);
        self.unit_indexes
            .insert(hash.to_owned(), Arc::clone(&archive));
        Ok(archive)
    }
}

pub(crate) struct PreparedMigration {
    archives: &'static [ArmorEntry],
    by_hash: HashMap<String, String>,
    empty_unit_template: Option<EmptyUnitTemplate>,
    mapping: CategoryMapping,
    padding_mode: PaddingMode,
    prepared: PreparedPatch,
    source_archive: Option<Arc<StreamToc>>,
    source_hash: String,
    source_name: String,
    unmatched_unit_policy: UnmatchedUnitPolicy,
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
    let patch = StreamToc::from_buffers(
        &patch_bytes.toc,
        &patch_bytes.gpu,
        &patch_bytes.stream,
        patch_bytes.name.clone(),
    )?;
    let mut cache = MigrationArchiveCache::open(source).await?;
    let source_hash = resolve_source_hash_for_options(&patch, options, category)?;
    let prepared = PreparedMigration::new(
        &patch,
        source,
        &mut cache,
        PreparedMigrationOptions {
            category,
            no_padding: options.no_padding,
            source_hash: &source_hash,
            unmatched_unit_policy: options.unmatched_unit_policy,
        },
    )
    .await?;

    let mut results = Vec::with_capacity(options.target_hashes.len());
    for target_hash in &options.target_hashes {
        results.push(
            prepared
                .migrate_target(source, &mut cache, target_hash, progress)
                .await?,
        );
    }
    Ok(results)
}

pub(crate) struct PreparedMigrationOptions<'a> {
    pub(crate) category: &'a str,
    pub(crate) no_padding: bool,
    pub(crate) source_hash: &'a str,
    pub(crate) unmatched_unit_policy: UnmatchedUnitPolicy,
}

impl PreparedMigration {
    pub(crate) async fn new<S: DataSource + ?Sized>(
        patch: &StreamToc,
        source: &S,
        cache: &mut MigrationArchiveCache,
        options: PreparedMigrationOptions<'_>,
    ) -> crate::Result<Self> {
        let (archives, by_hash) = migration_catalog(options.category)?;
        let mapping = CategoryMapping::load(options.category)?;
        let source_name = required_archive_name(&by_hash, options.source_hash, "source")?;
        let source_archive =
            cached_source_archive(cache, source, options.source_hash, &mapping).await?;
        let prepared = prepare_source_patch(
            patch,
            SourcePatchContext {
                category: options.category,
                mapping: &mapping,
                source: source_archive.as_deref(),
                source_name: &source_name,
                unmatched_unit_policy: options.unmatched_unit_policy,
            },
        )?;
        let (empty_unit_template, padding_mode) = migration_padding(options.no_padding);
        Ok(Self {
            archives,
            by_hash,
            empty_unit_template,
            mapping,
            padding_mode,
            prepared,
            source_archive,
            source_hash: options.source_hash.to_owned(),
            source_name,
            unmatched_unit_policy: options.unmatched_unit_policy,
        })
    }

    pub(crate) async fn migrate_target<S: DataSource + ?Sized>(
        &self,
        source: &S,
        cache: &mut MigrationArchiveCache,
        target_hash: &str,
        progress: Option<&dyn WebProgress>,
    ) -> crate::Result<WebTargetResult> {
        if target_hash == self.source_hash {
            eyre::bail!("source archive cannot also be a migration target");
        }
        let target_name = required_archive_name(&self.by_hash, target_hash, "target")?;
        notify_target_start(progress, &target_name, target_hash)?;
        let loaded = self
            .load_target(source, cache, target_hash, &target_name)
            .await?;
        let result = self.compute_target(loaded, target_name, progress)?;
        if let Some(progress) = progress {
            progress.target_finished(&result.target_name)?;
        }
        Ok(result)
    }

    async fn load_target<S: DataSource + ?Sized>(
        &self,
        source: &S,
        cache: &mut MigrationArchiveCache,
        target_hash: &str,
        target_name: &str,
    ) -> crate::Result<LoadedTarget> {
        match &self.mapping {
            CategoryMapping::Armor(_) => Ok(LoadedTarget {
                hash: target_hash.to_owned(),
                archive: cache.load_target_archive(source, target_hash).await?,
            }),
            CategoryMapping::Helmet(table) => {
                load_cached_helmet_target(cache, source, self.archives, target_name, table).await
            }
        }
    }

    fn compute_target(
        &self,
        loaded: LoadedTarget,
        target_name: String,
        progress: Option<&dyn WebProgress>,
    ) -> crate::Result<WebTargetResult> {
        let context = self.compute_context();
        let stage = |value: &str| notify_stage(progress, &target_name, value);
        let identity = TargetIdentity {
            hash: &loaded.hash,
            name: &target_name,
        };
        let artifact = compute_cross_target(&context, &loaded.archive, &identity, stage)?;
        Ok(finish_target_result(
            loaded.hash,
            target_name,
            artifact,
            &self.prepared,
        ))
    }

    fn compute_context(&self) -> MigrationComputeContext<'_> {
        MigrationComputeContext {
            patch: &self.prepared.migration,
            source: self.source_archive.as_deref(),
            source_name: &self.source_name,
            mapping: &self.mapping,
            empty_unit_template: self.empty_unit_template.as_ref(),
            padding_mode: self.padding_mode,
            unmatched_unit_policy: self.unmatched_unit_policy,
        }
    }
}

fn migration_catalog(
    category: &str,
) -> crate::Result<(&'static [ArmorEntry], HashMap<String, String>)> {
    let archives = ArchiveIndex::builtin()
        .category(category)
        .ok_or_else(|| eyre::eyre!("category {category:?} not found in builtin index"))?;
    let by_hash = selectable_archive_entries(category)?
        .into_iter()
        .map(|archive| (archive.hash.clone(), archive.name.clone()))
        .collect();
    Ok((archives, by_hash))
}

fn required_archive_name(
    by_hash: &HashMap<String, String>,
    hash: &str,
    role: &str,
) -> crate::Result<String> {
    by_hash
        .get(hash)
        .cloned()
        .ok_or_else(|| eyre::eyre!("{role} {hash} not in builtin index"))
}

async fn cached_source_archive<S: DataSource + ?Sized>(
    cache: &mut MigrationArchiveCache,
    source: &S,
    hash: &str,
    mapping: &CategoryMapping,
) -> crate::Result<Option<Arc<StreamToc>>> {
    match mapping {
        CategoryMapping::Armor(_) => cache.load_source_archive(source, hash).await.map(Some),
        CategoryMapping::Helmet(_) => Ok(None),
    }
}

struct SourcePatchContext<'a> {
    category: &'a str,
    mapping: &'a CategoryMapping,
    source: Option<&'a StreamToc>,
    source_name: &'a str,
    unmatched_unit_policy: UnmatchedUnitPolicy,
}

fn prepare_source_patch(
    patch: &StreamToc,
    context: SourcePatchContext<'_>,
) -> crate::Result<PreparedPatch> {
    let mut prepared = prepare_patch(
        patch,
        context.source,
        context.mapping,
        context.unmatched_unit_policy,
    );
    prepared.source_unit_ids = selected_source_unit_ids(
        &prepared.migration,
        context.source,
        context.mapping,
        context.source_name,
    )?;
    prepared.model_detection_warning =
        detect_unclaimed_model_warning(context.category, context.source_name, patch)?;
    Ok(prepared)
}

fn migration_padding(no_padding: bool) -> (Option<EmptyUnitTemplate>, PaddingMode) {
    if no_padding {
        (None, PaddingMode::Disabled)
    } else {
        (Some(padding::builtin_template()), PaddingMode::Sanitized)
    }
}

fn notify_target_start(
    progress: Option<&dyn WebProgress>,
    target_name: &str,
    target_hash: &str,
) -> crate::Result<()> {
    if let Some(progress) = progress {
        progress.target_started(target_name, target_hash)?;
        progress.stage(target_name, "loading target")?;
    }
    Ok(())
}

fn notify_stage(
    progress: Option<&dyn WebProgress>,
    target_name: &str,
    stage: &str,
) -> crate::Result<()> {
    match progress {
        Some(progress) => progress.stage(target_name, stage),
        None => Ok(()),
    }
}

fn resolve_source_hash_for_options(
    patch: &StreamToc,
    options: &WebMigrateOptions,
    category: &str,
) -> crate::Result<String> {
    let (_, by_hash) = migration_catalog(category)?;
    resolve_source_hash(patch, options, category, &by_hash)
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
    patch: &StreamToc,
    source: Option<&StreamToc>,
    mapping: &CategoryMapping,
    policy: UnmatchedUnitPolicy,
) -> PreparedPatch {
    match (mapping, source) {
        (CategoryMapping::Armor(_), Some(source)) => prepare_armor_patch(patch, source, policy),
        _ => PreparedPatch {
            migration: patch.clone(),
            preserved_entries: Vec::new(),
            dropped_entries: 0,
            preserved_units: 0,
            model_detection_warning: None,
            source_unit_ids: HashSet::new(),
        },
    }
}

fn prepare_armor_patch(
    patch: &StreamToc,
    source: &StreamToc,
    policy: UnmatchedUnitPolicy,
) -> PreparedPatch {
    let filter = source_selection::filter_patch_to_source_archive_units(patch, source);
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
    let foreign_units = unit_file_ids(patch)
        .difference(&selected_units)
        .copied()
        .collect::<HashSet<_>>();
    PreparedPatch {
        migration: filter.patch,
        preserved_entries: source_selection::unit_dependency_entries(patch, &foreign_units),
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
        unit_mappings: artifact.unit_mappings,
    }
}

struct LoadedTarget {
    hash: String,
    archive: Arc<StreamToc>,
}

async fn load_cached_helmet_target<S: DataSource + ?Sized>(
    cache: &mut MigrationArchiveCache,
    source: &S,
    archives: &[ArmorEntry],
    target_name: &str,
    table: &HelmetMappingTable,
) -> crate::Result<LoadedTarget> {
    let target_unit_id = table
        .unit_id(target_name)
        .ok_or_else(|| eyre::eyre!("helmet {target_name:?} is missing from the bundled mapping"))?;
    for candidate in archives
        .iter()
        .filter(|archive| archive.name == target_name)
    {
        let archive = cache.load_unit_index(source, &candidate.hash).await?;
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

fn compute_cross_target<F: Fn(&str) -> crate::Result<()>>(
    context: &MigrationComputeContext<'_>,
    target: &StreamToc,
    identity: &TargetIdentity<'_>,
    on_stage: F,
) -> crate::Result<TargetBuildArtifact> {
    match context.mapping {
        CategoryMapping::Armor(_) => compute_armor_target(context, target, identity, on_stage),
        CategoryMapping::Helmet(table) => {
            on_stage("rewriting helmet Unit")?;
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

fn compute_armor_target<F: Fn(&str) -> crate::Result<()>>(
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
mod archive_cache_tests {
    use super::*;
    use crate::io::IoFuture;
    use std::cell::Cell;

    struct CountingSource {
        reads: Cell<usize>,
        toc: Vec<u8>,
    }

    impl DataSource for CountingSource {
        fn read_full<'a>(&'a self, _path: &'a str) -> IoFuture<'a, Vec<u8>> {
            self.reads.set(self.reads.get() + 1);
            Box::pin(async { Ok(self.toc.clone()) })
        }

        fn read_range<'a>(
            &'a self,
            _path: &'a str,
            _offset: u64,
            _len: u64,
        ) -> IoFuture<'a, Vec<u8>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn exists<'a>(&'a self, _path: &'a str) -> IoFuture<'a, bool> {
            Box::pin(async { Ok(false) })
        }

        fn list_bundle_chunks<'a>(&'a self) -> IoFuture<'a, Vec<String>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn list_packages<'a>(&'a self) -> IoFuture<'a, Vec<String>> {
            Box::pin(async { Ok(Vec::new()) })
        }
    }

    #[test]
    fn reuses_source_archives_without_retaining_unique_targets() {
        let (toc, _, _) = StreamToc::default().serialize();
        let source = CountingSource {
            reads: Cell::new(0),
            toc,
        };
        let mut cache = pollster::block_on(MigrationArchiveCache::open(&source)).expect("cache");

        let first =
            pollster::block_on(cache.load_source_archive(&source, "source")).expect("source");
        let second = pollster::block_on(cache.load_source_archive(&source, "source"))
            .expect("cached source");
        pollster::block_on(cache.load_target_archive(&source, "target")).expect("first target");
        pollster::block_on(cache.load_target_archive(&source, "target")).expect("second target");

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(source.reads.get(), 3);
    }
}
