//! Mode A: derive remap by reading game `data/` archives.
//!
//! 1. Optionally load a BundleIndex (Slim install: packages inside
//!    `bundles.*.nxa`).
//! 2. Auto-detect the source armor archive by FileID overlap.
//! 3. For each target armor:
//!    a. non-Unit ordinal remap via [`super::build_remap`]
//!    b. Unit-FileID remap via the authoritative armor-part table first,
//!    then [`crate::unit::geometry::build_unit_geometry_remap`] for gaps
//!    c. Empty source placeholders are assigned to leftover target Unit slots
//!    d. Patch entries are rewritten through [`crate::refs::rewrite`], with
//!    per-Unit header dependency refs specialized to each source→target
//!    Unit pair
//!    e. Extra target Unit slots padded with the empty-mesh template

mod write;

use super::{MigrateAllOpts, MigrationReport};
use crate::archive::{BundleIndex, StreamToc, TocEntry};
use crate::constants::{MATERIAL_ID, TEX_ID, UNIT_ID};
use crate::refs;
use crate::unit::authority::{build_authority_matches, ArmorMappingTable};
use crate::unit::geometry::{
    build_unit_geometry_remap, format_unit_geometry_issues, UnitGeometryRemap,
};
use byteorder::{ByteOrder, LittleEndian as LE};
use eyre::WrapErr;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
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

    if let Some(p) = progress {
        p.stage(progress_label, "computing remap");
    }
    let plan = super::build_remap(source, &target);
    let mut remap: HashMap<u64, u64> = plan.remap.clone();
    let slot_remap: HashMap<u32, u32> = HashMap::new();
    let mut skipped_file_ids = plan.skipped_file_ids.clone();

    let authority_matches = build_authority_matches(
        patch,
        &target,
        armor_mapping_table,
        source_name,
        target_name,
    );
    if !authority_matches.is_empty() {
        tracing::info!(
            target = %target_name,
            file_ids = authority_matches.len(),
            "applied authoritative Unit part mapping"
        );
    }

    let settings = crate::unit::geometry::GeometryMatchSettings::default();
    let unit_remap =
        build_unit_geometry_remap(patch, source, &target, &settings, &authority_matches)
            .wrap_err_with(|| format!("Unit geometry remap for {target_name}"))?;

    if !unit_remap.is_complete() {
        if !experimental_partial_remap {
            eyre::bail!(
                "[{}] incomplete Unit geometry remap. Unit slots are matched by the \
                 authoritative armor-part table first, then mesh geometry. {}",
                target_name,
                format_unit_geometry_issues(&unit_remap, 6)
            );
        }
        log_experimental_unit_remap(target_name, &unit_remap);
        skipped_file_ids.extend(unit_issue_file_ids(&unit_remap));
    }

    for (sid, tid) in &unit_remap.remap {
        remap.insert(*sid, *tid);
    }
    let unit_targets = unit_remap.expanded_remap.clone();

    // Empty source placeholders → leftover target Unit slots.
    let (empty_remap, leftover_extras) = assign_empty_unit_placeholders(patch, &unit_remap);
    let empty_remap_count = empty_remap.len();
    for (k, v) in &empty_remap {
        remap.insert(*k, *v);
    }
    let extra_unit_file_ids = leftover_extras;
    for k in unit_remap.remap.keys() {
        skipped_file_ids.remove(k);
    }
    for k in empty_remap.keys() {
        skipped_file_ids.remove(k);
    }
    log_unit_geometry_remap(target_name, &unit_remap);
    if empty_remap_count > 0 {
        tracing::info!(
            target = %target_name,
            count = empty_remap_count,
            "mapped empty source Unit placeholder(s)"
        );
    }

    if let Some(p) = progress {
        p.stage(progress_label, "rewriting entries");
    }

    let source_units: HashMap<u64, &TocEntry> = source
        .entries
        .iter()
        .filter(|e| e.type_id == UNIT_ID)
        .map(|e| (e.file_id, e))
        .collect();
    let target_units: HashMap<u64, &TocEntry> = target
        .entries
        .iter()
        .filter(|e| e.type_id == UNIT_ID)
        .map(|e| (e.file_id, e))
        .collect();

    let mut new_patch = StreamToc::default();
    let mut written = 0usize;
    let mut skipped_entries = 0usize;
    let slot_ref = if slot_remap.is_empty() {
        None
    } else {
        Some(&slot_remap)
    };
    for e in &patch.entries {
        if skipped_file_ids.contains(&e.file_id) {
            log_dropped_patch_entry(target_name, e);
            skipped_entries += 1;
            continue;
        }
        for new_file_id in entry_target_file_ids(e, &remap, &unit_targets) {
            let entry_remap =
                entry_specific_remap(e, new_file_id, &remap, &source_units, &target_units);
            let toc_data = refs::rewrite(e.type_id, &e.toc_data, &entry_remap, slot_ref);
            let mut new_entry = TocEntry::new(new_file_id, e.type_id);
            new_entry.toc_data = toc_data;
            new_entry.gpu_data = e.gpu_data.clone();
            new_entry.stream_data = e.stream_data.clone();
            new_patch.entries.push(new_entry);
            written += 1;
        }
    }

    let padded = if !extra_unit_file_ids.is_empty() {
        if let Some(template) = empty_unit_template {
            if let Some(p) = progress {
                p.stage(progress_label, "padding empty units");
            }
            let extras = crate::padding::pad_patch(
                &mut new_patch,
                &extra_unit_file_ids,
                template,
                padding_mode,
                slot_ref,
            );
            extras.len()
        } else {
            tracing::warn!(
                target = %target_name,
                count = extra_unit_file_ids.len(),
                "target has extra Unit slots but no empty-mesh template supplied"
            );
            0
        }
    } else {
        0
    };

    tracing::info!(
        target = %target_name,
        entries = written,
        file_id_remapped = remap.len(),
        slot_id_remapped = slot_remap.len(),
        padded,
        "migrated"
    );

    let report = MigrationReport {
        target_hash: target_hash.to_string(),
        target_name: target_name.to_string(),
        out_path: None,
        file_id_remapped: remap.len(),
        slot_id_remapped: slot_remap.len(),
        padded_units: padded,
        skipped_entries,
        skipped_types: plan.skipped_types.clone(),
        type_counts: plan.type_counts.clone(),
        warnings: Vec::new(),
    };
    Ok(TargetBuild {
        order,
        patch: new_patch,
        report,
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
    tracing::info!(
        target = %target_name,
        "prepared source target without remap"
    );
    let report = MigrationReport {
        target_hash: target_hash.to_string(),
        target_name: target_name.to_string(),
        out_path: None,
        file_id_remapped: patch.entries.len(),
        slot_id_remapped: 0,
        padded_units: 0,
        skipped_entries: 0,
        skipped_types: Vec::new(),
        type_counts: HashMap::new(),
        warnings: Vec::new(),
    };
    Ok(TargetBuild {
        order,
        patch: patch.clone(),
        report,
    })
}

// ---------- entry-rewrite helpers ---------------------------------------

fn entry_target_file_ids(
    entry: &TocEntry,
    remap: &HashMap<u64, u64>,
    unit_targets: &HashMap<u64, Vec<u64>>,
) -> Vec<u64> {
    if entry.type_id == UNIT_ID {
        if let Some(targets) = unit_targets.get(&entry.file_id) {
            if !targets.is_empty() {
                return targets.clone();
            }
        }
    }
    vec![remap.get(&entry.file_id).copied().unwrap_or(entry.file_id)]
}

fn entry_specific_remap(
    entry: &TocEntry,
    target_file_id: u64,
    base_remap: &HashMap<u64, u64>,
    source_units: &HashMap<u64, &TocEntry>,
    target_units: &HashMap<u64, &TocEntry>,
) -> HashMap<u64, u64> {
    if entry.type_id != UNIT_ID || entry.file_id == target_file_id {
        return base_remap.clone();
    }
    let (Some(src), Some(tgt)) = (
        source_units.get(&entry.file_id),
        target_units.get(&target_file_id),
    ) else {
        return base_remap.clone();
    };
    let mut remap = base_remap.clone();
    let source_refs = unit_header_refs(&src.toc_data);
    let target_refs = unit_header_refs(&tgt.toc_data);
    for (s, t) in source_refs.iter().zip(target_refs.iter()) {
        if *s == 0 || *t == 0 {
            continue;
        }
        remap.insert(*s, *t);
    }
    remap
}

fn unit_header_refs(toc_data: &[u8]) -> [u64; 5] {
    if toc_data.len() < 0x28 {
        return [0; 5];
    }
    let mut out = [0u64; 5];
    for i in 0..5 {
        out[i] = LE::read_u64(&toc_data[i * 8..i * 8 + 8]);
    }
    out
}

fn log_dropped_patch_entry(target_name: &str, entry: &TocEntry) {
    let type_label = crate::constants::type_name(entry.type_id).unwrap_or("<unknown>");
    if entry.type_id == UNIT_ID {
        log_unit_dropped_entry(target_name, entry.file_id, type_label);
    } else if entry.type_id == MATERIAL_ID || entry.type_id == TEX_ID {
        log_debug_dropped_entry(target_name, entry.file_id, type_label);
    } else {
        log_warn_dropped_entry(target_name, entry.file_id, type_label);
    }
}

fn log_unit_dropped_entry(target_name: &str, file_id: u64, type_label: &str) {
    tracing::info!(
        target = %target_name,
        file_id,
        type_id = %type_label,
        "dropping entry (target lacks matching Unit slot)"
    );
}

fn log_debug_dropped_entry(target_name: &str, file_id: u64, type_label: &str) {
    tracing::debug!(
        target = %target_name,
        file_id,
        type_id = %type_label,
        "dropping entry (target lacks matching slot)"
    );
}

fn log_warn_dropped_entry(target_name: &str, file_id: u64, type_label: &str) {
    tracing::warn!(
        target = %target_name,
        file_id,
        type_id = %type_label,
        "dropping entry (target lacks matching slot)"
    );
}

// ---------- empty placeholder helpers -----------------------------------

fn assign_empty_unit_placeholders(
    patch: &StreamToc,
    unit_remap: &UnitGeometryRemap,
) -> (HashMap<u64, u64>, Vec<u64>) {
    let mut target_ids = unit_remap.extra_unit_file_ids.clone();
    let mut assignments: HashMap<u64, u64> = HashMap::new();
    for entry in patch.entries.iter().filter(|e| e.type_id == UNIT_ID) {
        if !unit_remap.empty_source_file_ids.contains(&entry.file_id) {
            continue;
        }
        if target_ids.is_empty() {
            break;
        }
        let tid = target_ids.remove(0);
        assignments.insert(entry.file_id, tid);
    }
    (assignments, target_ids)
}

// ---------- diagnostics --------------------------------------------------

fn log_unit_geometry_remap(target_name: &str, unit_remap: &UnitGeometryRemap) {
    tracing::info!(
        target = %target_name,
        file_ids = unit_remap.remap.len(),
        "applied geometry Unit remap"
    );
    let mut entries: Vec<(u64, Vec<u64>)> = unit_remap
        .expanded_remap
        .iter()
        .map(|(k, v)| (*k, v.clone()))
        .collect();
    entries.sort_by_key(|(k, _)| *k);
    for (source_id, target_ids) in entries {
        let score = unit_remap.scores.get(&source_id).copied().unwrap_or(0.0);
        let margin = unit_remap.margins.get(&source_id).copied().unwrap_or(0.0);
        let level = unit_remap
            .match_levels
            .get(&source_id)
            .cloned()
            .unwrap_or_else(|| "geometry".to_string());
        tracing::debug!(
            target = %target_name,
            source = source_id,
            targets = ?target_ids,
            level = %level,
            score = score,
            margin = margin,
            "Unit match"
        );
    }
}

fn log_experimental_unit_remap(target_name: &str, unit_remap: &UnitGeometryRemap) {
    let details = format_unit_geometry_issues(unit_remap, 12);
    tracing::warn!(
        target = %target_name,
        mapped = unit_remap.remap.len(),
        skipped = unit_issue_file_ids(unit_remap).len(),
        details = %details,
        "experimental partial Unit geometry remap"
    );
}

fn unit_issue_file_ids(unit_remap: &UnitGeometryRemap) -> HashSet<u64> {
    unit_remap
        .missing
        .iter()
        .chain(unit_remap.ambiguous.iter())
        .map(|i| i.source_file_id)
        .collect()
}

// ---------- BundleIndex bootstrap ---------------------------------------

fn load_bundle_index_if_present(data_dir: &Path) -> crate::Result<Option<BundleIndex>> {
    let p = data_dir.join("bundles.nxa");
    if !p.exists() {
        return Ok(None);
    }
    let idx = BundleIndex::from_data_dir(data_dir)?;
    Ok(Some(idx))
}

// ---------- authoritative armor mapping ----------------------------------

fn load_armor_mapping_table(path: Option<&Path>) -> crate::Result<ArmorMappingTable> {
    match path {
        Some(path) => ArmorMappingTable::load(path),
        None => ArmorMappingTable::bundled(),
    }
}
