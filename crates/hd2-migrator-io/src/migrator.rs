//! Top-level migration orchestration.
//!
//! Ports `mod_armor_migrator/migrator.py`'s `migrate_all` and `migrate_one`.
//! Parallelism: per-target via rayon `par_iter` inside Mode A.

pub mod mode_a;
pub mod mode_a_common;
pub mod mode_a_web;
pub mod report;
pub(crate) mod source_selection;

pub use report::MigrationReport;
pub(crate) use source_selection::{detect_source_archive, filter_patch_to_source_archive_units};

use crate::archive::{StreamToc, TocEntry};
use crate::constants::{type_name, MATERIAL_ID, TEX_ID, UNIT_ID};
use crate::index::ArchiveIndex;
use crate::padding::{EmptyUnitTemplate, PaddingMode};
use crate::refs;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub trait ProgressSink: Sync {
    fn target_started(&self, name: &str);
    fn stage(&self, name: &str, stage: &str);
    fn target_finished(&self, name: &str);
}

pub struct MigrateAllOpts<'a> {
    pub patch_path: &'a Path,
    pub data_dir: &'a Path,
    pub out_dir: &'a Path,
    pub archive_index: &'a ArchiveIndex,
    pub source_hash: Option<&'a str>,
    pub target_hashes: Option<&'a [String]>,
    pub category: &'a str,
    pub patch_suffix: &'a str,
    pub empty_unit_template: Option<&'a EmptyUnitTemplate>,
    pub padding_mode: PaddingMode,
    pub armor_mapping_json: Option<&'a Path>,
    pub experimental_partial_remap: bool,
    pub progress: Option<&'a dyn ProgressSink>,
}

pub fn migrate_all(opts: MigrateAllOpts) -> crate::Result<Vec<MigrationReport>> {
    mode_a::run(opts)
}

// ---------- shared helpers (also used by Mode A) ----------

pub(crate) fn safe_filename(name: &str) -> String {
    name.chars()
        .map(|c| if "<>:\"/\\|?*".contains(c) { '_' } else { c })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .to_string()
}

fn log_type_count_mismatch(type_id: u64, source_count: usize, target_count: usize) {
    if source_count == target_count {
        return;
    }
    let type_label = type_name(type_id).unwrap_or("<unknown>");
    if type_id == UNIT_ID {
        log_unit_count_mismatch(type_label, source_count, target_count);
    } else if type_id == MATERIAL_ID || type_id == TEX_ID {
        log_debug_count_mismatch(type_label, source_count, target_count);
    } else {
        log_warn_count_mismatch(type_label, source_count, target_count);
    }
}

fn log_unit_count_mismatch(type_label: &str, source_count: usize, target_count: usize) {
    tracing::info!(
        type_id = %type_label,
        source = source_count,
        target = target_count,
        "Unit count mismatch (partial geometry remap)"
    );
}

fn log_debug_count_mismatch(type_label: &str, source_count: usize, target_count: usize) {
    tracing::debug!(
        type_id = %type_label,
        source = source_count,
        target = target_count,
        "type count mismatch (partial ordinal remap)"
    );
}

fn log_warn_count_mismatch(type_label: &str, source_count: usize, target_count: usize) {
    tracing::warn!(
        type_id = %type_label,
        source = source_count,
        target = target_count,
        "type count mismatch (partial ordinal remap)"
    );
}

/// Build a FileID remap from source -> target archive entries.
///
/// Unit entries are intentionally excluded because their archive order is not
/// a stable slot identity. Automatic Unit mapping is done later by geometry.
pub(crate) fn build_remap(source: &StreamToc, target: &StreamToc) -> RemapPlan {
    let src_by_type = type_order(&source.entries);
    let tgt_by_type = type_order(&target.entries);
    let mut remap: HashMap<u64, u64> = HashMap::new();
    let mut skipped_types: Vec<u64> = Vec::new();
    let mut counts: HashMap<u64, (usize, usize)> = HashMap::new();
    let mut skipped_file_ids: HashSet<u64> = HashSet::new();

    let src_entries_by_type = entries_by_type(&source.entries);
    let tgt_entries_by_type = entries_by_type(&target.entries);

    for tid in &src_by_type {
        let src_entries: &[&TocEntry] = src_entries_by_type
            .get(tid)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let tgt_entries: &[&TocEntry] = tgt_entries_by_type
            .get(tid)
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        counts.insert(*tid, (src_entries.len(), tgt_entries.len()));
        if tgt_entries.is_empty() {
            skipped_types.push(*tid);
            for e in src_entries {
                skipped_file_ids.insert(e.file_id);
            }
            tracing::debug!(
                type_id = %type_name(*tid).unwrap_or("<unknown>"),
                "skip type: not present in target"
            );
            continue;
        }
        log_type_count_mismatch(*tid, src_entries.len(), tgt_entries.len());
        if *tid == UNIT_ID {
            if src_entries.len() > tgt_entries.len() {
                skipped_types.push(*tid);
                for e in src_entries.iter() {
                    skipped_file_ids.insert(e.file_id);
                }
            }
            tracing::debug!("skip Unit ordinal remap: Unit slots require geometry remap");
            continue;
        }
        if src_entries.len() > tgt_entries.len() {
            skipped_types.push(*tid);
            for e in src_entries[tgt_entries.len()..].iter() {
                skipped_file_ids.insert(e.file_id);
            }
        }
        for (src_e, tgt_e) in src_entries.iter().zip(tgt_entries.iter()) {
            if src_e.file_id != tgt_e.file_id {
                remap.insert(src_e.file_id, tgt_e.file_id);
            }
        }
        // Also discard tgt_by_type if it has types not in source — handled via
        // type_order iteration over both below.
        let _ = &tgt_by_type;
    }

    RemapPlan {
        remap,
        skipped_types,
        type_counts: counts,
        skipped_file_ids,
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RemapPlan {
    pub remap: HashMap<u64, u64>,
    pub skipped_types: Vec<u64>,
    pub type_counts: HashMap<u64, (usize, usize)>,
    pub skipped_file_ids: HashSet<u64>,
}

fn type_order(entries: &[TocEntry]) -> Vec<u64> {
    let mut order = Vec::new();
    for e in entries {
        if !order.contains(&e.type_id) {
            order.push(e.type_id);
        }
    }
    order
}

fn entries_by_type(entries: &[TocEntry]) -> HashMap<u64, Vec<&TocEntry>> {
    let mut out: HashMap<u64, Vec<&TocEntry>> = HashMap::new();
    for e in entries {
        out.entry(e.type_id).or_default().push(e);
    }
    out
}

/// Texture-ID logger helper used in verbose mode (Mode A).
#[allow(dead_code)]
pub(crate) fn list_referenced_source_texture_ids(
    patch: &StreamToc,
    source: &StreamToc,
) -> Vec<u64> {
    let source_textures: HashSet<u64> = source
        .entries
        .iter()
        .filter(|e| e.type_id == TEX_ID)
        .map(|e| e.file_id)
        .collect();
    let mut out: Vec<u64> = Vec::new();
    for entry in patch
        .entries
        .iter()
        .filter(|e| e.type_id == crate::constants::MATERIAL_ID)
    {
        for tex_id in refs::list_material_refs(&entry.toc_data) {
            if source_textures.contains(&tex_id) && !out.contains(&tex_id) {
                out.push(tex_id);
            }
        }
    }
    out
}

#[allow(dead_code)]
pub(crate) fn out_path(out_root: &Path, target_name: &str, patch_suffix: &str) -> PathBuf {
    out_root.join(safe_filename(target_name)).join(patch_suffix)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_filename_strips_forbidden_chars() {
        assert_eq!(safe_filename(r#"A:Foo/Bar*"<>?|"#), "A_Foo_Bar______");
        assert_eq!(safe_filename("trailing dots..."), "trailing dots");
    }
}
