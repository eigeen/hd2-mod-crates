//! Mode A pure-compute core, shared between the native (CLI) and async (web) orchestrators.
//!
//! Inputs are already-loaded `StreamToc` instances; this module performs no I/O.
//! See `mode_a.rs` for the rayon-parallel native driver and `mode_a_web.rs` for the
//! sequential async driver backed by a [`crate::io::DataSource`].

use crate::archive::{StreamToc, TocEntry};
use crate::constants::{MATERIAL_ID, TEX_ID, UNIT_ID};
use crate::migrator::report::MigrationReport;
use crate::padding::{EmptyUnitTemplate, PaddingMode};
use crate::refs;
use crate::unit::authority::{ArmorMappingTable, build_authority_matches};
use crate::unit::geometry::{
    UnitGeometryRemap, build_unit_geometry_remap, format_unit_geometry_issues,
};
use byteorder::{ByteOrder, LittleEndian as LE};
use eyre::WrapErr;
use std::collections::{HashMap, HashSet};

/// Result of computing one target build: a rewritten patch + diagnostic report.
#[derive(Debug, Clone)]
pub struct TargetBuildArtifact {
    pub patch: StreamToc,
    pub report: MigrationReport,
}

/// Inputs common to all targets in a single migration run.
pub struct CommonInputs<'a> {
    pub patch: &'a StreamToc,
    pub source: &'a StreamToc,
    pub source_name: &'a str,
    pub armor_mapping_table: &'a ArmorMappingTable,
    pub empty_unit_template: Option<&'a EmptyUnitTemplate>,
    pub padding_mode: PaddingMode,
    pub incomplete_unit_policy: IncompleteUnitPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncompleteUnitPolicy {
    Fail,
    Drop,
    Keep,
}

/// Compute a migrated patch for one target archive.
///
/// All input archives must already be loaded into memory. `on_stage` is invoked
/// at each major phase with a short label (e.g. "computing remap"); pass `|_| {}`
/// to ignore progress.
pub fn compute_migrated_target<F: Fn(&str)>(
    common: &CommonInputs<'_>,
    target: &StreamToc,
    target_hash: &str,
    target_name: &str,
    on_stage: F,
) -> crate::Result<TargetBuildArtifact> {
    on_stage("computing remap");
    let plan = super::build_remap(common.source, target);
    let mut remap: HashMap<u64, u64> = plan.remap.clone();
    let slot_remap: HashMap<u32, u32> = HashMap::new();
    let mut skipped_file_ids = plan.skipped_file_ids.clone();

    let authority_matches = build_authority_matches(
        common.patch,
        target,
        common.armor_mapping_table,
        common.source_name,
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
    let unit_remap = build_unit_geometry_remap(
        common.patch,
        common.source,
        target,
        &settings,
        &authority_matches,
    )
    .wrap_err_with(|| format!("Unit geometry remap for {target_name}"))?;

    let mut preserved_entries = Vec::new();
    let mut preserved_unit_ids = HashSet::new();
    if !unit_remap.is_complete() {
        let issue_ids = unit_issue_file_ids(&unit_remap);
        match common.incomplete_unit_policy {
            IncompleteUnitPolicy::Fail => {
                eyre::bail!(
                    "[{}] incomplete Unit geometry remap. Unit slots are matched by the \
                     authoritative armor-part table first, then mesh geometry. {}",
                    target_name,
                    format_unit_geometry_issues(&unit_remap, 6)
                );
            }
            IncompleteUnitPolicy::Drop => log_incomplete_unit_remap(target_name, &unit_remap),
            IncompleteUnitPolicy::Keep => {
                log_incomplete_unit_remap(target_name, &unit_remap);
                preserved_entries =
                    super::source_selection::unit_dependency_entries(common.patch, &issue_ids);
                preserved_unit_ids = issue_ids.clone();
            }
        }
        skipped_file_ids.extend(issue_ids);
    }

    for (sid, tid) in &unit_remap.remap {
        remap.insert(*sid, *tid);
    }
    let unit_targets = unit_remap.expanded_remap.clone();

    let (empty_remap, leftover_extras) = assign_empty_unit_placeholders(common.patch, &unit_remap);
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

    on_stage("rewriting entries");

    let source_units: HashMap<u64, &TocEntry> = common
        .source
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
    for e in &common.patch.entries {
        if skipped_file_ids.contains(&e.file_id) {
            log_dropped_patch_entry(target_name, e);
            if !preserved_unit_ids.contains(&e.file_id) {
                skipped_entries += 1;
            }
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
        if let Some(template) = common.empty_unit_template {
            on_stage("padding empty units");
            let extras = crate::padding::pad_patch(
                &mut new_patch,
                &extra_unit_file_ids,
                template,
                common.padding_mode,
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
    merge_preserved_entries(&mut new_patch, &preserved_entries);

    tracing::info!(
        target = %target_name,
        entries = written,
        file_id_remapped = remap.len(),
        slot_id_remapped = slot_remap.len(),
        padded,
        "migrated"
    );

    let warnings = (!preserved_unit_ids.is_empty())
        .then(|| {
            format!(
                "kept {} unrecognized parts in the result without converting them",
                preserved_unit_ids.len()
            )
        })
        .into_iter()
        .collect();
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
        warnings,
    };
    Ok(TargetBuildArtifact {
        patch: new_patch,
        report,
    })
}

/// Build a "same source" artifact: pass the patch through unchanged.
pub fn compute_source_target(
    patch: &StreamToc,
    target_hash: &str,
    target_name: &str,
) -> TargetBuildArtifact {
    tracing::info!(target = %target_name, "prepared source target without remap");
    TargetBuildArtifact {
        patch: patch.clone(),
        report: MigrationReport {
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
        },
    }
}

// ---------- entry-rewrite helpers ---------------------------------------

fn entry_target_file_ids(
    entry: &TocEntry,
    remap: &HashMap<u64, u64>,
    unit_targets: &HashMap<u64, Vec<u64>>,
) -> Vec<u64> {
    if entry.type_id == UNIT_ID
        && let Some(targets) = unit_targets.get(&entry.file_id)
        && !targets.is_empty()
    {
        return targets.clone();
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

fn log_incomplete_unit_remap(target_name: &str, unit_remap: &UnitGeometryRemap) {
    let details = format_unit_geometry_issues(unit_remap, 12);
    tracing::warn!(
        target = %target_name,
        mapped = unit_remap.remap.len(),
        skipped = unit_issue_file_ids(unit_remap).len(),
        details = %details,
        "continuing with incomplete Unit geometry remap"
    );
}

/// Merge exact passthrough resources, preferring their original bytes on key collisions.
pub(crate) fn merge_preserved_entries(patch: &mut StreamToc, entries: &[TocEntry]) {
    let mut positions = patch
        .entries
        .iter()
        .enumerate()
        .map(|(index, entry)| ((entry.type_id, entry.file_id), index))
        .collect::<HashMap<_, _>>();
    for entry in entries {
        let key = (entry.type_id, entry.file_id);
        if let Some(index) = positions.get(&key).copied() {
            patch.entries[index] = entry.clone();
            continue;
        }
        positions.insert(key, patch.entries.len());
        patch.entries.push(entry.clone());
    }
}

fn unit_issue_file_ids(unit_remap: &UnitGeometryRemap) -> HashSet<u64> {
    unit_remap
        .missing
        .iter()
        .chain(unit_remap.ambiguous.iter())
        .map(|i| i.source_file_id)
        .collect()
}
