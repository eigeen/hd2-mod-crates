//! Mode A (native): derive remap by reading game `data/` archives from disk.
//!
//! 1. Optionally load a BundleIndex (Slim install: packages inside
//!    `bundles.*.nxa`).
//! 2. Auto-detect the source armor archive by FileID overlap.
//! 3. For each target armor (rayon-parallel):
//!    - Load target archive bytes from disk
//!    - Delegate to [`super::mode_a_common::compute_migrated_target`] for the
//!      pure computation (remap, authority, geometry, padding, rewrite)
//!
//! The pure logic is shared with the web/async driver in `mode_a_web.rs`.

mod write;

use super::mode_a_common::{self, CommonInputs, IncompleteUnitPolicy};
use super::{MigrateAllOpts, MigrationReport};
use crate::archive::{BundleIndex, StreamToc};
use crate::unit::authority::ArmorMappingTable;
use eyre::WrapErr;
use rayon::prelude::*;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
struct TargetBuild {
    order: usize,
    patch: StreamToc,
    report: MigrationReport,
}

pub(super) fn run(opts: MigrateAllOpts) -> crate::Result<Vec<MigrationReport>> {
    std::fs::create_dir_all(opts.out_dir)
        .wrap_err_with(|| format!("create out_dir {}", opts.out_dir.display()))?;

    // Optional Slim BundleIndex
    let bundle_index = load_bundle_index_if_present(opts.data_dir)?;
    if bundle_index.is_some() {
        tracing::info!("loaded Slim bundles.nxa index");
    }

    let archives = opts
        .archive_index
        .category(opts.category)
        .ok_or_else(|| eyre::eyre!("category {:?} not found in archive index", opts.category))?;
    let armor_list: Vec<(String, String)> = archives
        .iter()
        .map(|a| (a.hash.clone(), a.name.clone()))
        .collect();
    let by_hash: HashMap<String, String> = armor_list.iter().cloned().collect();

    tracing::info!(path = %opts.patch_path.display(), "loading patch");
    let patch = StreamToc::from_files(opts.patch_path)?;
    tracing::info!(entries = patch.entries.len(), "patch loaded");

    let bundle_index_ref = bundle_index.as_ref();

    let (source_hash, source_name) = match opts.source_hash {
        Some(h) => {
            let name = by_hash.get(h).cloned().ok_or_else(|| {
                eyre::eyre!("--source {h} not found in category {:?}", opts.category)
            })?;
            (h.to_string(), name)
        }
        None => {
            let detected =
                super::detect_source_archive(&patch, opts.data_dir, &armor_list, bundle_index_ref)
                    .ok_or_else(|| {
                        eyre::eyre!(
                    "could not auto-detect source archive — pass --source <hash> explicitly"
                )
                    })?;
            tracing::info!(
                hash = %detected.hash,
                name = %detected.name,
                unit_hits = detected.unit_hits,
                "source archive auto-detected"
            );
            (detected.hash, detected.name)
        }
    };

    let source_path = opts.data_dir.join(&source_hash);
    let source = StreamToc::from_files_with_bundle(&source_path, bundle_index_ref)?;
    tracing::info!(entries = source.entries.len(), "source loaded");
    let filter = super::filter_patch_to_source_archive_units(&patch, &source);
    log_patch_source_filter(&source_name, &filter);
    let patch = filter.patch;

    let armor_mapping_table = load_armor_mapping_table(opts.armor_mapping_json)?;

    let targets: Vec<(String, String)> = match opts.target_hashes {
        Some(filter) => resolve_target_filters(filter, &armor_list, &by_hash),
        None => armor_list
            .into_iter()
            .filter(|(h, n)| {
                h == &source_hash || !crate::target_exclusions::is_default_excluded_target(h, n)
            })
            .collect(),
    };

    let source = Arc::new(source);
    let patch = Arc::new(patch);
    let bundle_arc = bundle_index.map(Arc::new);
    let builds: Mutex<Vec<TargetBuild>> = Mutex::new(Vec::new());

    targets
        .par_iter()
        .enumerate()
        .for_each(|(order, (thash, tname))| {
            let progress_label = progress_label(tname, thash);
            if let Some(p) = opts.progress {
                p.target_started(&progress_label);
                p.stage(&progress_label, "loading target");
            }
            let res = if thash == &source_hash {
                build_source_target(&patch, order, thash, tname, &progress_label, opts.progress)
            } else {
                build_migrated_target(
                    &patch,
                    &source,
                    opts.data_dir,
                    bundle_arc.as_deref(),
                    order,
                    thash,
                    tname,
                    &progress_label,
                    opts.empty_unit_template,
                    opts.padding_mode,
                    &armor_mapping_table,
                    &source_name,
                    opts.experimental_partial_remap,
                    opts.progress,
                )
            };
            if let Some(p) = opts.progress {
                p.target_finished(&progress_label);
            }
            match res {
                Ok(build) => builds.lock().expect("lock").push(build),
                Err(e) => tracing::error!(target = %tname, error = %e, "migration failed"),
            }
        });

    let mut builds = builds.into_inner().expect("lock");
    builds.sort_by_key(|build| build.order);
    let mut out = write::write_grouped_builds(builds, opts.out_dir, opts.patch_suffix)?;
    out.sort_by(|a, b| a.target_name.cmp(&b.target_name));
    let _ = source_name;
    Ok(out)
}

fn progress_label(target_name: &str, target_hash: &str) -> String {
    format!("{target_name} [{target_hash}]")
}

fn resolve_target_filters(
    filters: &[String],
    armor_list: &[(String, String)],
    by_hash: &HashMap<String, String>,
) -> Vec<(String, String)> {
    let mut targets = Vec::new();
    for filter in filters {
        if let Some(name) = by_hash.get(filter) {
            targets.push((filter.clone(), name.clone()));
            continue;
        }
        targets.extend(
            armor_list
                .iter()
                .filter(|(_, name)| target_name_matches(name, filter))
                .cloned(),
        );
    }
    targets
}

fn target_name_matches(name: &str, filter: &str) -> bool {
    name == filter || name.eq_ignore_ascii_case(filter)
}

fn log_patch_source_filter(source_name: &str, filter: &super::source_selection::PatchSourceFilter) {
    if filter.dropped_entries == 0 {
        tracing::info!(
            source = %source_name,
            units = filter.kept_units,
            "patch already matches source archive Unit set"
        );
        return;
    }
    tracing::warn!(
        source = %source_name,
        kept_units = filter.kept_units,
        dropped_units = filter.dropped_units,
        dropped_entries = filter.dropped_entries,
        "filtered patch to selected source archive Unit set"
    );
}

#[allow(clippy::too_many_arguments)]
fn build_migrated_target(
    patch: &StreamToc,
    source: &StreamToc,
    data_dir: &Path,
    bundle_index: Option<&BundleIndex>,
    order: usize,
    target_hash: &str,
    target_name: &str,
    progress_label: &str,
    empty_unit_template: Option<&crate::padding::EmptyUnitTemplate>,
    padding_mode: crate::padding::PaddingMode,
    armor_mapping_table: &ArmorMappingTable,
    source_name: &str,
    experimental_partial_remap: bool,
    progress: Option<&dyn super::ProgressSink>,
) -> crate::Result<TargetBuild> {
    let target_path = data_dir.join(target_hash);
    if let Some(p) = progress {
        p.stage(progress_label, "reading target archive");
    }
    let target = StreamToc::from_files_with_bundle(&target_path, bundle_index)?;

    let common = CommonInputs {
        patch,
        source,
        source_name,
        armor_mapping_table,
        empty_unit_template,
        padding_mode,
        incomplete_unit_policy: if experimental_partial_remap {
            IncompleteUnitPolicy::Drop
        } else {
            IncompleteUnitPolicy::Fail
        },
    };
    let artifact = mode_a_common::compute_migrated_target(
        &common,
        &target,
        target_hash,
        target_name,
        |stage| {
            if let Some(p) = progress {
                p.stage(progress_label, stage);
            }
        },
    )?;
    Ok(TargetBuild {
        order,
        patch: artifact.patch,
        report: artifact.report,
    })
}

fn build_source_target(
    patch: &StreamToc,
    order: usize,
    target_hash: &str,
    target_name: &str,
    progress_label: &str,
    progress: Option<&dyn super::ProgressSink>,
) -> crate::Result<TargetBuild> {
    if let Some(p) = progress {
        p.stage(progress_label, "copying source patch");
    }
    let artifact = mode_a_common::compute_source_target(patch, target_hash, target_name);
    Ok(TargetBuild {
        order,
        patch: artifact.patch,
        report: artifact.report,
    })
}

fn load_bundle_index_if_present(data_dir: &Path) -> crate::Result<Option<BundleIndex>> {
    let p = data_dir.join("bundles.nxa");
    if !p.exists() {
        return Ok(None);
    }
    let idx = BundleIndex::from_data_dir(data_dir)?;
    Ok(Some(idx))
}

fn load_armor_mapping_table(path: Option<&Path>) -> crate::Result<ArmorMappingTable> {
    match path {
        Some(path) => ArmorMappingTable::load(path),
        None => ArmorMappingTable::bundled(),
    }
}
