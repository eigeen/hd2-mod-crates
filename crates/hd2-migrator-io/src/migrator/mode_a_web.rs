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

use super::mode_a_common::{self, CommonInputs};
use super::source_selection;
use crate::archive::StreamToc;
use crate::index::ArchiveIndex;
use crate::io::{BundleSlicer, DataSource};
use crate::migrator::report::MigrationReport;
use crate::padding::{self, EmptyUnitTemplate, PaddingMode};
use crate::unit::authority::ArmorMappingTable;
use crate::web::migration::{detect_source_via_authority, unit_file_ids};
use crate::web::migration::{PatchBytes, WebMigrateOptions};
use std::collections::HashMap;

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
    let by_hash: HashMap<String, String> = archives
        .iter()
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

    let source_hash = resolve_source_hash(&patch, options, category, &by_hash)?;
    let source_name = by_hash
        .get(&source_hash)
        .cloned()
        .ok_or_else(|| eyre::eyre!("source {source_hash} not in builtin index"))?;

    let source_archive = load_archive_async(source, bundle.as_ref(), &source_hash).await?;
    let filter = source_selection::filter_patch_to_source_archive_units(&patch, &source_archive);
    let patch = filter.patch;

    let armor_mapping_table = ArmorMappingTable::bundled()?;
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

    let common = CommonInputs {
        patch: &patch,
        source: &source_archive,
        source_name: &source_name,
        armor_mapping_table: &armor_mapping_table,
        empty_unit_template: empty_unit_template.as_ref(),
        padding_mode,
        experimental_partial_remap: options.experimental_partial_remap,
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
        let result = if target_hash == &source_hash {
            let artifact = mode_a_common::compute_source_target(&patch, target_hash, &target_name);
            WebTargetResult {
                target_hash: target_hash.clone(),
                target_name: target_name.clone(),
                patch: artifact.patch,
                report: artifact.report,
            }
        } else {
            let target = load_archive_async(source, bundle.as_ref(), target_hash).await?;
            let stage_callback = |stage: &str| {
                if let Some(p) = progress {
                    p.stage(&target_name, stage);
                }
            };
            let artifact = mode_a_common::compute_migrated_target(
                &common,
                &target,
                target_hash,
                &target_name,
                stage_callback,
            )?;
            WebTargetResult {
                target_hash: target_hash.clone(),
                target_name: target_name.clone(),
                patch: artifact.patch,
                report: artifact.report,
            }
        };
        if let Some(p) = progress {
            p.target_finished(&result.target_name);
        }
        results.push(result);
    }
    Ok(results)
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

async fn read_sidecar<S: DataSource + ?Sized>(
    source: &S,
    path: &str,
) -> crate::Result<Vec<u8>> {
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
